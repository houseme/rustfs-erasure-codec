"""Generate and validate the same external sampling overlay on unmodified codecs."""
import hashlib
import json
import os
import pathlib
import time
import native_tools

TEMPLATES = pathlib.Path(__file__).resolve().parent / "release-sampling"


def service_window_contract():
    return native_tools.sampling_catalog()['service_window']


PROTOCOL = native_tools.sampling_catalog()['protocol']
REPEAT_DRIFT_LIMIT = native_tools.sampling_catalog()['repeat_drift_limit']


def block(text, name):
    start = text.index(name)
    return text[start:text.index("\n}", start) + 2]


def implementation_digest():
    files = [pathlib.Path(__file__).resolve(), *sorted(TEMPLATES.glob("*.rs"))]
    return hashlib.sha256(b"".join(p.name.encode() + b"\0" + p.read_bytes() for p in files)+native_tools.implementation_bytes()).hexdigest()


# The native renderer and verifier share one workload/fixture catalog.
HOTSPOT_CASES = tuple((data, parity, name, tuple(missing), None if required is None else tuple(required))
                      for data, parity, name, missing, required in native_tools.sampling_catalog()['hotspot_cases'])


def hotspot_case(case):
    try: return next(row for row in HOTSPOT_CASES if row[2] == case)
    except StopIteration: raise ValueError('unknown hotspot workload') from None


def workload_contract(profile, case):
    return native_tools.sampling_contract('workload-contract', profile, case)


def validate_workload(record, profile):
    native_tools.sampling_request('validate-workload', profile=profile, record=record)


def validate_workload_sequence(records, profile):
    native_tools.sampling_request('validate-workload-sequence', profile=profile, records=records)


def calibration_profile(records, profile, revision, expected_page_size=None, *, allowed_cpus=None):
    result = native_tools.sampling_request('calibration-profile', records=records, profile=profile,
        revision=revision, expected_page_size=expected_page_size,
        allowed_cpus=None if allowed_cpus is None else list(allowed_cpus))
    return dict(result, counts=dict(result['counts']))


def fixture_contract(profile, case):
    return native_tools.sampling_contract('fixture-contract', profile, case)


def render(root):
    return native_tools.request('sampler-render', root=str(root), tool=str(native_tools.ROOT))


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


def validate_placement(record, allowed_cpus=None):
    native_tools.sampling_request('validate-placement', record=record,
                                  allowed_cpus=None if allowed_cpus is None else list(allowed_cpus))


def worker_environment(environment):
    result = dict(environment)
    if hasattr(os,'sched_getaffinity'):
        result['RSE_SAMPLER_CPU'] = str(min(os.sched_getaffinity(0)))
    return result


def validate_telemetry(record, allowed_cpus=None):
    native_tools.sampling_request('validate-telemetry', record=record,
                                  allowed_cpus=None if allowed_cpus is None else list(allowed_cpus))


def validate_fixture(record, profile, expected_page_size=None):
    native_tools.sampling_request('validate-fixture', profile=profile, record=record,
                                  expected_page_size=expected_page_size)


def validate_timing(record, profile, count):
    native_tools.sampling_request('validate-timing', profile=profile, record=record, count=count)


def calibration_batch(records, count, batch):
    return native_tools.sampling_request('calibration-batch', records=records, count=count, batch=batch)


def calibration_worker(records, profile, count, batch, expected_page_size=None):
    return native_tools.sampling_request('validate-calibration-worker', records=records, profile=profile,
                                         count=count, batch=batch, expected_page_size=expected_page_size)


def build_environment(root, environment):
    """Keep generated build outputs with their source, even under a shared caller cache."""
    return dict(environment, CARGO_TARGET_DIR=str(root.resolve() / 'target'))


def run_profile(root, environment, profile, mode, log_path, counts_file=None, *, cases, journal=None):
    import subprocess
    ordered = sorted(cases)
    if mode not in ("calibrate", "sample") or not ordered or len(ordered) != len(set(ordered)):
        raise ValueError("a valid mode and nonempty unique case catalog are required")
    env = dict(build_environment(root, worker_environment(environment)), RSE_SAMPLER_PROFILE=profile, RSE_SAMPLER_MODE=mode, RSE_SAMPLER_PAGE_SIZE=str(base_page_size()))
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
                if journal:
                    identity = {'case': case, 'mode': mode, 'iterations': count, 'calibration_batch': batch}
                    journal(dict(identity, event='start', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns()))
                try:
                    subprocess.run(command, cwd=root, env=worker, stdout=log, stderr=subprocess.STDOUT, check=True)
                except BaseException as error:
                    if journal:
                        journal(dict(identity, event='end', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns(),
                                     exit_code=getattr(error, 'returncode', None), error=type(error).__name__))
                    raise
                else:
                    if journal:
                        journal(dict(identity, event='end', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns(), exit_code=0))
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


def _sample_batch(records, profile, revision, expected, counts, allowed_cpus, expected_page_size, output):
    result = native_tools.sampling_request('batch', request=dict(profile=profile, revision=revision,
        records=records, expected=sorted(expected), counts=counts,
        allowed_cpus=None if allowed_cpus is None else list(allowed_cpus),
        expected_page_size=expected_page_size, output=output))
    return result


def sample_groups(records, profile, revision, expected, counts, allowed_cpus=None, expected_page_size=None):
    result = _sample_batch(records, profile, revision, expected, counts, allowed_cpus,
                           expected_page_size, dict(kind='groups'))
    return {case: [records[i] for i in indices] for case, indices in result['groups']}


def select_profile(records, profile, metric, revision, expected, counts, allowed_cpus=None, expected_page_size=None, *, diagnostic=False):
    result = _sample_batch(records, profile, revision, expected, counts, allowed_cpus,
                           expected_page_size, dict(kind='select', metric=metric, diagnostic=diagnostic))
    return dict(selected=[records[i]['rows'][0] for i in result['selected_indices']],
                spreads=result['spreads'], preparation=result['preparation'], worker_starts=result['worker_starts'])


def describe_groups(records, profile, revision, expected, counts, fixtures, allowed_cpus=None, expected_page_size=None):
    return _sample_batch(records, profile, revision, expected, counts, allowed_cpus,
                          expected_page_size, dict(kind='describe', fixtures=fixtures))['descriptions']


def checkout_baseline(source, destination, revision):
    """Fetch the exact baseline tree even when source is a shallow CI checkout."""
    import subprocess
    subprocess.run(['git','init','--quiet',str(destination)],check=True)
    subprocess.run(['git','fetch','--quiet','--no-tags','--depth=1',str(source),revision],cwd=destination,check=True)
    subprocess.run(['git','checkout','--quiet','--detach',revision],cwd=destination,check=True)


def check(root, workers=False):
    import subprocess
    from diagnostic_overlay import overlay
    with overlay(root, render(root)):
        result = subprocess.run(["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","--","--skip","release_controlled_profile","--nocapture"],
                                cwd=root,env=build_environment(root,dict(os.environ,RUSTUP_TOOLCHAIN="stable")),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        print(result.stdout, end='')
        result.check_returncode()
        actual = {}
        if workers:
            from release_evidence import SAMPLING_ENV, expected_cases
            for profile in ('smoke','hotspot'):
                for case in sorted(expected_cases(profile)):
                    env=dict(build_environment(root,worker_environment(dict(os.environ,**SAMPLING_ENV))),RSE_SAMPLER_PROFILE=profile,RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='1',RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(base_page_size()))
                    process=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
                    if process.returncode:
                        print(process.stdout,end='');process.check_returncode()
                    rows=parse_output(process.stdout)
                    if len(rows)!=12 or [r['index'] for r in rows]!=list(range(12)) or any(r['case'] != case or r['iterations'] != 1 for r in rows):
                        raise ValueError('complete one-iteration worker output is required')
                    validate_workload_sequence(rows,profile)
                    if hasattr(os,'sched_getaffinity'):
                        for row in rows:validate_placement(row,os.sched_getaffinity(0))
                    actual[case]=[{'workload':r['workload'],'placement':r['placement']} for r in rows]
            print('All 23 smoke/hotspot workers executed at one iteration; input/preparation correctness only')
    records = parse_output(result.stdout,'RSE_INPUT_REPLAY ')
    if len(records) != len(HOTSPOT_CASES) or {r['case'] for r in records} != {row[2] for row in HOTSPOT_CASES}:
        raise ValueError('complete native input replay witnesses are required')
    if workers and hasattr(os,'sched_getaffinity'):
        placements={json.dumps(row['placement'],sort_keys=True) for rows in actual.values() for row in rows}
        if len(placements) != 1: raise ValueError('native workers changed their constructor/pool placement')
        print('RSE_PLACEMENT_CHECK '+next(iter(placements)))
    return {'prefixes':sorted(records,key=lambda row:row['case']),'workers':actual}


if __name__ == "__main__":
    import argparse
    import subprocess
    import tempfile
    parser=argparse.ArgumentParser(description="Check actual shared workers and optional published-baseline input replay; never performance acceptance")
    parser.add_argument("--check",action="store_true",required=True)
    parser.add_argument("--check-workers",action="store_true",help="Execute the 23 changed workers at one iteration, never performance acceptance")
    parser.add_argument("--baseline-ref",help="Full published 9.0.0 source commit, already available locally")
    args=parser.parse_args()
    root=TEMPLATES.parent.parent
    current=check(root,args.check_workers)
    if args.baseline_ref:
        from release_evidence import BASELINE_COMMIT
        if args.baseline_ref != BASELINE_COMMIT: parser.error('baseline-ref must be the published 9.0.0 commit')
        with tempfile.TemporaryDirectory(prefix='rse-input-replay-') as temporary:
            baseline=pathlib.Path(temporary)/'baseline'
            checkout_baseline(root,baseline,BASELINE_COMMIT)
            previous=check(baseline,args.check_workers)
        if current != previous: raise ValueError('published and candidate input replay witnesses differ')
        print('Published 9.0.0/candidate input replay matches for all eight hotspot scenarios; correctness only')
