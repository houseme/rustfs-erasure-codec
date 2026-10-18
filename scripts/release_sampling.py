"""Generate and validate the same external sampling overlay on unmodified codecs."""
import hashlib
import json
import math
import os
import pathlib
import re

PROTOCOL = {"name": "shared-external-duration-sampler", "version": 10, "samples": 9, "warmups": 3, "case_isolation": "fresh-process-per-count", "case_order": "lexicographic", "max_calibration_batches": 12, "max_calibration_records": 144,
            "target_wall_ns": 100_000_000, "target_timed_ns": 5_000_000,
            "minimum_wall_ns": 50_000_000, "minimum_timed_ns": 2_000_000,
            "central_spread_limit": .10,
            "observer_buffer_policy": "bounded-stack-procfs-no-rust-heap",
            "result_metadata_policy": "bounded-inline-hotspot-patterns-deferred-formatting",
            "stream_output_policy": "independent-capacity-per-writer-per-iteration",
            "fixture_layout": {"scope": "small_file:all;stream:all;caller-storage-only", "codec_lifetime": "worker", "dynamic_outputs": "original per-iteration policy; offsets explicitly untracked", "entry": "Vec<Vec<u8>>", "lifetime": "worker", "construction": "caller-buffers-before-codec", "offset_moduli": [64, 4096, "os_base_page_size"], "page_size_source": "controller-sysconf", "untracked_cases": "original allocation contract; no layout guarantee"},
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


def reserve_stream_outputs(text):
    """Correct one declared fixture constructor on both untouched source trees."""
    original = "let mut writers = vec![Vec::with_capacity(case.shard_size); case.parity_shards];"
    replacement = "let mut writers = reserved_stream_outputs(case.parity_shards, case.shard_size);"
    if text.count(original) != 1:
        raise ValueError("stream output construction changed; review the shared fixture adapter")
    return text.replace(original, replacement, 1)


# The loop body remains authoritative in the original benchmark source. These
# descriptors define only the explicit caller-fixture and API contracts.
SMALL_KERNELS = (
    ("SmallFileOp::Standard(Operation::Encode)", "encode"),
    ("SmallFileOp::Standard(Operation::Verify)", "verify"),
    ("SmallFileOp::VerifyWithBuffer", "verify_with_buffer"),
    ("SmallFileOp::Standard(Operation::Reconstruct)", "reconstruct"),
    ("SmallFileOp::ReconstructOpt", "reconstruct_opt"),
    ("SmallFileOp::ReconstructShardSlot", "reconstruct_shard_slot"),
    ("SmallFileOp::ReconstructSomeDataOnly", "reconstruct_some_data_only"),
    ("SmallFileOp::Standard(Operation::ReconstructData)", "reconstruct_data"),
)
STREAM_KERNELS = tuple(("StreamOperation::"+name, name.lower()+"_stream")
                       for name in ("Encode", "Verify", "Reconstruct"))


def fixture_contract(profile, case):
    if profile not in ("small_file", "stream"): return None
    parts = case.split(":")
    if len(parts) != (4 if profile == "small_file" else 5):
        raise ValueError("invalid caller-fixture case")
    operation, data, parity, size = parts[:4]
    data, parity, size = int(data), int(parity), int(size)
    if not 1 <= data < data+parity <= 32 or size <= 0:
        raise ValueError("invalid caller-fixture shape")
    all_mask = (1 << (data+parity))-1
    contracts = {
        "encode": ("Vec<Vec<u8>>", "caller-parity-in-place", 0, 0),
        "verify": ("Vec<Vec<u8>>", "internal-scratch-untracked", 0, 0),
        "verify_with_buffer": ("Vec<Vec<u8>>", "retained-caller-scratch", 0, 0),
        "reconstruct": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), all_mask),
        "reconstruct_opt": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), all_mask),
        "reconstruct_shard_slot": ("Vec<ShardSlot<Vec<u8>>>", "iteration-clones-retain-missing-storage-untracked", 1 | (1 << data), all_mask),
        "reconstruct_some_data_only": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), 1),
        "reconstruct_data": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 3, (1 << data)-1),
        "encode_stream": ("Vec<&[u8]> -> Vec<Vec<u8>>", "iteration-reserved-writers-untracked", 0, 0),
        "verify_stream": ("Vec<&[u8]>", "iteration-readers-internal-scratch-untracked", 0, 0),
        "reconstruct_stream": ("Vec<Cursor<Vec<u8>>>", "iteration-cloned-cursors-empty-missing-untracked", 1 | (1 << data), all_mask),
    }
    allowed = dict(SMALL_KERNELS if profile == "small_file" else STREAM_KERNELS).values()
    if operation not in allowed: raise ValueError("unexpected caller-fixture operation")
    return dict(zip(("call_entry", "output_policy", "missing_mask", "required_mask"), contracts[operation]))


def original_loop(text, pattern):
    """Extract exactly one rustfmt-shaped loop, never copy one-time setup."""
    marker = "        "+pattern+" => {"
    if text.count(marker) != 1: raise ValueError("benchmark operation arm changed")
    arm = text[text.index(marker):]
    arm = arm[:re.search(r"(?m)^        }", arm).start()]
    match = re.search(r"(?ms)^            for _ in 0\.\.iterations \{.*?^            }", arm)
    if match is None or arm.count("for _ in 0..iterations {") != 1:
        raise ValueError("benchmark operation loop changed")
    return match.group()


def prepared_kernel(text, profile):
    arms = []
    descriptor = []
    kernels = SMALL_KERNELS if profile == "small_file" else STREAM_KERNELS
    for pattern, operation in kernels:
        loop = original_loop(text, pattern)
        replacements = {}
        if profile == "small_file":
            if operation == "encode": replacements = {"rs.encode(&mut shards)": "rs.encode(&mut *original)"}
            elif operation == "verify": replacements = {"rs.verify(&shards)": "rs.verify(&*original)"}
            elif operation == "verify_with_buffer": replacements = {"rs.verify_with_buffer(&shards, &mut buffer)": "rs.verify_with_buffer(&*original, &mut *scratch)"}
            elif operation == "reconstruct_shard_slot": replacements = {"shards_to_slots(&original)": "shards_to_slots(original)"}
            elif operation == "reconstruct_some_data_only": replacements = {"&required)": "required)"}
        else:
            if operation == "encode_stream": replacements = {"data.iter()": "original.iter()"}
            else: replacements = {"shards.iter()": "original.iter()"}
            if operation == "reconstruct_stream": replacements["&shards[0]"] = "&original[0]"
        for before, after in replacements.items():
            if loop.count(before) != 1: raise ValueError("benchmark operand layout changed")
            loop = loop.replace(before, after, 1)
        # Existing buffered verify observes its boolean result. Apply the same
        # explicit correctness oracle to ordinary verify; no call is removed.
        if operation in ("verify", "verify_with_buffer"):
            old = "rs."+operation+"(&*original"+("" if operation == "verify" else ", &mut *scratch")+").unwrap();"
            if loop.count(old) != 1: raise ValueError("benchmark verification oracle changed")
            loop = loop.replace(old, "assert!(std::hint::black_box("+old[:-1]+"));")
        arms.append("        "+pattern+" => {\n"+loop+"\n        }")
        contract = fixture_contract(profile, operation+":4:2:1024"+(":1024" if profile == "stream" else ""))
        missing = "0" if contract["missing_mask"] == 0 else ("3" if operation == "reconstruct_data" else "1 | (1 << case.data_shards)")
        required = "0" if contract["required_mask"] == 0 else ("1" if operation == "reconstruct_some_data_only" else "(1 << case.data_shards)-1" if operation == "reconstruct_data" else "(1 << (case.data_shards+case.parity_shards))-1")
        descriptor.append(f'        {pattern} => ("{contract["call_entry"]}", "{contract["output_policy"]}", {missing}, {required}),')
    if profile == "small_file":
        signature = "prepared: &mut PreparedSmallFile, iterations: usize"
        bindings = "    let case = prepared.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let scratch = &mut prepared.scratch; let required = &prepared.required;\n"
        arms.append('        _ => panic!("unsupported small-file release operation"),')
        descriptor.append('        _ => panic!("unsupported small-file release operation"),')
        contract_signature = "case: BenchCase, operation: SmallFileOp"
    else:
        signature = "prepared: &mut PreparedStream, iterations: usize"
        bindings = "    let case = prepared.stream_case.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let options = &prepared.options;\n"
        # The original code already takes &options; the generated parameter is
        # a reference, so remove this extra borrow explicitly.
        arms = [arm.replace("&options", "options") for arm in arms]
        contract_signature = "case: BenchCase, operation: StreamOperation"
    return "\nfn execute_release_kernel("+signature+") -> std::time::Duration {\n"+bindings+"    let start = Instant::now();\n    match operation {\n"+"\n".join(arms)+"\n    }\n    start.elapsed()\n}\n"+"fn release_contract("+contract_signature+") -> (&'static str, &'static str, u64, u64) {\n    match operation {\n"+"\n".join(descriptor)+"\n    }\n}\n"


def render(root):
    rendered = {name: (TEMPLATES / name).read_text() for name in ("engine.rs", "process.rs", "layout.rs", "pattern.rs", "verify_layout.rs")}
    sources = {"smoke": "benchmark_smoke.rs", "small_file": "benchmark_small_files.rs", "stream": "benchmark_stream_paths.rs"}
    for profile, filename in sources.items():
        text = (root / "tests" / filename).read_text()
        shared_modules = '#[path = "../benches/common/mod.rs"]\nmod bench_common;\nmod common;'
        if text.count(shared_modules) != 1:
            raise ValueError("benchmark common-module layout changed")
        text = text.replace(shared_modules, 'use super::{bench_common, common};', 1)
        name = {"smoke": "fn smoke_iterations", "small_file": "fn small_file_iterations", "stream": "fn stream_iterations"}[profile]
        text = text.replace(block(text, name), name + "() -> usize { super::sampling::iterations() }")
        if profile in ("small_file", "stream"):
            text = exclude_once_only_setup(text, "fn run_operation" if profile == "small_file" else "fn run_stream_operation", 11 if profile == "small_file" else 3)
        if profile == "stream": text = reserve_stream_outputs(text)
        rendered[profile + ".rs"] = text + "\n" + (TEMPLATES / (profile + ".rs")).read_text()
        if profile in ("small_file", "stream"): rendered[profile+".rs"] += prepared_kernel(text, profile)
    original = (root / "src/tests/mod.rs").read_text()
    parts = [block(original, name) for name in HOTSPOT_BLOCKS]
    text = '\n'.join(parts)
    if text.count(": String,") != 2:
        raise ValueError("hotspot result metadata changed; review the bounded adapter")
    text = text.replace(": String,", ": super::pattern::ShardPattern,")
    original_pattern = block(text, "fn shard_index_pattern")
    text = text.replace(original_pattern, "fn shard_index_pattern(data_shards: usize, indices: &[usize]) -> super::pattern::ShardPattern { super::pattern::ShardPattern::new(data_shards, indices) }", 1)
    text = text.replace(block(text, "fn benchmark_test_iterations"), 'fn benchmark_test_iterations() -> usize { super::sampling::iterations() }')
    prefix = 'use std::{fs, path::PathBuf, time::Instant};\nuse rustfs_erasure_codec::galois_8::ReedSolomon;\nfn benchmark_metrics_enabled() -> bool { cfg!(feature="benchmark-metrics") }\nconst BENCHMARK_ARTIFACT_SCHEMA_VERSION: u32 = 1;\n'
    rendered["hotspot.rs"] = prefix + text + "\n" + (TEMPLATES / "hotspot.rs").read_text()
    root_source = f'#![allow(dead_code)]\n#[path="{root}/benches/common/mod.rs"] mod bench_common;\n#[path="{root}/tests/common/mod.rs"] mod common;\n' + '\n'.join(f'#[path="rse_release_sampling/{name}.rs"] mod {name};' for name in ("engine", "process", "layout", "pattern", "verify_layout", *sources, "hotspot"))
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


def parse_output(output, prefix="RSE_SAMPLE "):
    """Keep completed journal records; ignore only one interrupted final fragment."""
    records = []
    lines = output.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if not line.startswith(prefix): continue
        try:
            records.append(json.loads(line.removeprefix(prefix)))
        except json.JSONDecodeError:
            if index != len(lines)-1 or line.endswith('\n'): raise
    return records


def valid_page_size(value):
    return type(value) is int and 1024 <= value <= 1048576 and value & (value-1) == 0


def base_page_size():
    value = os.sysconf('SC_PAGE_SIZE')
    if not valid_page_size(value): raise ValueError("invalid OS base page size")
    return value


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


def validate_fixture(record, profile, expected_page_size=None):
    """Require the complete retained caller contract; dynamic outputs stay explicit."""
    fixture = record.get("fixture")
    contract = fixture_contract(profile, record["case"])
    tracked = contract is not None
    if not tracked:
        if fixture is not None: raise ValueError("unexpected fixture layout declaration")
        return
    if not isinstance(fixture, dict) or set(fixture) != {"entry", "lifetime", "page_size", "source", "scratch", *contract} or fixture.get("entry") != "Vec<Vec<u8>>" or fixture.get("lifetime") != "worker":
        raise ValueError("caller storage requires a retained Vec fixture layout")
    if any(type(fixture.get(key)) is not type(value) or fixture[key] != value for key, value in contract.items()):
        raise ValueError("API entry or destination allocation contract differs")
    page_size = fixture.get("page_size")
    if not valid_page_size(page_size) or expected_page_size is not None and page_size != expected_page_size:
        raise ValueError("fixture page size differs from the recorded runtime")
    operation, data, parity, size = record["case"].split(":")[:4]
    data, parity, size = int(data), int(parity), int(size)
    source_count = data if operation == "encode_stream" else data+parity
    scratch_count = parity if operation == "verify_with_buffer" else 0
    for role, count in (("source", source_count), ("scratch", scratch_count)):
        positions = fixture.get(role)
        if not isinstance(positions, list) or len(positions) != count:
            raise ValueError("incomplete fixture buffer layout")
        for position in positions:
            if not isinstance(position, dict) or set(position) != {"len", "capacity", "offset_64", "offset_4096", "offset_page"} or any(type(v) is not int for v in position.values()):
                raise ValueError("invalid fixture buffer position")
            if position["len"] != size or not size <= position["capacity"] <= 2**64 - 1 or not 0 <= position["offset_4096"] < 4096 or not 0 <= position["offset_64"] < 64 or position["offset_4096"] % 64 != position["offset_64"] or not 0 <= position["offset_page"] < page_size or position["offset_page"] % min(page_size, 4096) != position["offset_4096"] % min(page_size, 4096):
                raise ValueError("invalid fixture shape or offset")


def fixture_catalog(records, *, calibration=False):
    """Extract already-validated fixed layouts, never infer them from timing."""
    return {r["case"]: r["fixture"] for r in records
            if r.get("fixture") is not None and
            (r.get("complete") is True if calibration else r.get("kind") == "sample")}


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

def calibration_batch(records, count, batch):
    """Replay one complete worker schedule; return the next count, or None."""
    width = PROTOCOL["warmups"] + PROTOCOL["samples"]
    if type(count) is not int or not 1 <= count <= 1_000_000_000 or type(batch) is not int or not 1 <= batch <= PROTOCOL["max_calibration_batches"] or len(records) != width:
        raise ValueError("invalid bounded calibration batch")
    for index, record in enumerate(records):
        kind = "calibration_warmup" if index < PROTOCOL["warmups"] else "calibration"
        if record.get("kind") != kind or any(type(record.get(field)) is not int for field in ("index", "iterations", "calibration_batch")) or record.get("index") != index or record.get("iterations") != count or record.get("calibration_batch") != batch or record.get("isolated_case") is not True:
            raise ValueError("calibration requires the complete fixed warmup and rehearsal schedule")
        for field in ("wall_ns", "timed_ns"):
            value = record.get(field)
            if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
                raise ValueError("invalid calibration interval")
        met = record["wall_ns"] >= PROTOCOL["target_wall_ns"] and record["timed_ns"] >= PROTOCOL["target_timed_ns"]
        if record.get("target_met") is not met:
            raise ValueError("calibration target flag differs from measured intervals")
    if any(r.get("fixture") != records[0].get("fixture") for r in records):
        raise ValueError("fixture layout changed inside a calibration worker")
    probes = records[PROTOCOL["warmups"]:]
    complete = all(r["target_met"] for r in probes)
    if any(r.get("complete") is not (complete and index == width-1) for index, r in enumerate(records)):
        raise ValueError("calibration lacks the complete measurement rehearsal")
    if complete:
        return None
    desired = max(max(PROTOCOL["target_wall_ns"]/r["wall_ns"], PROTOCOL["target_timed_ns"]/r["timed_ns"]) for r in probes)*1.25
    following = count * math.ceil(min(64, max(2, desired)))
    if following > 1_000_000_000:
        raise ValueError("calibration iteration budget exceeded")
    return following


def run_profile(root, environment, profile, mode, log_path, counts_file=None, *, cases):
    import subprocess
    ordered = sorted(cases)
    if mode not in ("calibrate", "sample") or not ordered or len(ordered) != len(set(ordered)):
        raise ValueError("a valid mode and nonempty unique case catalog are required")
    env = dict(environment, RSE_SAMPLER_PROFILE=profile, RSE_SAMPLER_MODE=mode, RSE_SAMPLER_PAGE_SIZE=str(base_page_size()))
    counts = {}
    if mode == "sample":
        if counts_file is None: raise ValueError("frozen counts are required")
        for line in counts_file.read_text().splitlines():
            name, value = line.split("\t")
            if name in counts: raise ValueError("duplicate frozen case count")
            count = int(value)
            if not 1 <= count <= 1_000_000_000: raise ValueError("invalid frozen case count")
            counts[name] = count
        if set(counts) != set(ordered): raise ValueError("complete frozen case counts are required")
    command = ["cargo", "test", "--locked", "--release", "--features", "std simd-accel", "--test", "rse_release_sampling", "release_controlled_profile", "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    records = []
    with log_path.open("w") as log:
        for case in ordered:
            count, batch = (1, 1) if mode == "calibrate" else (counts[case], 0)
            while True:
                worker = dict(env, RSE_SAMPLER_CASE=case, RSE_SAMPLER_ITERATIONS=str(count), RSE_SAMPLER_BATCH=str(batch))
                offset = log.tell()
                subprocess.run(command, cwd=root, env=worker, stdout=log, stderr=subprocess.STDOUT, check=True)
                with log_path.open() as reader:
                    reader.seek(offset)
                    current = parse_output(reader.read())
                if not current or any(r.get("case") != case or r.get("isolated_case") is not True for r in current):
                    raise ValueError("case process did not emit exactly its selected workload")
                records.extend(current)
                if mode != "calibrate": break
                following = calibration_batch(current, count, batch)
                if following is None: break
                count, batch = following, batch+1
                if batch > PROTOCOL["max_calibration_batches"]:
                    raise ValueError("calibration batch budget exceeded; observations preserved")
    return records


def sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus=None, expected_page_size=None):
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
            validate_fixture(record, profile, expected_page_size)
            if record.get("fixture") != group[0].get("fixture"):
                raise ValueError("fixture layout changed inside a measurement worker")
            if any(type(record.get(field)) is not int for field in ("index", "iterations", "calibration_batch")) or record.get("calibration_batch") != 0 or record.get("isolated_case") is not True or record.get("iterations") != counts[case] or not isinstance(record.get("rows"), list) or len(record["rows"]) != 1:
                raise ValueError("sample count/iterations mismatch")
            row = record["rows"][0]
            if normalize(profile, row, revision) != case:
                raise ValueError("raw sample workload mismatch")
            validate_timing(record, profile, counts[case])
    return groups


def selected_row(records, profile, metric, revision, expected, counts, normalize, allowed_cpus=None, expected_page_size=None):
    groups = sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus, expected_page_size)
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
    parser=argparse.ArgumentParser(description="Compile and test the shared external sampler, then remove its generated sources")
    parser.add_argument("--check",action="store_true",required=True)
    args=parser.parse_args()
    root=TEMPLATES.parent.parent
    generate(root)
    try:
        subprocess.run(["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","--","--skip","release_controlled_profile"],cwd=root,env=dict(os.environ,RUSTUP_TOOLCHAIN="stable"),check=True)
    finally:
        (root/"tests/rse_release_sampling.rs").unlink()
        shutil.rmtree(root/"tests/rse_release_sampling")
