#!/usr/bin/env python3
"""Collect controlled release ABBAs, then verify immutable evidence before tagging.

No synthetic benchmark mode is provided. Tests use explicit in-memory fixtures;
production capture invokes the four real benchmark entry points on both sources.
"""
from __future__ import annotations

from contextlib import ExitStack

import argparse
import hashlib
import json
import math
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import time
import copy

import release_sampling as sampling
from diagnostic_overlay import overlay

from check_benchmark_regression import PROVENANCE_FIELDS, index_records, provenance_value
from check_reconstruction_hotspot_gate import CASE_FIELDS, index_results

BASELINE_RELEASE = "9.0.0"
BASELINE_COMMIT = "9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6"
PHASES = ("A1", "B1", "B2", "A2")
FILES = {
    "smoke": "smoke-results.json",
    "small_file": "small-file-results.json",
    "hotspot": "reconstruction-hotspot-results.json",
    "stream": "stream-path-results.json",
}
VARIABLES = {
    "smoke": "RSE_SMOKE_BASELINE", "small_file": "RSE_SMALL_FILE_BASELINE",
    "hotspot": "RSE_RECONSTRUCTION_HOTSPOT_BASELINE", "stream": "RSE_STREAM_PATH_BASELINE",
}
REQUIRED = {
    "smoke": ("encode:4:2:65536", "encode:10:4:1048576", "verify:10:4:1048576", "reconstruct:10:4:1048576", "reconstruct_data:10:4:1048576"),
    "small_file": ("encode:4:2:1024", "verify_with_buffer:4:2:4096", "reconstruct:4:2:16384", "reconstruct_data:10:4:65536"),
    "stream": ("encode_stream:4:2:65536:65536", "verify_stream:4:2:65536:65536", "reconstruct_stream:10:4:1048576:1048576"),
    "hotspot": ("reconstruct_data_missing_1_data", "reconstruct_data_missing_2_data", "reconstruct_data_missing_data_plus_parity", "reconstruct_data_32x16_missing_2_data", "reconstruct_some_required_1_of_2_missing_data", "reconstruct_some_required_2_of_3_missing_data", "reconstruct_some_required_data_and_skip_parity", "reconstruct_some_32x16_required_2_of_4_missing_data"),
}
METRICS = {"smoke": "throughput_mb_s", "small_file": "ns_per_iter", "stream": "ns_per_block", "hotspot": "candidate_mb_s"}
KINDS = {key: name.removesuffix(".json") for key, name in FILES.items()}
REVISION = re.compile(r"[0-9a-f]{40}\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
MAX_BYTES = 32 * 1024 * 1024
BASE_ENV = {
    "RUSTUP_TOOLCHAIN": "stable", "RUSTFLAGS": "", "RAYON_NUM_THREADS": "4",
    "RS_PARALLEL_POLICY_MAX_JOBS": "4", "RSE_BACKEND_OVERRIDE": "auto",
    "RSE_STRICT_BACKEND_OVERRIDE": "1", "RSE_SMOKE_PROFILE": "extended",
    "RSE_SMALL_FILE_PROFILE": "fast", "RSE_STREAM_PROFILE": "fast", "RSE_STREAM_IO_MODE": "auto",
    "RSE_BENCHMARK_ALLOW_MISSING_CASES": "0", "CARGO_BUILD_JOBS": "4",
    "RSE_SMOKE_ITERATIONS": "200", "RSE_SMALL_FILE_ITERATIONS": "1000",
    "RSE_STREAM_ITERATIONS": "30", "RSE_TEST_BENCH_ITERATIONS": "100",
}

SAMPLING_ENV = {k: v for k, v in BASE_ENV.items() if not k.endswith("_ITERATIONS")}
SAMPLING_ENV["RSE_SAMPLER_PROTOCOL"] = f"duration-v{sampling.PROTOCOL['version']}"

# Complete exported profile catalogs; harness changes require an explicit review.
def expected_cases(profile):
    if profile == "hotspot":
        return set(REQUIRED[profile])
    if profile == "smoke":
        cases = [(4, 2, 65536), (10, 4, 1048576), (32, 16, 1048576)]
        operations = ("encode", "update", "verify", "reconstruct", "reconstruct_data")
    elif profile == "small_file":
        cases = [(4, 2, size * 1024) for size in (1, 4, 16, 64, 128, 256, 512)]
        cases += [(10, 4, size * 1024) for size in (16, 64, 256, 512)]
        operations = ("encode", "verify", "verify_with_buffer", "reconstruct", "reconstruct_opt", "reconstruct_shard_slot", "reconstruct_some_data_only", "reconstruct_data")
    else:
        cases = [(4, 2, 65536, 65536), (4, 2, 1048576, 262144), (10, 4, 65536, 65536), (10, 4, 1048576, 1048576), (10, 4, 16777216, 4194304)]
        operations = ("encode_stream", "verify_stream", "reconstruct_stream")
    return {":".join(map(str, (operation, *case))) for operation in operations for case in cases}


class EvidenceFailure(ValueError):
    def __init__(self, message, details):
        super().__init__(message)
        self.details = details


def command(args, root, env=None):
    return subprocess.check_output(args, cwd=root, env=env, text=True).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_data(path):
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= MAX_BYTES:
        raise ValueError("evidence must contain bounded regular JSON files")
    return json.loads(path.read_text(), parse_constant=lambda x: (_ for _ in ()).throw(ValueError("nonfinite JSON")))


def source_at(root, revision):
    commit = command(["git", "rev-parse", revision + "^{commit}"], root)
    return {"commit": commit, "tree": command(["git", "rev-parse", commit + "^{tree}"], root),
            "lock_sha256": digest(subprocess.check_output(["git", "show", commit + ":Cargo.lock"], cwd=root))}


def source_info(root):
    if command(["git", "status", "--porcelain", "--untracked-files=no"], root):
        raise ValueError("tracked source changes must be committed before measuring or verification")
    return source_at(root, "HEAD")


def harness_digest(root):
    paths = ["tests/benchmark_smoke.rs", "tests/benchmark_small_files.rs", "tests/benchmark_stream_paths.rs", "tests/common/mod.rs", "benches/common/mod.rs"]
    data = b"".join(p.encode() + b"\0" + (root / p).read_bytes() + b"\0" for p in paths)
    unit = (root / "src/tests/mod.rs").read_text()
    for name in ("struct ReconstructionHotspotBenchResult", "fn benchmark_test_iterations", "macro_rules! make_random_shards", "fn option_shards_with_missing", "fn shard_index_pattern", "fn assert_required_reconstruction_matches", "fn bench_reconstruct_data_hotspot", "fn bench_reconstruct_some_hotspot", "fn write_reconstruction_hotspot_bench_results", "fn benchmark_reconstruction_hotspots"):
        start = unit.index(name)
        end = unit.index("\n}", start) + 2
        data += unit[start:end].encode() + b"\0"
    return digest(data)


def cpu_set(value):
    result = set()
    for item in value.strip().split(","):
        bounds = [int(x) for x in item.split("-")]
        result.update(range(bounds[0], bounds[-1] + 1))
    return result


def isolation_context(group):
    group = group.resolve()
    actual = pathlib.Path("/sys/fs/cgroup") / pathlib.Path(pathlib.Path("/proc/self/cgroup").read_text().split("0::", 1)[1].strip()).relative_to("/")
    if group == pathlib.Path("/sys/fs/cgroup") or actual.resolve() != group:
        raise ValueError("capture must run inside the reserved child CPU partition")
    if (group / "cpuset.cpus.partition").read_text().strip() not in ("root", "isolated"):
        raise ValueError("a valid exclusive cgroup CPU partition is required")
    cpus = cpu_set((group / "cpuset.cpus.effective").read_text())
    if cpus != cpu_set((group / "cpuset.cpus.exclusive.effective").read_text()):
        raise ValueError("partition CPUs must be exclusive")
    if any(path.is_dir() for path in group.iterdir()):
        raise ValueError("nested workloads are not allowed in the capture partition")
    if cpus != set(os.sched_getaffinity(0)) or len(cpus) < 2:
        raise ValueError("capture affinity must match the reserved partition")
    for pid in (group / "cgroup.procs").read_text().split():
        current = int(pid)
        while current not in (0, 1, os.getpid()):
            try:
                current = int(pathlib.Path(f"/proc/{current}/stat").read_text().rsplit(") ", 1)[1].split()[1])
            except FileNotFoundError:
                current = 0
        if current == 1 and int(pid) != os.getpid():
            raise ValueError("another workload is present in the capture partition")
    return {"scope": "exclusive-vm-cpu-partition", "reserved_cpu_count": len(cpus), "cpu_affinity": sorted(cpus), "physical_host_exclusivity_proven": False}


def allocator_override_key(key):
    return key.startswith(("MALLOC_", "LD_")) or key == "GLIBC_TUNABLES"


def runtime_context(env):
    name, version = platform.libc_ver()
    if not name or not version:
        raise ValueError("libc identity must be available for release evidence")
    return {"libc": {"name": name, "version": version},
            "page_size": sampling.base_page_size(),
            "allocator_environment": {k: env[k] for k in sorted(env) if allocator_override_key(k)}}


def environment_context(root, env, group):
    isolation = isolation_context(group)
    cpu = next((line.split(":", 1)[1].strip() for line in pathlib.Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "")
    if not cpu:
        raise ValueError("CPU model must be available for release evidence")
    return {"compiler": command(["rustc", "--version"], root, env), "cpu_model": cpu,
            "isolation": isolation, "runtime": runtime_context(env), "settings": {k: env[k] for k in SAMPLING_ENV}}


def rows(profile, path, revision, iteration_counts=None):
    return indexed_rows(profile, json_data(path), revision, iteration_counts)


def indexed_rows(profile, original, revision, iteration_counts=None, complete=True):
    if profile == "hotspot":
        indexed = index_results(copy.deepcopy(original), METRICS[profile])
    else:
        normalized = index_records(original, "release evidence")
        indexed = {":".join(key).rstrip(":"): value for key, value in normalized.items()}
        for row in original:
            recorded = row.get("git_revision")
            if not isinstance(recorded, str) or not re.fullmatch(r"[0-9a-f]{7,40}", recorded) or not revision.startswith(recorded):
                raise ValueError("benchmark revision does not match the measured source")
    if (complete and set(indexed) != expected_cases(profile)) or not set(indexed).issubset(expected_cases(profile)):
        raise ValueError(f"{profile}: incomplete or unexpected release case catalog")
    for case, row in zip(indexed, original):
        if type(row.get("schema_version")) is not int or row["schema_version"] != 1 or row.get("benchmark_metrics_enabled") is not False:
            raise ValueError("release artifacts require schema 1 and disabled instrumentation")
        if profile == "hotspot" and any(field not in row for field in CASE_FIELDS):
            raise ValueError("missing hotspot workload fields")
        if profile != "hotspot":
            required = ("target_triple", "backend", "backend_id", "backend_kind", "backend_override", "override_honored", "features")
            if any(provenance_value(row, field) is None for field in required):
                raise ValueError("missing native benchmark provenance")
            # Schema 1 smoke exports no profile label. Its full case catalog,
            # exact iterations and captured settings bind the extended profile.
            expected_profile = "extended" if profile == "smoke" else "fast"
            if profile != "smoke" and row.get("profile") != expected_profile:
                raise ValueError("unexpected benchmark profile")
            if "profile" in row and row["profile"] != expected_profile:
                raise ValueError("unexpected benchmark profile")
            iteration_setting = {"smoke": "RSE_SMOKE_ITERATIONS", "small_file": "RSE_SMALL_FILE_ITERATIONS", "stream": "RSE_STREAM_ITERATIONS"}[profile]
            expected_iterations = iteration_counts[case] if iteration_counts is not None else int(BASE_ENV[iteration_setting])
            if type(row.get("iterations")) is not int or row["iterations"] != expected_iterations:
                raise ValueError("benchmark iterations differ from captured settings")
            if provenance_value(row, "features") != "simd-accel|std":
                raise ValueError("unexpected features or benchmark profile")
        if row.get("artifact_kind") != KINDS[profile]:
            raise ValueError("wrong benchmark artifact kind")
        number = row.get(METRICS[profile])
        if isinstance(number, bool) or not isinstance(number, (int, float)) or not math.isfinite(number) or number <= 0:
            raise ValueError("release metrics must be finite and positive")
    return indexed


def compatible(profile, left, right):
    if left.keys() != right.keys():
        raise ValueError("complete evidence case sets must match")
    for key in left:
        for field in PROVENANCE_FIELDS:
            if provenance_value(left[key], field) != provenance_value(right[key], field):
                raise ValueError(f"{profile}: incompatible {field}")
        if profile == "hotspot":
            for field in CASE_FIELDS:
                if left[key].get(field) != right[key].get(field):
                    raise ValueError(f"hotspot: incompatible {field}")


def safe_file(root, name):
    path = pathlib.PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or len(path.parts) != 2 or path.parts[0] not in PHASES + ("calibration-baseline", "calibration-candidate") or path.name not in set(FILES.values()) | {p + "-samples.json" for p in FILES}:
        raise ValueError("invalid evidence member path")
    current = root
    for part in path.parts:
        current /= part
        if current.is_symlink():
            raise ValueError("symlinks are not evidence members")
    return current


def checker_args(profile, baseline, current):
    scripts = pathlib.Path(__file__).resolve().parent
    if profile == "hotspot":
        args = [sys.executable, str(scripts / "check_reconstruction_hotspot_gate.py"), "--baseline", str(baseline), "--current", str(current)]
        for case in REQUIRED[profile]: args += ["--require-scenario", case]
        return args
    args = [sys.executable, str(scripts / "check_benchmark_regression.py"), "--baseline", str(baseline), "--current", str(current)]
    if profile != "smoke": args += ["--metric", METRICS[profile]]
    thresholds = {"small_file": ("encode=0.12", "verify=0.12", "verify_with_buffer=0.12", "reconstruct=0.18", "reconstruct_data=0.18"), "stream": ("encode_stream=0.15", "verify_stream=0.15", "reconstruct_stream=0.18")}
    for threshold in thresholds.get(profile, ()): args += ["--threshold", threshold]
    for case in REQUIRED[profile]: args += ["--require-case", case]
    return args


def verify(manifest, repository, compiler=True):
    document = json_data(manifest)
    if not isinstance(document, dict):
        raise ValueError("evidence manifest must be an object")
    for field in ("sources", "contexts", "files"):
        if not isinstance(document.get(field), dict):
            raise ValueError("invalid manifest container: " + field)
    if type(document.get("schema_version")) is not int or document["schema_version"] != 2 or document.get("phase_order") != list(PHASES):
        raise ValueError("sampled schema 2 A1/B1/B2/A2 evidence is required; legacy mean-only records cannot approve release")
    if document.get("baseline_release") != BASELINE_RELEASE:
        raise ValueError("release baseline must be the published 9.0.0")
    sources = document.get("sources", {})
    for role in ("baseline", "candidate"):
        if not isinstance(sources.get(role), dict) or any(not REVISION.fullmatch(sources[role].get(k, "")) for k in ("commit", "tree")):
            raise ValueError("sources must bind full commit and tree identifiers")
        if not DIGEST.fullmatch(sources[role].get("lock_sha256", "")):
            raise ValueError("sources must bind lockfile digests")
    baseline = sources["baseline"]
    if baseline["commit"] != BASELINE_COMMIT:
        raise ValueError("baseline commit is not published 9.0.0")
    if baseline != source_at(repository, BASELINE_COMMIT):
        raise ValueError("baseline source metadata differs from published 9.0.0")
    if sources["baseline"]["commit"] == sources["candidate"]["commit"] or sources["baseline"]["tree"] == sources["candidate"]["tree"]:
        raise ValueError("candidate/self comparison is not release evidence")
    actual = source_info(repository)
    if sources["candidate"]["tree"] != actual["tree"] or sources["candidate"]["lock_sha256"] != actual["lock_sha256"]:
        raise ValueError("evidence does not match the current candidate tree/lock")
    if document.get("harness_sha256") != harness_digest(repository):
        raise ValueError("benchmark harness differs from measured evidence")
    contexts = document.get("contexts", {})
    reference = contexts.get("A1")
    if not isinstance(reference, dict) or not isinstance(reference.get("isolation"), dict) or reference["isolation"].get("scope") != "exclusive-vm-cpu-partition":
        raise ValueError("controlled resource provenance is required")
    isolation = reference["isolation"]
    expected_env = SAMPLING_ENV
    if reference.get("settings") != expected_env or not reference.get("cpu_model") or not reference.get("compiler") or type(isolation.get("reserved_cpu_count")) is not int or isolation["reserved_cpu_count"] < 2 or isolation.get("physical_host_exclusivity_proven") is not False:
        raise ValueError("incomplete or unexpected controlled capture settings")
    runtime = reference.get("runtime")
    if not isinstance(runtime, dict) or set(runtime) != {"libc", "allocator_environment", "page_size"}:
        raise ValueError("libc, allocator and base page provenance is required")
    if not sampling.valid_page_size(runtime["page_size"]):
        raise ValueError("invalid recorded OS base page size")
    libc = runtime["libc"]
    if not isinstance(libc, dict) or set(libc) != {"name", "version"} or any(not isinstance(v, str) or not v for v in libc.values()):
        raise ValueError("invalid libc identity")
    allocator = runtime["allocator_environment"]
    if not isinstance(allocator, dict) or any(not allocator_override_key(k) or not isinstance(v, str) for k, v in allocator.items()):
        raise ValueError("invalid allocator environment")
    if any(contexts.get(phase) != reference for phase in PHASES + ("after",)):
        raise ValueError("measurement environment changed during the campaign")
    if compiler and reference.get("compiler") != command(["rustc", "--version"], repository, dict(os.environ, RUSTUP_TOOLCHAIN="stable")):
        raise ValueError("release compiler differs from measured evidence")
    root = manifest.parent
    data, paths = {}, {}
    if set(document.get("files", {})) != set(PHASES):
        raise ValueError("unexpected or missing measurement phases")
    for phase in PHASES:
        role = "baseline" if phase.startswith("A") else "candidate"
        data[phase], paths[phase] = {}, {}
        entries = document.get("files", {}).get(phase, {})
        if set(entries) != set(FILES): raise ValueError("every phase needs all four release profiles")
        for profile, filename in FILES.items():
            entry = entries[profile]
            if not isinstance(entry, dict):
                raise ValueError("evidence member must be an object")
            expected = f"{phase}/{filename}"
            if entry.get("path") != expected or not DIGEST.fullmatch(entry.get("sha256", "")):
                raise ValueError("invalid evidence member declaration")
            path = safe_file(root, expected)
            json_data(path)
            if digest(path.read_bytes()) != entry["sha256"]: raise ValueError("evidence digest mismatch")
            paths[phase][profile] = path
            counts = document.get("sampling", {}).get("common_iterations", {}).get(profile)
            data[phase][profile] = rows(profile, path, sources[role]["commit"], counts)
    sample_spreads = verify_sampling(document, root, repository, data)
    drifts = []
    for profile in FILES:
        for phase in PHASES[1:]: compatible(profile, data["A1"][profile], data[phase][profile])
        for first, second in (("A1", "A2"), ("B1", "B2")):
            for case, row in data[first][profile].items():
                old, new = float(row[METRICS[profile]]), float(data[second][profile][case][METRICS[profile]])
                drift = abs(new / old - 1)
                drifts.append({"profile": profile, "case": case, "pair": first + second, "drift": drift})
    if any(item["drift"] > .05 for item in drifts):
        raise EvidenceFailure("ABBA drift exceeds 5%; no candidate attribution or release approval",
                              {"drift_checks": drifts, "sample_spreads": sample_spreads, "candidate_attribution": None})
    for profile in FILES:
        for a in ("A1", "A2"):
            for b in ("B1", "B2"):
                subprocess.run(checker_args(profile, paths[a][profile], paths[b][profile]), cwd=repository, check=True, stdout=subprocess.DEVNULL)
    return document, paths, {"status": "passed", "candidate_commit": actual["commit"], "candidate_tree": actual["tree"], "measured_candidate_commit": sources["candidate"]["commit"], "baseline_commit": sources["baseline"]["commit"], "manifest_sha256": digest(manifest.read_bytes()), "drift_checks": len(drifts), "sample_distribution_checks": len(sample_spreads), "case_counts": {p: len(data["A1"][p]) for p in FILES}, "scope": "controlled recorded VM campaign; no new measurement on the verification runner"}


def evidence_member(root, entry, expected):
    if not isinstance(entry, dict) or entry.get("path") != expected or not DIGEST.fullmatch(entry.get("sha256", "")):
        raise ValueError("invalid sampling evidence declaration")
    path = safe_file(root, expected)
    result = json_data(path)
    if digest(path.read_bytes()) != entry["sha256"]:
        raise ValueError("sampling evidence digest mismatch")
    return result


def verify_sampling(document, root, repository, summarized):
    contract = document.get("sampling", {})
    expected = dict(sampling.PROTOCOL, implementation_sha256=sampling.implementation_digest(), overlay_sha256=sampling.overlay_digest(repository))
    if any(contract.get(k) != v for k, v in expected.items()):
        raise ValueError("sampling protocol/implementation mismatch")
    counts = contract.get("common_iterations", {})
    if set(counts) != set(FILES): raise ValueError("complete common iteration counts are required")
    pilot_counts = {role: {} for role in ("baseline", "candidate")}
    pilot_layouts = {role: {} for role in pilot_counts}
    for role in pilot_counts:
        for profile in FILES:
            name = f"calibration-{role}/{profile}-samples.json"
            records = evidence_member(root, document.get("calibration", {}).get(role, {}).get(profile), name)
            pilot_counts[role][profile] = calibration_counts(records, profile, document["sources"][role]["commit"], document["contexts"]["A1"]["runtime"]["page_size"])
            pilot_layouts[role][profile] = sampling.fixture_catalog(records, calibration=True)
    spreads = []
    layouts = {}
    for profile in FILES:
        layouts[profile] = {}
        expected_counts = {key: max(pilot_counts[role][profile][key] for role in pilot_counts) for key in expected_cases(profile)}
        if counts[profile] != expected_counts: raise ValueError("common iterations differ from complete calibration")
        for phase in PHASES:
            revision = document["sources"]["baseline" if phase.startswith("A") else "candidate"]["commit"]
            entry = document["files"][phase][profile].get("samples")
            records = evidence_member(root, entry, f"{phase}/{profile}-samples.json")
            normalize = lambda p, row, rev: next(iter(indexed_rows(p, [row], rev, counts[p], complete=False)))
            selected, widths = sampling.selected_row(records, profile, METRICS[profile], revision, expected_cases(profile), counts[profile], normalize, document["contexts"][phase]["isolation"].get("cpu_affinity"), document["contexts"][phase]["runtime"]["page_size"])
            layouts[profile][phase] = sampling.fixture_catalog(records)
            role = "baseline" if phase.startswith("A") else "candidate"
            if layouts[profile][phase] != pilot_layouts[role][profile]:
                raise ValueError(f"{profile}/{phase}: buffer layout differs from final calibration")
            recomputed = indexed_rows(profile, selected, revision, counts[profile])
            if recomputed != summarized[phase][profile]: raise ValueError("summary is not the median of all fixed samples")
            spreads.extend(dict(item, phase=phase, profile=profile) for item in widths)
        if any(layout != layouts[profile]["A1"] for layout in layouts[profile].values()):
            raise ValueError(f"{profile}: buffer layout differs between ABBA phases")
    unstable = [r for r in spreads if r["central_spread"] > sampling.PROTOCOL["central_spread_limit"]]
    if unstable:
        raise EvidenceFailure("sample distributions are unstable; no candidate attribution", {"sample_spreads": spreads, "candidate_attribution": None})
    return spreads


def calibration_counts(records, profile, revision, expected_page_size=None):
    groups = {}
    for record in records:
        key = record.get("case")
        count = record.get("iterations")
        if record.get("isolated_case") is not True or record.get("kind") not in ("calibration", "calibration_warmup") or record.get("revision") != revision or key not in expected_cases(profile) or type(count) is not int or not 0 < count <= 1_000_000_000:
            raise ValueError("invalid calibration sample")
        if not isinstance(record.get("rows"), list) or len(record["rows"]) != 1:
            raise ValueError("calibration must contain one raw row")
        sampling.validate_telemetry(record)
        sampling.validate_fixture(record, profile, expected_page_size)
        if set(indexed_rows(profile, record["rows"], revision, {key: count}, complete=False)) != {key}:
            raise ValueError("calibration row does not match declared case")
        sampling.validate_timing(record, profile, count)
        groups.setdefault(key, []).append(record)
    if set(groups) != expected_cases(profile): raise ValueError("calibration case catalog is incomplete")
    counts = {}
    width = sampling.PROTOCOL["warmups"] + sampling.PROTOCOL["samples"]
    for key, group in groups.items():
        if not 1 <= len(group) <= sampling.PROTOCOL["max_calibration_records"] or len(group) % width:
            raise ValueError("calibration rehearsal is incomplete or exceeds its budget")
        count = 1
        for offset in range(0, len(group), width):
            following = sampling.calibration_batch(group[offset:offset+width], count, offset//width+1)
            if following is None:
                if offset+width != len(group):
                    raise ValueError("extra calibration records after a complete rehearsal")
                counts[key] = count
            else:
                count = following
        if key not in counts:
            raise ValueError("calibration rehearsal is incomplete")

    return counts


def capture(baseline, candidate, output, group):
    if output.exists(): raise ValueError("use a new evidence directory; old observations are preserved")
    output.mkdir(parents=True)
    env = {k:v for k,v in os.environ.items() if not k.startswith(("RSE_", "RS_", "CARGO_PROFILE_")) and k not in ("CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
    env.update(SAMPLING_ENV)
    environment_context(candidate, env, group)
    sources = {"baseline": source_info(baseline), "candidate": source_info(candidate)}
    if sources["baseline"]["commit"] != BASELINE_COMMIT or sources["candidate"]["commit"] == BASELINE_COMMIT:
        raise ValueError("published baseline and a different candidate are required")
    harness = harness_digest(candidate)
    if harness_digest(baseline) != harness or sampling.overlay_digest(baseline) != sampling.overlay_digest(candidate):
        raise ValueError("both codecs require the same original harness and external overlay")
    contract = dict(sampling.PROTOCOL, implementation_sha256=sampling.implementation_digest(), overlay_sha256=sampling.overlay_digest(candidate))
    metadata = {"schema_version":2,"baseline_release":BASELINE_RELEASE,"phase_order":list(PHASES),"sources":sources,"harness_sha256":harness,"sampling":contract,"contexts":{},"files":{},"calibration":{}}
    (output/"capture-provenance.json").write_text(json.dumps({
        "purpose":"capture setup only; never release acceptance", "sources":sources,
        "harness_sha256":harness, "sampling":contract,
        "initial_context":environment_context(candidate,env,group)},indent=2)+"\n")
    with ExitStack() as overlays:
        for role, root in (("baseline", baseline), ("candidate", candidate)):
            overlays.enter_context(overlay(root, sampling.render(root)))
            args = ["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","--no-run"]
            with (output/(role+"-build.log")).open("w") as log:
                subprocess.run(args,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        def execute(root, folder, profile, mode, count_file=None):
            before = environment_context(root,env,group)
            log_path = folder/(profile+".log")
            try:
                records = sampling.run_profile(root,env,profile,mode,log_path,count_file,cases=expected_cases(profile))
            except BaseException:
                # run_profile closes and flushes its journal before propagating failure.
                # Preserve completed numeric rows without promoting them to members
                # of a complete/accepted manifest; raw logs are never removed.
                if log_path.exists():
                    text = log_path.read_text()
                    for kind, prefix in (("samples", "RSE_SAMPLE "), ("timing", "RSE_TIMING ")):
                        try:
                            partial = sampling.parse_output(text, prefix)
                            (folder/(profile+"-"+kind+".partial.json")).write_text(json.dumps(partial,indent=2)+"\n")
                        except (ValueError, OSError) as error:
                            # A malformed journal remains rejected and intact. Do not
                            # replace the original worker failure with a parser error.
                            (folder/(profile+"-"+kind+".error.json")).write_text(json.dumps({"status":"failed", "error":str(error)})+"\n")
                raise
            member=folder/(profile+"-samples.json")
            member.write_text(json.dumps(records,indent=2)+"\n")
            if environment_context(root,env,group)!=before: raise ValueError("measurement environment changed")
            return records, {"path":member.relative_to(output).as_posix(),"sha256":digest(member.read_bytes())}
        pilots={}
        for role,root in (("baseline",baseline),("candidate",candidate)):
            folder=output/("calibration-"+role);folder.mkdir()
            metadata["calibration"][role]={};pilots[role]={}
            for profile in FILES:
                records,entry=execute(root,folder,profile,"calibrate")
                metadata["calibration"][role][profile]=entry
                pilots[role][profile]=calibration_counts(records,profile,sources[role]["commit"])
        counts={p:{k:max(pilots[r][p][k] for r in pilots) for k in expected_cases(p)} for p in FILES}
        contract["common_iterations"]=counts
        count_files={}
        for profile in FILES:
            count_files[profile]=output/("counts-"+profile+".tsv")
            count_files[profile].write_text("".join(f"{key}\t{value}\n" for key,value in sorted(counts[profile].items())))
        for phase in PHASES:
            time.sleep(10)
            role="baseline" if phase.startswith("A") else "candidate"
            root=baseline if role=="baseline" else candidate
            metadata["contexts"][phase]=environment_context(root,env,group)
            folder=output/phase;folder.mkdir();metadata["files"][phase]={}
            for profile,filename in FILES.items():
                records,entry=execute(root,folder,profile,"sample",count_files[profile])
                normalize=lambda p,row,rev: next(iter(indexed_rows(p,[row],rev,counts[p],complete=False)))
                selected,_=sampling.selected_row(records,profile,METRICS[profile],sources[role]["commit"],expected_cases(profile),counts[profile],normalize)
                result=folder/filename;result.write_text(json.dumps(selected,indent=2)+"\n")
                metadata["files"][phase][profile]={"path":f"{phase}/{filename}","sha256":digest(result.read_bytes()),"samples":entry}
                (output/"manifest.partial.json").write_text(json.dumps(metadata,indent=2)+"\n")
        metadata["contexts"]["after"]=environment_context(candidate,env,group)
        if any(source_info(root)!=sources[role] for role,root in (("baseline",baseline),("candidate",candidate))): raise ValueError("source changed during capture")
        manifest=output/"manifest.json";manifest.write_text(json.dumps(metadata,indent=2)+"\n")
        _,_,report=verify(manifest,candidate)
        (output/"report.json").write_text(json.dumps(report,indent=2)+"\n")


def materialize(manifest, repository, env_file):
    _, paths, report = verify(manifest, repository)
    destination = repository / "target/release-preflight/inputs"
    destination.mkdir(parents=True, exist_ok=True)
    current = repository / "target/benchmark-smoke"
    current.mkdir(parents=True, exist_ok=True)
    values = {}
    for profile, filename in FILES.items():
        shutil.copyfile(paths["A1"][profile], destination / filename)
        shutil.copyfile(paths["B2"][profile], current / filename)
        values[VARIABLES[profile]] = str(destination / filename)
    if any("\n" in value or "\r" in value for value in values.values()): raise ValueError("invalid output path")
    env_file.write_text("".join(f"{name}={value}\n" for name, value in values.items()))
    (destination.parent / "evidence-validation.json").write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    cap = sub.add_parser("capture")
    cap.add_argument("--baseline", type=pathlib.Path, required=True)
    cap.add_argument("--candidate", type=pathlib.Path, required=True)
    cap.add_argument("--output", type=pathlib.Path, required=True)
    cap.add_argument("--isolation-cgroup", type=pathlib.Path, required=True)
    for mode in ("verify", "materialize"):
        child = sub.add_parser(mode)
        child.add_argument("--manifest", type=pathlib.Path, required=True)
        child.add_argument("--repository", type=pathlib.Path, default=pathlib.Path.cwd())
        if mode == "materialize": child.add_argument("--env-file", type=pathlib.Path, required=True)
        else: child.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    capture_output_was_absent = args.mode == "capture" and not args.output.exists()
    try:
        if args.mode == "capture": capture(args.baseline.resolve(), args.candidate.resolve(), args.output.resolve(), args.isolation_cgroup)
        elif args.mode == "materialize": materialize(args.manifest.resolve(), args.repository.resolve(), args.env_file)
        else:
            _, _, report = verify(args.manifest.resolve(), args.repository.resolve())
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(report, indent=2) + "\n")
        print("release evidence verified")
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError, SystemExit) as error:
        # Never leave a stale passing report for a failed invocation.
        report_path = getattr(args, "report", None)
        if capture_output_was_absent and args.output.is_dir():
            report_path = args.output / "report.json"
        if args.mode == "materialize":
            report_path = args.repository / "target/release-preflight/evidence-validation.json"
        if report_path:
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps({"status": "failed", "error": str(error), **getattr(error, "details", {})}) + "\n")
        parser.exit(1, f"release evidence rejected: {error}\n")


if __name__ == "__main__": main()
