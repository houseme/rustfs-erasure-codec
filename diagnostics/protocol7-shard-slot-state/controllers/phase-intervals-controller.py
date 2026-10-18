#!/usr/bin/env python3
"""Compare fresh fixed-count modes and trace the original per-iteration Slot clones.

Instrumentation changes timing and heap history. Neither arm can approve a release
or establish a candidate speedup; the plain arm preserves the shared sampler.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
import pathlib
import subprocess

import release_evidence as evidence
import release_sampling as sampling

CASE = 'reconstruct_shard_slot:4:2:65536'
SCHEDULE = ('calibrate', 'sample', 'sample', 'calibrate')
TEMPLATE = pathlib.Path(__file__).resolve().parent/'diagnostics/shard_slot.rs'
PREFIX = 'RSE_SLOT_LAYOUT '


def render(root, traced):
    result = sampling.render(root)
    if not traced:
        return result
    original = '''        SmallFileOp::ReconstructShardSlot => {
            let mut original =
                make_full_shards(seed, case.data_shards, case.parity_shards, case.shard_size);
            rs.encode(&mut original).unwrap();
            let start = Instant::now();
            for _ in 0..iterations {
                let mut shards = shards_to_slots(&original);
                mark_missing_slots(&mut shards, &[0, case.data_shards]);
                rs.reconstruct(&mut shards).unwrap();
            }
            measured += start.elapsed();
        }'''
    changed = original.replace('            let start = Instant::now();',
        '            let mut trace = super::slot_trace::Trace::for_process(&original, iterations);\n            let start = Instant::now();')
    changed = changed.replace('                let mut shards = shards_to_slots(&original);',
        '                let clone_start = Instant::now();\n                let mut shards = shards_to_slots(&original);')
    changed = changed.replace('                rs.reconstruct(&mut shards).unwrap();',
        '                let clone_ns = clone_start.elapsed().as_nanos();\n                trace.before(&shards);\n                let reconstruct_start = Instant::now();\n                rs.reconstruct(&mut shards).unwrap();\n                let reconstruct_ns = reconstruct_start.elapsed().as_nanos();\n                trace.after(&shards, &original);\n                let drop_start = Instant::now();\n                drop(shards);\n                trace.phases(clone_ns, reconstruct_ns, drop_start.elapsed().as_nanos());')
    changed = changed.replace('            measured += start.elapsed();',
        '            measured += start.elapsed();\n            trace.finish();')
    if result['small_file.rs'].count(original) != 1:
        raise ValueError('ShardSlot workload changed; review the diagnostic adapter')
    result['small_file.rs'] = result['small_file.rs'].replace(original, changed, 1)
    result['slot_trace.rs'] = TEMPLATE.read_text()
    result['root.rs'] += '\n#[path="rse_release_sampling/slot_trace.rs"] mod slot_trace;\n'
    return result


@contextmanager
def overlay(repository, traced):
    entry, directory = repository/'tests/rse_release_sampling.rs', repository/'tests/rse_release_sampling'
    if entry.exists() or directory.exists(): raise ValueError('refuse to overwrite a sampling overlay')
    files = render(repository, traced)
    directory.mkdir()
    try:
        for name, value in files.items():
            with (entry if name == 'root.rs' else directory/name).open('x') as target:
                target.write(value)
        yield files
    finally:
        # Never delete unrelated or changed generated inputs.
        if {p.name for p in directory.iterdir()} != set(files)-{'root.rs'} or any((entry if n == 'root.rs' else directory/n).read_text() != value for n, value in files.items()):
            raise ValueError('unexpected generated input; preserve it for inspection')
        for path in directory.iterdir(): path.unlink()
        directory.rmdir(); entry.unlink()


def check(repository):
    with overlay(repository, True):
        subprocess.run(['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
                        '--test', 'rse_release_sampling', 'slot_trace', '--', '--nocapture'],
                       cwd=repository, env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'), check=True)


def layouts(records, iterations, page_size):
    if len(records) != 12:
        raise ValueError('all twelve instrumented observations are required')
    for record in records:
        if record.get('entry') != 'Vec<ShardSlot<Vec<u8>>>' or record.get('source_lifetime') != 'observation' or record.get('clone_lifetime') != 'iteration' or type(record.get('iterations')) is not int or record['iterations'] != iterations or type(record.get('page_size')) is not int or record['page_size'] != page_size:
            raise ValueError('incorrect Slot workload contract')
        if type(record.get('overflow')) is not int or record['overflow'] < 0 or type(record.get('transitions')) is not int or not 0 <= record['transitions'] < iterations:
            raise ValueError('invalid bounded histogram counters')
        phases = record.get('phase_ns')
        if not isinstance(phases, dict) or set(phases) != {'clone', 'reconstruct', 'drop'} or any(type(v) is not int or v <= 0 for v in phases.values()):
            raise ValueError('complete positive phase intervals are required')
        buckets = record.get('layouts')
        if not isinstance(buckets, list) or not 1 <= len(buckets) <= 128:
            raise ValueError('invalid bounded histogram')
        for bucket in buckets:
            if type(bucket.get('count')) is not int or not 1 <= bucket['count'] <= iterations:
                raise ValueError('invalid layout count')
        if sum(b['count'] for b in buckets) + record['overflow'] != iterations:
            raise ValueError('missing clone observations')
        for buffers in [record.get('source'), *(b.get('buffers') for b in buckets)]:
            # Reuse the formal buffer-shape checks without declaring a Slot case
            # to be an accepted retained Vec workload.
            synthetic = {'case': 'verify_with_buffer:4:2:65536', 'fixture': {
                'entry': 'Vec<Vec<u8>>', 'lifetime': 'worker', 'page_size': page_size,
                'source': buffers, 'scratch': buffers[:2] if isinstance(buffers, list) else None}}
            sampling.validate_fixture(synthetic, 'small_file', page_size)
    return {'histogram_complete': all(r['overflow'] == 0 for r in records),
            'source_layouts': [r['source'] for r in records],
            'clone_layout_counts': [len(r['layouts']) for r in records],
            'clone_layout_transitions': [r['transitions'] for r in records]}


def capture(tool, repository, output, group, iterations):
    if type(iterations) is not int or not 1 <= iterations <= 1_000_000:
        raise ValueError('invalid bounded diagnostic iteration count')
    if (tool/'scripts').resolve() != pathlib.Path(__file__).resolve().parent:
        raise ValueError('tool path differs from imported diagnostic')
    if output.exists(): raise ValueError('use a new diagnostic directory')
    output.mkdir(parents=True, mode=0o700)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_'))
           and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    source, controller = evidence.source_info(repository), evidence.source_info(tool)
    if source['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('diagnose the published 9.0.0 baseline first')
    context = evidence.environment_context(repository, env, group)
    page_size = context['runtime']['page_size']
    if page_size < 4096: raise ValueError('Slot diagnosis requires a base page of at least 4096 bytes')
    cpu = min(context['isolation']['cpu_affinity'])
    def save(name, value): (output/name).write_text(json.dumps(value, indent=2)+'\n')
    metadata = {'schema_version': 'shard-slot-diagnostic-v2', 'source': source, 'tool': controller,
                'context': context, 'measuring_cpu': cpu, 'iterations': iterations,
                'protocol': sampling.PROTOCOL, 'renderer_sha256': sampling.implementation_digest(),
                'controller_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                'trace_sha256': hashlib.sha256(TEMPLATE.read_bytes()).hexdigest(),
                'purpose': 'within-version fixed-count diagnosis; instrumented timings are perturbed',
                'release_approval': False, 'candidate_attribution': None}
    save('provenance.json', metadata)
    reports = []
    for traced in (False, True):
        arm = 'traced' if traced else 'plain'
        with overlay(repository, traced) as files:
            save(arm+'-overlay.json', {'sha256': hashlib.sha256(b''.join(name.encode()+b'\0'+value.replace(str(repository), '<SOURCE>').encode() for name, value in sorted(files.items()))).hexdigest()})
            command = ['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel', '--test', 'rse_release_sampling']
            with (output/(arm+'-build.log')).open('x') as log:
                subprocess.run([*command, '--no-run'], cwd=repository, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            for worker, mode in enumerate(SCHEDULE):
                name = f'{arm}-{worker}-{mode}'
                worker_env = dict(env, RSE_SAMPLER_CASE=CASE, RSE_SAMPLER_PROFILE='small_file',
                                  RSE_SAMPLER_MODE=mode, RSE_SAMPLER_ITERATIONS=str(iterations),
                                  RSE_SAMPLER_BATCH='1' if mode == 'calibrate' else '0',
                                  RSE_SAMPLER_PAGE_SIZE=str(page_size))
                log_path = output/(name+'.log')
                try:
                    with log_path.open('x') as log:
                        subprocess.run(['taskset', '--cpu-list', str(cpu), *command, 'release_controlled_profile', '--', '--exact', '--ignored', '--nocapture', '--test-threads=1'], cwd=repository, env=worker_env, stdout=log, stderr=subprocess.STDOUT, check=True)
                finally:
                    if log_path.exists():
                        text = log_path.read_text()
                        records = sampling.parse_output(text)
                        traces = sampling.parse_output(text, PREFIX)
                        save(name+'-samples.partial.json', records)
                        save(name+'-timing.partial.json', sampling.parse_output(text, 'RSE_TIMING '))
                        save(name+'-layouts.partial.json', traces)
                counts = {CASE: iterations}
                normalize = lambda p, row, rev: next(iter(evidence.indexed_rows(p, [row], rev, counts, complete=False)))
                if mode == 'calibrate':
                    sampling.calibration_batch(records, iterations, 1)
                    for record in records:
                        sampling.validate_telemetry(record, {cpu})
                        sampling.validate_timing(record, 'small_file', iterations)
                        normalize('small_file', record['rows'][0], source['commit'])
                else:
                    sampling.sample_groups(records, 'small_file', source['commit'], {CASE}, counts, normalize, {cpu}, page_size)
                detail = layouts(traces, iterations, page_size) if traced else {}
                if traced and any(sum(trace['phase_ns'].values()) > row['timed_ns'] + 1 for trace, row in zip(traces, records)):
                    raise ValueError('phase intervals exceed the enclosing diagnostic timer')
                if not traced and traces: raise ValueError('plain arm was instrumented')
                reports.append({'arm': arm, 'worker': worker, 'mode': mode,
                                'wall_ns': [r['wall_ns'] for r in records],
                                'timed_ns': [r['timed_ns'] for r in records], **detail})
                if evidence.source_info(repository) != source or evidence.source_info(tool) != controller or evidence.environment_context(repository, env, group) != context:
                    raise ValueError('source or resource context changed')
    report = dict(metadata, workers=reports)
    save('diagnosis.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='compile and test the diagnostic adapter without measuring')
    for name in ('tool', 'repository', 'output', 'isolation-cgroup'):
        parser.add_argument('--'+name, type=pathlib.Path)
    parser.add_argument('--iterations', type=int, default=4096)
    args = parser.parse_args()
    if args.check:
        check(pathlib.Path(__file__).resolve().parents[1]); return
    if any(getattr(args, name) is None for name in ('tool', 'repository', 'output', 'isolation_cgroup')):
        parser.error('capture requires --tool, --repository, --output and --isolation-cgroup')
    capture(args.tool.resolve(), args.repository.resolve(), args.output.resolve(), args.isolation_cgroup, args.iterations)


if __name__ == '__main__': main()
