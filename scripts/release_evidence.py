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
import os
import pathlib
import platform
import re
import shutil
import subprocess
import time

import release_sampling as sampling
import native_tools
from diagnostic_overlay import overlay

from native_tools import PROVENANCE_FIELDS, index_records, provenance_value, CASE_FIELDS, index_results

SCHEMA_VERSION = 4
QUALIFICATION_SCHEMA = "baseline-qualification-v2"
DIAGNOSTIC_SCHEMA = "complete-campaign-diagnostic-v1"
LAYOUT_DIRECTORY = "layout-pair"
LAYOUT_CASES = tuple(f"verify_with_buffer:4:2:{size}" for size in (1024, 4096))
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
    "hotspot": tuple(row[2] for row in sampling.HOTSPOT_CASES),
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
_CASE_CATALOG = {profile: frozenset(cases) for profile, cases in native_tools.sampling_catalog()['cases'].items()}


def expected_cases(profile):
    return set(_CASE_CATALOG[profile])


class EvidenceFailure(ValueError):
    def __init__(self, message, details):
        super().__init__(message)
        self.details = details


def command(args, root, env=None):
    return subprocess.check_output(args, cwd=root, env=env, text=True).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def campaign_analysis(data, sample_spreads, diagnostic):
    # Pairs preserve each phase's original case order across JSON object sorting.
    projected={phase:{profile:list(rows.items()) for profile,rows in profiles.items()}
               for phase,profiles in data.items()}
    return native_tools.request('baseline-analysis', request=dict(operation='campaign', request=dict(
        data=projected, sample_spreads=sample_spreads if diagnostic else [], diagnostic=diagnostic)))


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
    for name in ("struct ReconstructionHotspotBenchResult", "fn benchmark_test_iterations", "macro_rules! make_random_shards", "pub fn fill_random", "fn option_shards_with_missing", "fn shard_index_pattern", "fn assert_required_reconstruction_matches", "fn bench_reconstruct_data_hotspot", "fn bench_reconstruct_some_hotspot", "fn write_reconstruction_hotspot_bench_results", "fn benchmark_reconstruction_hotspots"):
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


def validate_runtime(runtime):
    native_tools.validate_runtime(runtime)


def runtime_context(env):
    name, version = platform.libc_ver()
    if not name or not version:
        raise ValueError("libc identity must be available for release evidence")
    result = {"libc": {"name": name, "version": version},
            "page_size": sampling.base_page_size(),
            "kernel_memory_policy": native_tools.capture_memory_policy(),
            "allocator_environment": {k: env[k] for k in sorted(env) if allocator_override_key(k)}}

    validate_runtime(result)
    return result


def cpu_identity(text, machine, reserved_cpus):
    return native_tools.request('cpu-identity', text=text, machine=machine, reserved_cpus=list(reserved_cpus))


def environment_context(root, env, group):
    isolation = isolation_context(group)
    cpu = cpu_identity(pathlib.Path("/proc/cpuinfo").read_text(), platform.machine(), isolation["cpu_affinity"])
    return {"compiler": command(["rustc", "--version"], root, env), "cpu_model": cpu,
            "isolation": isolation, "runtime": runtime_context(env), "settings": {k: env[k] for k in SAMPLING_ENV}}


def rows(profile, path, revision, iteration_counts=None):
    return indexed_rows(profile, json_data(path), revision, iteration_counts)


def indexed_rows(profile, original, revision, iteration_counts=None, complete=True):
    defaults={'smoke':'RSE_SMOKE_ITERATIONS','small_file':'RSE_SMALL_FILE_ITERATIONS','stream':'RSE_STREAM_ITERATIONS'}
    default=int(BASE_ENV[defaults[profile]]) if profile in defaults and iteration_counts is None else None
    normalized=native_tools.index_release_rows(profile=profile, rows=original, revision=revision,
        iteration_counts=iteration_counts, default_iterations=default, complete=complete)
    return {(row['scenario'] if profile=='hotspot' else ':'.join(row[k] for k in
             ('operation','data_shards','parity_shards','shard_size','stream_block_size')).rstrip(':')):row
            for row in normalized}


def compatible(profile, left, right):
    native_tools.request('compatible-rows', profile=profile, left=left, right=right)


def observation_contracts(profile, records, reference=None):
    """Validate the complete batch natively; retain original read-only references."""
    return native_tools.observation_contracts(profile, records, reference)


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
    # Offline evidence directories need not contain Cargo aliases or a workspace.
    # Select the controller's tool explicitly, as the former Python paths did.
    manifest = pathlib.Path(__file__).resolve().parents[1] / 'xtask/Cargo.toml'
    command = ['cargo', 'run', '--quiet', '--locked', '--offline', '--manifest-path', str(manifest), '--package', 'xtask', '--']
    if profile == "hotspot":
        args = command + ["check-hotspot", "--baseline", str(baseline), "--current", str(current)]
        for case in REQUIRED[profile]: args += ["--require-scenario", case]
        return args
    args = command + ["check-benchmark", "--baseline", str(baseline), "--current", str(current)]
    if profile != "smoke": args += ["--metric", METRICS[profile]]
    thresholds = {"small_file": ("encode=0.12", "verify=0.12", "verify_with_buffer=0.12", "reconstruct=0.18", "reconstruct_data=0.18"), "stream": ("encode_stream=0.15", "verify_stream=0.15", "reconstruct_stream=0.18")}
    for threshold in thresholds.get(profile, ()): args += ["--threshold", threshold]
    for case in REQUIRED[profile]: args += ["--require-case", case]
    return args


def validate_envelope(document):
    if not isinstance(document, dict):
        raise ValueError("evidence manifest must be an object")
    for field in ("sources", "contexts", "files"):
        if not isinstance(document.get(field), dict):
            raise ValueError("invalid manifest container: " + field)
    if type(document.get("schema_version")) is not int or document["schema_version"] != SCHEMA_VERSION or document.get("phase_order") != list(PHASES):
        raise ValueError("sampled schema 4 with kernel memory policy, complete ABBA and paired layouts is required; older envelopes cannot approve release")


def layout_member(document, root, *, diagnostic=False):
    entry = document.get('layout_supplement')
    expected = LAYOUT_DIRECTORY+('/diagnostic-manifest.json' if diagnostic else '/manifest.json')
    if (not isinstance(entry, dict) or entry.get('path') != expected
            or not isinstance(entry.get('sha256'), str) or not DIGEST.fullmatch(entry['sha256'])):
        raise ValueError('a bound complete paired-layout supplement is required')
    if (root/LAYOUT_DIRECTORY).is_symlink():
        raise ValueError('paired-layout supplement must not be symlinked')
    path = root/expected
    json_data(path)
    if digest(path.read_bytes()) != entry['sha256']:
        raise ValueError('paired-layout supplement digest mismatch')
    return path


def layout_iterations(sampling_contract):
    counts = sampling_contract.get('common_iterations', {}).get('small_file', {})
    values = [counts.get(case) for case in LAYOUT_CASES]
    if any(type(n) is not int or not 1 <= n <= 1_000_000_000 for n in values):
        raise ValueError('layout iterations require both completed buffered-verify calibration counts')
    return max(values)


def verify(manifest, repository, compiler=True):
    document = json_data(manifest)
    validate_envelope(document)
    # Refuse a missing/broken supplement before running any regression programs.
    layout_path = layout_member(document, manifest.parent)
    paths, data, report = _verify_profiles(document, manifest.parent, repository, compiler)
    from diagnose_verify_layout import verify_paired
    layout = verify_paired(layout_path.parent, repository, compiler)
    verify_layout_binding(document, data, layout)
    if not layout['layout_gate_passed']:
        raise EvidenceFailure('paired layouts failed; no release approval', {'layout': layout, 'candidate_attribution': None})
    report.update(status='passed', manifest_sha256=digest(manifest.read_bytes()),
                  layout_manifest_sha256=document['layout_supplement']['sha256'],
                  layout_distribution_checks=sum(r['case_count'] for r in layout['phase_reports'].values()),
                  layout_repeat_checks=len(layout['repeat_checks'])+sum(len(r['repeat_checks']) for r in layout['phase_reports'].values()),
                  layout_comparisons=len(layout['candidate_attribution']))
    return document, paths, report


def verify_layout_binding(document, data, layout):
    if any(layout['tool'][key] != document['sources']['candidate'][key] for key in ('tree', 'lock_sha256')):
        raise ValueError('paired-layout tool does not match the frozen candidate tree/lock')
    if layout['sources'] != document['sources'] or layout['context'] != document['contexts']['A1']:
        raise ValueError('paired-layout source or resource context differs from the full campaign')
    if layout['iterations'] != layout_iterations(document['sampling']):
        raise ValueError('paired-layout iterations differ from the frozen full-campaign calibration')
    native = layout['phase_reports']['A1']['provenance']
    fields = ('backend', 'backend_id', 'backend_kind', 'features')
    target_prefix = native['target_arch']+'-'+native['target_os']+'-'
    for case in LAYOUT_CASES:
        row = data['A1']['small_file'][case]
        if (any(provenance_value(row, field) != native[field] for field in fields)
                or not provenance_value(row, 'target_triple').startswith(target_prefix)):
            raise ValueError(case+': paired-layout backend/features/target differ from the original buffered-verify cases')


def verify_diagnostic(manifest, repository, compiler=True):
    """Recompute every check after complete collection; never release evidence."""
    document = json_data(manifest)
    if (not isinstance(document, dict) or document.get('schema_version') != DIAGNOSTIC_SCHEMA
            or document.get('phase_order') != list(PHASES)
            or document.get('diagnostic_only') is not True
            or document.get('release_approval') is not False):
        raise ValueError('a complete diagnostic manifest is required')
    layout_path = layout_member(document, manifest.parent, diagnostic=True)
    invocations = verify_worker_journal(document, manifest.parent)
    paths, data, report = _verify_profiles(document, manifest.parent, repository, compiler, diagnostic=True)
    from diagnose_verify_layout import verify_paired
    layout = verify_paired(layout_path.parent, repository, compiler, diagnostic=True)
    verify_layout_binding(document, data, layout)
    report.update(status='diagnostic_complete', diagnostic_only=True, release_approval=False,
                  candidate_attribution=None, all_checks_executed=True,
                  manifest_sha256=digest(manifest.read_bytes()), layout=layout,
                  worker_invocations=invocations,
                  all_original_limits_passed=report['all_original_limits_passed'] and layout['layout_gate_passed'])
    return document, paths, report


def verify_worker_journal(document, root):
    entry = document.get('worker_journal', {})
    path = root/'worker-journal.jsonl'
    if (not isinstance(entry, dict) or entry.get('path') != path.name or path.is_symlink() or not path.is_file()
            or not 0 < path.stat().st_size <= MAX_BYTES
            or digest(path.read_bytes()) != entry.get('sha256')):
        raise ValueError('complete digest-bound worker journal is required')
    expected = []
    for phase in ('calibration-baseline', 'calibration-candidate', *PHASES):
        calibration = phase.startswith('calibration-')
        for profile in FILES:
            member = (document['calibration'][phase.removeprefix('calibration-')][profile] if calibration
                      else document['files'][phase][profile]['samples'])
            records = evidence_member(root, member, phase+'/'+profile+'-samples.json')
            expected.extend({'phase': phase, 'profile': profile, 'mode': 'calibrate' if calibration else 'sample',
                             'case': r['case'], 'iterations': r['iterations'], 'calibration_batch': r['calibration_batch']}
                            for r in records if r['index'] == 0)
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    if len(rows) != 2*len(expected):
        raise ValueError('complete start/end worker journal is required')
    previous = 0
    for index, identity in enumerate(expected):
        for position, event in enumerate(('start', 'end')):
            row = rows[2*index+position]
            keys = set(identity) | {'event', 'monotonic_ns', 'utc_ns'} | ({'exit_code'} if position else set())
            if (not isinstance(row, dict) or set(row) != keys or row['event'] != event or any(row.get(k) != v for k,v in identity.items())
                    or any(type(row[k]) is not int or row[k] < 0 for k in ('monotonic_ns', 'utc_ns'))
                    or row['monotonic_ns'] < previous or position and (type(row['exit_code']) is not int or row['exit_code'] != 0)):
                raise ValueError('worker journal order, identity or outcome differs from original records')
            previous = row['monotonic_ns']
    return len(expected)


def diagnostic_rows(records, profile, metric, revision, expected, counts,
                    allowed_cpus=None, expected_page_size=None):
    """Use the original nine-sample median; record short windows without trimming."""
    return sampling.selected_row(records, profile, metric, revision, expected, counts,
                                 allowed_cpus, expected_page_size, diagnostic=True)


def _verify_profiles(document, root, repository, compiler=True, *, diagnostic=False):
    """Validate original full profiles; this partial result cannot approve release."""
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
    validate_runtime(runtime)
    if any(contexts.get(phase) != reference for phase in PHASES + ("after",)):
        raise ValueError("measurement environment changed during the campaign")
    if compiler and reference.get("compiler") != command(["rustc", "--version"], repository, dict(os.environ, RUSTUP_TOOLCHAIN="stable")):
        raise ValueError("release compiler differs from measured evidence")
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
    sample_spreads = verify_sampling(document, root, repository, data, diagnostic=diagnostic)
    analysis = campaign_analysis(data, sample_spreads, diagnostic)
    drifts = analysis['repeat_checks']
    if not diagnostic and any(item["drift"] > sampling.REPEAT_DRIFT_LIMIT for item in drifts):
        raise EvidenceFailure("ABBA drift exceeds 5%; no candidate attribution or release approval",
                              {"drift_checks": drifts, "sample_spreads": sample_spreads, "candidate_attribution": None})
    checks = []
    for profile in FILES:
        for a in ("A1", "A2"):
            for b in ("B1", "B2"):
                args = checker_args(profile, paths[a][profile], paths[b][profile])
                if diagnostic:
                    result = subprocess.run(args, cwd=repository, check=False, capture_output=True, text=True)
                    checks.append({'profile': profile, 'pair': a+'/'+b, 'exit_code': result.returncode,
                                   'stdout': result.stdout, 'stderr': result.stderr})
                else:
                    subprocess.run(args, cwd=repository, check=True, stdout=subprocess.DEVNULL)
    if diagnostic:
        analysis['regression_checks']=checks
        analysis['all_original_limits_passed'] &= not any(r['exit_code'] for r in checks)
        return paths, data, analysis
    return paths, data, {"status": "profiles_verified", "candidate_commit": actual["commit"], "candidate_tree": actual["tree"], "measured_candidate_commit": sources["candidate"]["commit"], "baseline_commit": sources["baseline"]["commit"], "drift_checks": len(drifts), "sample_distribution_checks": len(sample_spreads), "case_counts": {p: len(data["A1"][p]) for p in FILES}, "scope": "controlled recorded VM campaign; no new measurement on the verification runner"}


def evidence_member(root, entry, expected):
    if not isinstance(entry, dict) or entry.get("path") != expected or not DIGEST.fullmatch(entry.get("sha256", "")):
        raise ValueError("invalid sampling evidence declaration")
    path = safe_file(root, expected)
    result = json_data(path)
    if digest(path.read_bytes()) != entry["sha256"]:
        raise ValueError("sampling evidence digest mismatch")
    return result


def verify_sampling(document, root, repository, summarized, *, diagnostic=False):
    contract = document.get("sampling", {})
    expected = dict(sampling.PROTOCOL, implementation_sha256=sampling.implementation_digest(), overlay_sha256=sampling.overlay_digest(repository))
    if any(contract.get(k) != v for k, v in expected.items()):
        raise ValueError("sampling protocol/implementation mismatch")
    counts = contract.get("common_iterations", {})
    if set(counts) != set(FILES): raise ValueError("complete common iteration counts are required")
    pilot_counts = {role: {} for role in ("baseline", "candidate")}
    pilot_layouts = {role: {} for role in pilot_counts}
    pilot_workloads = {role: {} for role in pilot_counts}
    pilot_traces = {role: {} for role in pilot_counts}
    pilot_contracts = {role: {} for role in pilot_counts}
    for role in pilot_counts:
        for profile in FILES:
            name = f"calibration-{role}/{profile}-samples.json"
            records = evidence_member(root, document.get("calibration", {}).get(role, {}).get(profile), name)
            pilot_counts[role][profile] = calibration_counts(records, profile, document["sources"][role]["commit"], document["contexts"]["A1"]["runtime"]["page_size"], allowed_cpus=document["contexts"]["A1"]["isolation"]["cpu_affinity"])
            pilot_contracts[role][profile] = observation_contracts(profile, [r for r in records if r['complete']],
                pilot_contracts['baseline'][profile] if role == 'candidate' else None)
            pilot_layouts[role][profile] = sampling.fixture_catalog(records, calibration=True)
            pilot_workloads[role][profile] = sampling.workload_catalog(records, calibration=True)
            pilot_traces[role][profile] = sampling.workload_traces(records, calibration=True)
    spreads = []
    layouts = {}
    for profile in FILES:
        layouts[profile] = {}
        workloads, traces = {}, {}
        expected_counts = {key: max(pilot_counts[role][profile][key] for role in pilot_counts) for key in expected_cases(profile)}
        if counts[profile] != expected_counts: raise ValueError("common iterations differ from complete calibration")
        for phase in PHASES:
            revision = document["sources"]["baseline" if phase.startswith("A") else "candidate"]["commit"]
            entry = document["files"][phase][profile].get("samples")
            records = evidence_member(root, entry, f"{phase}/{profile}-samples.json")
            role = "baseline" if phase.startswith("A") else "candidate"
            select = diagnostic_rows if diagnostic else sampling.selected_row
            selected, widths = select(records, profile, METRICS[profile], revision, expected_cases(profile), counts[profile], document["contexts"][phase]["isolation"].get("cpu_affinity"), document["contexts"][phase]["runtime"]["page_size"])
            observation_contracts(profile, records, pilot_contracts[role][profile])
            layouts[profile][phase] = sampling.fixture_catalog(records)
            workloads[phase] = sampling.workload_catalog(records)
            traces[phase] = sampling.workload_traces(records)
            if workloads[phase] != pilot_workloads[role][profile]:
                raise ValueError(f"{profile}/{phase}: preparation or initial input state differs from final calibration")
            for case, trace in traces[phase].items():
                if pilot_counts[role][profile][case] == expected_counts[case] and trace != pilot_traces[role][profile][case]:
                    raise ValueError(f"{profile}/{phase}: replayed random input differs from final calibration")
            if layouts[profile][phase] != pilot_layouts[role][profile]:
                raise ValueError(f"{profile}/{phase}: buffer layout differs from final calibration")
            recomputed = indexed_rows(profile, selected, revision, counts[profile])
            if recomputed != summarized[phase][profile]: raise ValueError("summary is not the median of all fixed samples")
            spreads.extend(dict(item, phase=phase, profile=profile) for item in widths)
        if any(workload != workloads["A1"] for workload in workloads.values()) or any(trace != traces["A1"] for trace in traces.values()):
            raise ValueError(f"{profile}: API/preparation or replayed random input differs between ABBA phases")
        if any(layout != layouts[profile]["A1"] for layout in layouts[profile].values()):
            raise ValueError(f"{profile}: buffer layout differs between ABBA phases")
    unstable = [r for r in spreads if r["central_spread"] > sampling.PROTOCOL["central_spread_limit"]]
    if unstable and not diagnostic:
        raise EvidenceFailure("sample distributions are unstable; no candidate attribution", {"sample_spreads": spreads, "candidate_attribution": None})
    return spreads


def calibration_counts(records, profile, revision, expected_page_size=None, *, allowed_cpus=None):
    return dict(native_tools.sampling_request('calibration-profile', records=records, profile=profile,
                                            revision=revision, expected_page_size=expected_page_size,
                                            allowed_cpus=None if allowed_cpus is None else list(allowed_cpus)))


def capture_environment():
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("RSE_", "RS_", "CARGO_PROFILE_"))
           and k not in ("CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
    return dict(env, **SAMPLING_ENV)


def prepare_capture_source(overlays, root, env, log_path, *, render=None):
    overlays.enter_context(overlay(root, (render or sampling.render)(root)))
    args = ["cargo", "test", "--locked", "--release", "--features", "std simd-accel",
            "--test", "rse_release_sampling", "--no-run"]
    with log_path.open("x") as log:
        subprocess.run(args, cwd=root, env=sampling.build_environment(root, env),
                       stdout=log, stderr=subprocess.STDOUT, check=True)


def baseline_service_times(records):
    return native_tools.request('baseline-analysis', request=dict(operation='service-times', records=records))


def qualify_baseline(baseline, output, group):
    """One full-catalog same-version control; never a release manifest or retry loop."""
    if output.exists(): raise ValueError("use a new qualification directory; old observations are preserved")
    output.mkdir(parents=True)
    tool = pathlib.Path(__file__).resolve().parents[1]
    source = source_info(baseline)
    if source["commit"] != BASELINE_COMMIT: raise ValueError("qualification requires published 9.0.0")
    tool_source = source_info(tool)
    if harness_digest(baseline) != harness_digest(tool) or sampling.overlay_digest(baseline) != sampling.overlay_digest(tool):
        raise ValueError("qualification requires the same original harness and external overlay")
    env = capture_environment()
    context = environment_context(baseline, env, group)
    report = {"schema_version": QUALIFICATION_SCHEMA, "source": source, "tool": tool_source,
              "context": context, "sampling": dict(sampling.PROTOCOL,
                  implementation_sha256=sampling.implementation_digest(), overlay_sha256=sampling.overlay_digest(baseline)),
              "harness_sha256": harness_digest(baseline), "calibration": {}, "phases": {},
              "repeat_checks": [], "baseline_controls_passed": False,
              "release_approval": False, "candidate_attribution": None}
    def save(name, value): (output/name).write_text(json.dumps(value, indent=2)+"\n")
    def unchanged():
        if source_info(baseline) != source or source_info(tool) != tool_source:
            raise ValueError("qualification source changed")
        if environment_context(baseline, env, group) != context:
            raise ValueError("qualification context changed")
    save("provenance.json", report)
    counts, fixtures, workloads, traces, medians, calibrated, contracts = {}, {}, {}, {}, {}, {}, {}
    with ExitStack() as overlays:
        prepare_capture_source(overlays, baseline, env, output/"baseline-build.log")
        folder = output/"calibration"; folder.mkdir()
        for profile in FILES:
            records, entry = capture_profile(baseline, folder, profile, "calibrate", env, output, group)
            counts[profile] = calibration_counts(records, profile, source["commit"], context["runtime"]["page_size"], allowed_cpus=context["isolation"]["cpu_affinity"])
            final_rows = {row["case"]: row["rows"][0] for row in records}
            calibrated[profile] = indexed_rows(profile, list(final_rows.values()), source["commit"], counts[profile])
            contracts[profile] = observation_contracts(profile, [r for r in records if r["complete"]])
            fixtures[profile] = sampling.fixture_catalog(records, calibration=True)
            workloads[profile] = sampling.workload_catalog(records, calibration=True)
            traces[profile] = sampling.workload_traces(records, calibration=True)
            report["calibration"][profile] = entry
            (output/("counts-"+profile+".tsv")).write_text("".join(f"{case}\t{count}\n" for case, count in sorted(counts[profile].items())))
            unchanged(); save("qualification.partial.json", report)
        report["sampling"]["common_iterations"] = counts
        for phase in ("A1", "A2"):
            time.sleep(10)
            folder = output/phase; folder.mkdir()
            report["phases"][phase] = {}; medians[phase] = {}
            for profile in FILES:
                records, entry = capture_profile(baseline, folder, profile, "sample", env, output, group, output/("counts-"+profile+".tsv"))
                selected, widths = sampling.selected_row(records, profile, METRICS[profile], source["commit"],
                    expected_cases(profile), counts[profile], context["isolation"]["cpu_affinity"], context["runtime"]["page_size"])
                observation_contracts(profile, records, contracts[profile])
                if (sampling.fixture_catalog(records) != fixtures[profile]
                        or sampling.workload_catalog(records) != workloads[profile]
                        or sampling.workload_traces(records) != traces[profile]):
                    raise ValueError(f"{phase}/{profile}: caller layout, preparation or input replay differs from calibration")
                medians[phase][profile] = indexed_rows(profile, selected, source["commit"], counts[profile])
                compatible(profile, calibrated[profile], medians[phase][profile])
                report["phases"][phase][profile] = {"samples": entry, "case_count": len(selected), "sample_spreads": widths,
                    "service_times": baseline_service_times(records)}
                unchanged(); save("qualification.partial.json", report)
            if any(w["central_spread"] > sampling.PROTOCOL["central_spread_limit"]
                   for item in report["phases"][phase].values() for w in item["sample_spreads"]):
                raise EvidenceFailure("baseline distributions are unstable; stop without candidate attribution", report)
        for profile in FILES:
            compatible(profile, medians["A1"][profile], medians["A2"][profile])
            for case, row in medians["A1"][profile].items():
                old, new = float(row[METRICS[profile]]), float(medians["A2"][profile][case][METRICS[profile]])
                report["repeat_checks"].append({"profile": profile, "case": case, "first": old, "second": new,
                                                "metric": METRICS[profile], "drift": abs(new-old)/old})
        unchanged()
        save("qualification.partial.json", report)
        if any(item["drift"] > sampling.REPEAT_DRIFT_LIMIT for item in report["repeat_checks"]):
            raise EvidenceFailure("baseline drift exceeds 5%; no candidate attribution", report)
    # A clean overlay close is part of successful qualification, not an afterthought.
    unchanged()
    report["baseline_controls_passed"] = True
    save("qualification.json", report)
    return report


def capture_profile(root, folder, profile, mode, env, output, group, count_file=None, *, diagnostic=False):
    before = environment_context(root,env,group)
    log_path = folder/(profile+".log")
    def journal(event):
        with (output/'worker-journal.jsonl').open('a') as stream:
            stream.write(json.dumps(dict(event, phase=folder.name, profile=profile))+'\n')
    try:
        options = {'journal': journal} if diagnostic else {}
        records = sampling.run_profile(root,env,profile,mode,log_path,count_file,cases=expected_cases(profile), **options)
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


def capture(baseline, candidate, output, group, *, diagnostic=False):
    if output.exists(): raise ValueError("use a new evidence directory; old observations are preserved")
    output.mkdir(parents=True)
    env = capture_environment()
    environment_context(candidate, env, group)
    sources = {"baseline": source_info(baseline), "candidate": source_info(candidate)}
    if sources["baseline"]["commit"] != BASELINE_COMMIT or sources["candidate"]["commit"] == BASELINE_COMMIT:
        raise ValueError("published baseline and a different candidate are required")
    tool = source_info(pathlib.Path(__file__).resolve().parents[1])
    if any(tool[key] != sources['candidate'][key] for key in ('tree', 'lock_sha256')):
        raise ValueError('capture tool must match the frozen candidate tree/lock')
    harness = harness_digest(candidate)
    if harness_digest(baseline) != harness or sampling.overlay_digest(baseline) != sampling.overlay_digest(candidate):
        raise ValueError("both codecs require the same original harness and external overlay")
    contract = dict(sampling.PROTOCOL, implementation_sha256=sampling.implementation_digest(), overlay_sha256=sampling.overlay_digest(candidate))
    metadata = {"schema_version":SCHEMA_VERSION,"baseline_release":BASELINE_RELEASE,"phase_order":list(PHASES),"sources":sources,"harness_sha256":harness,"sampling":contract,"contexts":{},"files":{},"calibration":{}}
    partial_name = 'diagnostic-manifest.partial.json' if diagnostic else 'manifest.partial.json'
    if diagnostic:
        metadata.update(schema_version=DIAGNOSTIC_SCHEMA, diagnostic_only=True, release_approval=False)
    (output/"capture-provenance.json").write_text(json.dumps({
        "purpose":"capture setup only; never release acceptance", "sources":sources, "tool":tool,
        "capture_mode": "diagnose-complete" if diagnostic else "capture",
        "harness_sha256":harness, "sampling":contract,
        "initial_context":environment_context(candidate,env,group)},indent=2)+"\n")
    with ExitStack() as overlays:
        for role, root in (("baseline", baseline), ("candidate", candidate)):
            prepare_capture_source(overlays, root, env, output/(role+"-build.log"))
        pilots={}; contracts={}
        capture_options = {'diagnostic': True} if diagnostic else {}
        for role,root in (("baseline",baseline),("candidate",candidate)):
            folder=output/("calibration-"+role);folder.mkdir()
            metadata["calibration"][role]={};pilots[role]={};contracts[role]={}
            for profile in FILES:
                records,entry=capture_profile(root,folder,profile,"calibrate",env,output,group, **capture_options)
                metadata["calibration"][role][profile]=entry
                pilots[role][profile]=calibration_counts(records,profile,sources[role]["commit"])
                contracts[role][profile]=observation_contracts(profile,[r for r in records if r["complete"]],
                    contracts["baseline"][profile] if role=="candidate" else None)
                if diagnostic:
                    print(json.dumps({'diagnostic_calibration': role, 'profile': profile, 'records': len(records)}), flush=True)
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
            phase_spreads=[]
            for profile,filename in FILES.items():
                records,entry=capture_profile(root,folder,profile,"sample",env,output,group,count_files[profile], **capture_options)
                select = diagnostic_rows if diagnostic else sampling.selected_row
                selected,spreads=select(records,profile,METRICS[profile],sources[role]["commit"],expected_cases(profile),counts[profile])
                observation_contracts(profile,records,contracts[role][profile])
                phase_spreads.extend(dict(item, profile=profile) for item in spreads)
                result=folder/filename;result.write_text(json.dumps(selected,indent=2)+"\n")
                metadata["files"][phase][profile]={"path":f"{phase}/{filename}","sha256":digest(result.read_bytes()),"samples":entry}
                (output/partial_name).write_text(json.dumps(metadata,indent=2)+"\n")
                if diagnostic:
                    print(json.dumps({'diagnostic_phase': phase, 'profile': profile, 'records': len(records),
                                      'wide_cases': sum(r['central_spread'] > sampling.PROTOCOL['central_spread_limit'] for r in spreads),
                                      'short_samples': sum(len(r['short_samples']) for r in spreads)}), flush=True)
            # Keep the complete failed phase, but do not run more formal phases
            # after a quality failure that already makes the campaign unusable.
            if not diagnostic and any(item["central_spread"] > sampling.PROTOCOL["central_spread_limit"] for item in phase_spreads):
                raise EvidenceFailure("sample distributions are unstable; no later phase or candidate attribution", {
                    "failed_phase":phase, "sample_spreads":phase_spreads, "candidate_attribution":None})
        metadata["contexts"]["after"]=environment_context(candidate,env,group)
        if any(source_info(root)!=sources[role] for role,root in (("baseline",baseline),("candidate",candidate))): raise ValueError("source changed during capture")
        # Reject full-profile quality/drift/regressions before spending any time
        # on supplemental layouts. This is not a release success report.
        if not diagnostic:
            _verify_profiles(metadata, output, candidate)
    # Standalone layouts reuse the same overlay entry; both formal overlays must
    # close safely before their collector starts.
    from diagnose_verify_layout import capture_paired
    options = {'diagnostic': True} if diagnostic else {}
    capture_paired(baseline, candidate, output/LAYOUT_DIRECTORY, group, layout_iterations(contract), **options)
    manifest_name = 'diagnostic-manifest.json' if diagnostic else 'manifest.json'
    layout_path = output/LAYOUT_DIRECTORY/manifest_name
    metadata['layout_supplement'] = {'path': LAYOUT_DIRECTORY+'/'+manifest_name, 'sha256': digest(layout_path.read_bytes())}
    (output/partial_name).write_text(json.dumps(metadata, indent=2)+'\n')
    metadata['contexts']['after'] = environment_context(candidate, env, group)
    (output/partial_name).write_text(json.dumps(metadata, indent=2)+'\n')
    if metadata['contexts']['after'] != metadata['contexts']['A1']:
        raise ValueError('resource context changed during supplemental capture')
    if any(source_info(root) != sources[role] for role, root in (("baseline", baseline), ("candidate", candidate))):
        raise ValueError('source changed during supplemental capture')
    manifest = output/manifest_name
    if diagnostic:
        journal = output/'worker-journal.jsonl'
        metadata['worker_journal'] = {'path': journal.name, 'sha256': digest(journal.read_bytes())}
    manifest.write_text(json.dumps(metadata, indent=2)+'\n')
    _, _, report = (verify_diagnostic if diagnostic else verify)(manifest, candidate)
    (output/'report.json').write_text(json.dumps(report, indent=2)+'\n')


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
    for mode in ("capture", "qualify-baseline", "diagnose-complete"):
        cap = sub.add_parser(mode)
        cap.add_argument("--baseline", type=pathlib.Path, required=True)
        if mode != "qualify-baseline": cap.add_argument("--candidate", type=pathlib.Path, required=True)
        cap.add_argument("--output", type=pathlib.Path, required=True)
        cap.add_argument("--isolation-cgroup", type=pathlib.Path, required=True)
    for mode in ("verify", "materialize", "verify-diagnostic"):
        child = sub.add_parser(mode)
        child.add_argument("--manifest", type=pathlib.Path, required=True)
        child.add_argument("--repository", type=pathlib.Path, default=pathlib.Path.cwd())
        if mode == "materialize": child.add_argument("--env-file", type=pathlib.Path, required=True)
        else: child.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    capture_output_was_absent = args.mode in ("capture", "qualify-baseline", "diagnose-complete") and not args.output.exists()
    try:
        if args.mode == "capture": capture(args.baseline.resolve(), args.candidate.resolve(), args.output.resolve(), args.isolation_cgroup)
        elif args.mode == "diagnose-complete": capture(args.baseline.resolve(), args.candidate.resolve(), args.output.resolve(), args.isolation_cgroup, diagnostic=True)
        elif args.mode == "qualify-baseline": qualify_baseline(args.baseline.resolve(), args.output.resolve(), args.isolation_cgroup)
        elif args.mode == "materialize": materialize(args.manifest.resolve(), args.repository.resolve(), args.env_file)
        else:
            verifier = verify_diagnostic if args.mode == 'verify-diagnostic' else verify
            _, _, report = verifier(args.manifest.resolve(), args.repository.resolve())
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(report, indent=2) + "\n")
        print('complete diagnostic; no release approval' if args.mode in ('diagnose-complete', 'verify-diagnostic') else "baseline qualification completed; not release acceptance" if args.mode == "qualify-baseline" else "release evidence verified")
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
