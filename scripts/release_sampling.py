"""Generate and validate the same external sampling overlay on unmodified codecs."""
import hashlib
import json
import math
import pathlib
import re

PROTOCOL = {"name": "shared-external-duration-sampler", "version": 3, "samples": 9, "warmups": 3, "case_isolation": "fresh-process", "case_order": "lexicographic", "calibration_confirmations": 2, "max_calibration_records": 12,
            "target_wall_ns": 100_000_000, "target_timed_ns": 5_000_000,
            "minimum_wall_ns": 50_000_000, "minimum_timed_ns": 2_000_000,
            "central_spread_limit": .10,
            "timing_scope": {"smoke":"original per-iteration workload", "small_file":"operation loops excluding one-time fixture preparation", "stream":"operation loops excluding one-time fixture preparation", "hotspot":"original inner recovery intervals"}}
TEMPLATES = pathlib.Path(__file__).resolve().parent / "release-sampling"
HOTSPOT_BLOCKS = ("struct ReconstructionHotspotBenchResult", "fn benchmark_test_iterations", "macro_rules! make_random_shards",
    "pub fn fill_random", "fn shards_to_option_shards", "fn option_shards_with_missing", "fn shard_index_pattern",
    "fn assert_required_reconstruction_matches", "fn bench_reconstruct_data_hotspot", "fn bench_reconstruct_some_hotspot",
    "fn write_reconstruction_hotspot_bench_results")


def block(text, name):
    start = text.index(name)
    return text[start:text.index("\n}", start) + 2]


def implementation_digest():
    files = [pathlib.Path(__file__).resolve(), *sorted(TEMPLATES.glob("*.rs"))]
    return hashlib.sha256(b"".join(p.name.encode() + b"\0" + p.read_bytes() for p in files)).hexdigest()


def exclude_once_only_setup(text, name, loop_count):
    original = block(text, name)
    # Operate only on the known rustfmt layout, and prove no non-timing statement
    # changes. Per-iteration reconstruction clones and stream adapters remain timed.
    starts = list(re.finditer(r"(?m)^            for _ in 0\.\.iterations \{", original))
    if len(starts) != loop_count:
        raise ValueError("benchmark loop layout changed; review the timing adapter")
    updated = original
    for start in reversed(starts):
        close = re.search(r"(?m)^            }$", original[start.end():])
        if close is None: raise ValueError("benchmark loop closing scope changed")
        end = start.end() + close.end()
        updated = updated[:end] + "\n            measured += start.elapsed();" + updated[end:]
        updated = updated[:start.start()] + "            let start = Instant::now();\n" + updated[start.start():]
    updated = updated.replace("    let start = Instant::now();", "    let mut measured = std::time::Duration::ZERO;", 1)
    updated = updated.replace("    let elapsed = start.elapsed();", "    let elapsed = measured;", 1)
    restored = updated.replace("            let start = Instant::now();\n", "").replace("\n            measured += start.elapsed();", "")
    restored = restored.replace("    let mut measured = std::time::Duration::ZERO;", "    let start = Instant::now();", 1).replace("    let elapsed = measured;", "    let elapsed = start.elapsed();", 1)
    if restored != original: raise ValueError("timing adapter changed a non-timing statement")
    return text.replace(original, updated, 1)


def render(root):
    rendered = {name: (TEMPLATES / name).read_text() for name in ("engine.rs", "process.rs")}
    sources = {"smoke": "benchmark_smoke.rs", "small_file": "benchmark_small_files.rs", "stream": "benchmark_stream_paths.rs"}
    for profile, filename in sources.items():
        text = (root / "tests" / filename).read_text()
        text = text.replace('#[path = "../benches/common/mod.rs"]', f'#[path = "{root}/benches/common/mod.rs"]')
        text = text.replace('mod common;', f'#[path = "{root}/tests/common/mod.rs"]\nmod common;')
        name = {"smoke": "fn smoke_iterations", "small_file": "fn small_file_iterations", "stream": "fn stream_iterations"}[profile]
        text = text.replace(block(text, name), name + "() -> usize { super::sampling::iterations() }")
        if profile in ("small_file", "stream"):
            text = exclude_once_only_setup(text, "fn run_operation" if profile == "small_file" else "fn run_stream_operation", 11 if profile == "small_file" else 3)
        rendered[profile + ".rs"] = text + "\n" + (TEMPLATES / (profile + ".rs")).read_text()
    original = (root / "src/tests/mod.rs").read_text()
    parts = [block(original, name) for name in HOTSPOT_BLOCKS]
    text = '\n'.join(parts)
    text = text.replace(block(text, "fn benchmark_test_iterations"), 'fn benchmark_test_iterations() -> usize { super::sampling::iterations() }')
    prefix = 'use std::{fs, path::PathBuf, time::Instant};\nuse rustfs_erasure_codec::galois_8::ReedSolomon;\nfn benchmark_metrics_enabled() -> bool { cfg!(feature="benchmark-metrics") }\nconst BENCHMARK_ARTIFACT_SCHEMA_VERSION: u32 = 1;\n'
    rendered["hotspot.rs"] = prefix + text + "\n" + (TEMPLATES / "hotspot.rs").read_text()
    root_source = '#![allow(dead_code)]\n' + '\n'.join(f'#[path="rse_release_sampling/{name}.rs"] mod {name};' for name in ("engine", "process", *sources, "hotspot"))
    root_source = root_source.replace('mod engine;', 'mod sampling;')
    root_source += '\n#[test]\n#[ignore]\nfn release_controlled_profile() { match std::env::var("RSE_SAMPLER_PROFILE").unwrap().as_str() {'
    root_source += ','.join(f'"{name}" => {name}::release_profile()' for name in (*sources, "hotspot"))
    root_source += ', _ => panic!("unknown release profile") } }\n'
    rendered["root.rs"] = root_source
    return rendered


def overlay_digest(root):
    return hashlib.sha256(b"".join(name.encode() + b"\0" + text.replace(str(root), "<SOURCE>").encode()
                                  for name, text in sorted(render(root).items()))).hexdigest()


def generate(root):
    directory = root / "tests/rse_release_sampling"
    entry = root / "tests/rse_release_sampling.rs"
    if directory.exists() or entry.exists():
        raise ValueError("refuse to replace an existing sampling overlay")
    directory.mkdir()
    for name, text in render(root).items():
        (entry if name == "root.rs" else directory / name).write_text(text)


def parse_output(output):
    return [json.loads(line.removeprefix("RSE_SAMPLE ")) for line in output.splitlines() if line.startswith("RSE_SAMPLE ")]


def parse_log(path):
    return parse_output(path.read_text())


def validate_telemetry(record, allowed_cpus=None):
    before, after = record.get("before"), record.get("after")
    for point in (before, after):
        if not isinstance(point, dict) or set(point) != {"cpu", "run_ns", "wait_ns", "switches"}:
            raise ValueError("per-sample scheduling telemetry is required")
        if any(type(value) is not int or value < 0 for value in point.values()):
            raise ValueError("invalid scheduling counters")
        if allowed_cpus is not None and point["cpu"] not in allowed_cpus:
            raise ValueError("sample escaped the reserved CPU set")
    if any(after[key] < before[key] for key in ("run_ns", "wait_ns", "switches")):
        raise ValueError("scheduling counters moved backwards")


def validate_timing(record, profile, count):
    row = record["rows"][0]
    for field in ("wall_ns", "timed_ns"):
        value = record.get(field)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
            raise ValueError("invalid measured interval")
    if profile == "hotspot":
        expected_timed = row["useful_shards"] * row["shard_size"] * 1e9 * count / (1048576.0 * row["candidate_mb_s"])
        tolerance = expected_timed * .00001 + 1
    else:
        expected_timed = row["ns_per_iter"] * count
        tolerance = .006 * count + expected_timed * .00001
    if abs(expected_timed - record["timed_ns"]) > tolerance:
        raise ValueError("timed interval differs from the raw exported measurement")

def run_profile(root, environment, profile, mode, log_path, counts_file=None, *, cases):
    import subprocess
    ordered = sorted(cases)
    if not ordered or len(ordered) != len(set(ordered)):
        raise ValueError("a nonempty unique case catalog is required")
    env = dict(environment, RSE_SAMPLER_PROFILE=profile, RSE_SAMPLER_MODE=mode)
    if counts_file is not None: env["RSE_SAMPLER_COUNTS"] = str(counts_file)
    command = ["cargo", "test", "--locked", "--release", "--features", "std simd-accel", "--test", "rse_release_sampling", "release_controlled_profile", "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    records = []
    with log_path.open("w") as log:
        for case in ordered:
            # A new test process owns exactly one workload. No previous case can
            # seed its allocator arenas or worker pool. All observations remain.
            offset = log.tell()
            subprocess.run(command, cwd=root, env=dict(env, RSE_SAMPLER_CASE=case), stdout=log, stderr=subprocess.STDOUT, check=True)
            with log_path.open() as reader:
                reader.seek(offset)
                current = parse_output(reader.read())
            if not current or any(r.get("case") != case or r.get("isolated_case") is not True for r in current):
                raise ValueError("case process did not emit exactly its selected workload")
            records.extend(current)
    return records


def sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus=None):
    groups = {}
    for record in records:
        if record.get("revision") != revision or record.get("case") not in expected:
            raise ValueError("sampling revision or case mismatch")
        groups.setdefault(record["case"], []).append(record)
    if set(groups) != expected:
        raise ValueError("complete sampling catalog is required")
    for case, group in groups.items():
        if [r.get("kind") for r in group] != ["warmup"] * PROTOCOL["warmups"] + ["sample"] * PROTOCOL["samples"] or [r.get("index") for r in group] != list(range(PROTOCOL["samples"]+PROTOCOL["warmups"])):
            raise ValueError("every fixed warmup and sample is required")
        for record in group:
            validate_telemetry(record, allowed_cpus)
            if record.get("isolated_case") is not True or record.get("iterations") != counts[case] or not isinstance(record.get("rows"), list) or len(record["rows"]) != 1:
                raise ValueError("sample count/iterations mismatch")
            row = record["rows"][0]
            if normalize(profile, row, revision) != case:
                raise ValueError("raw sample workload mismatch")
            validate_timing(record, profile, counts[case])
    return groups


def selected_row(records, profile, metric, revision, expected, counts, normalize, allowed_cpus=None):
    groups = sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus)
    selected, spread = [], []
    for case, group in groups.items():
        for record in group[PROTOCOL["warmups"]:]:
            for field, minimum in (("wall_ns", PROTOCOL["minimum_wall_ns"]), ("timed_ns", PROTOCOL["minimum_timed_ns"])):
                if record[field] < minimum: raise ValueError("sample measurement interval is insufficient")
        samples = group[PROTOCOL["warmups"]:]
        ordered = sorted(samples, key=lambda r: r["rows"][0][metric])
        median = ordered[len(ordered)//2]["rows"][0]
        width = (ordered[-2]["rows"][0][metric] - ordered[1]["rows"][0][metric]) / median[metric]
        spread.append({"case":case,"central_spread":width})
        selected.append(median)
    return selected, spread


if __name__ == "__main__":
    import argparse
    import os
    import shutil
    import subprocess
    parser=argparse.ArgumentParser(description="Compile the shared external sampler, then remove its generated sources")
    parser.add_argument("--check",action="store_true",required=True)
    args=parser.parse_args()
    root=TEMPLATES.parent.parent
    generate(root)
    try:
        subprocess.run(["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","--no-run"],cwd=root,env=dict(os.environ,RUSTUP_TOOLCHAIN="stable"),check=True)
        subprocess.run(["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","process::tests::"],cwd=root,env=dict(os.environ,RUSTUP_TOOLCHAIN="stable"),check=True)
    finally:
        (root/"tests/rse_release_sampling.rs").unlink()
        shutil.rmtree(root/"tests/rse_release_sampling")
