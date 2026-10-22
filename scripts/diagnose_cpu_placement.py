#!/usr/bin/env python3
"""Pin only owned measuring threads after codec construction; never release evidence."""
import argparse
import hashlib
import json
import os
import pathlib
import native_tools
import selectors
import signal
import subprocess
import time

from diagnostic_overlay import overlay
import release_evidence as evidence
import release_sampling as sampling

CASE = 'encode:10:4:524288'
TEMPLATE = pathlib.Path(__file__).resolve().parent/'diagnostics/cpu_placement.rs'
COMMAND = ['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel', '--test', 'rse_release_sampling']


def render(root, allocation_check=False):
    return native_tools.diagnostic_render(root, 'cpu-placement', allocation_check=allocation_check)


def worker_plan(cpus):
    return [(name, sequence) for name, sequence in native_tools.request('diagnostic-layout',
        request=dict(operation='cpu-plan', cpus=list(cpus)))]


def analyze(records, sequence, iterations, page_size):
    return native_tools.request('diagnostic-layout', request=dict(operation='cpu', records=records,
        sequence=list(sequence), iterations=iterations, page_size=page_size))


def run_worker(repository, environment, sequence, iterations, output, cpus):
    # A flag avoids raising between Popen spawning and assigning the owned child.
    # The bounded selector loop observes cancellation and executes its cleanup.
    requested = False
    def cancel(_signum, _frame):
        nonlocal requested
        requested = True
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    try:
        for sig in previous: signal.signal(sig, cancel)
        return _run_worker(repository, environment, sequence, iterations, output, cpus, lambda: requested)
    finally:
        for sig, handler in previous.items(): signal.signal(sig, handler)


def _run_worker(repository, environment, sequence, iterations, output, cpus, cancelled):
    """READY/END boundaries let the parent verify real affinity without Rust FFI."""
    env = dict(environment, RSE_CPU_ITERATIONS=str(iterations), RSE_CPU_SEQUENCE=','.join(map(str, sequence)),
               RSE_SAMPLER_PAGE_SIZE=str(sampling.base_page_size()))
    process = subprocess.Popen([*COMMAND, 'release_cpu_placement', '--', '--exact', '--ignored', '--nocapture', '--test-threads=1'],
                               cwd=repository, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, start_new_session=True)
    records, gates, pending = [], [], b''
    ready, ended, owned_tid = 0, 0, None
    completed = False
    try:
        with output.open('xb') as log, selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            deadline = time.monotonic()+300
            while True:
                if cancelled():
                    raise InterruptedError('placement worker cancelled; preserving completed observations')
                if time.monotonic() > deadline:
                    raise TimeoutError('placement worker exceeded its bounded runtime')
                events = selector.select(1)
                if not events:
                    continue
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                log.write(chunk); log.flush(); pending += chunk
                if len(pending) > 1024*1024:
                    raise ValueError('unexpected unbounded placement output')
                while b'\n' in pending:
                    line, pending = pending.split(b'\n', 1)
                    text = line.decode()
                    if text.startswith('RSE_CPU_READY '):
                        event = json.loads(text.removeprefix('RSE_CPU_READY '))
                        cpu, tid = event['cpu'], event['tid']
                        if any(type(event.get(k)) is not int for k in ('epoch', 'cpu', 'tid', 'budget_before', 'budget_after')) or tid <= 0 or event['epoch'] != ready or ended != ready or ready >= len(sequence) or cpu != sequence[ready] or event['budget_before'] != len(cpus) or event['budget_after'] != len(cpus):
                            raise ValueError('incorrect construction budget or placement handshake')
                        if os.getpgid(tid) != process.pid or owned_tid is not None and tid != owned_tid:
                            raise ValueError('refuse to pin a thread outside the owned worker session')
                        previous = set(cpus) if ready == 0 else {sequence[ready-1]}
                        if set(os.sched_getaffinity(tid)) != previous:
                            raise ValueError('measuring-thread affinity changed before handshake')
                        owned_tid = tid
                        os.sched_setaffinity(tid, {cpu})
                        if set(os.sched_getaffinity(tid)) != {cpu}:
                            raise ValueError('measuring-thread pin did not take effect')
                        gates.append({k: v for k, v in event.items() if k != 'tid'} | {'actual_affinity': [cpu]})
                        process.stdin.write(b'G'); process.stdin.flush(); ready += 1
                    elif text.startswith('RSE_CPU_SAMPLE '):
                        records.append(json.loads(text.removeprefix('RSE_CPU_SAMPLE ')))
                    elif text.startswith('RSE_CPU_END '):
                        event = json.loads(text.removeprefix('RSE_CPU_END '))
                        if ended >= len(sequence) or any(type(event.get(k)) is not int for k in ('epoch', 'cpu', 'available_parallelism_while_pinned')) or event.get('cached_policy_unchanged') is not True or event != {'epoch': ended, 'cpu': sequence[ended], 'cached_policy_unchanged': True, 'available_parallelism_while_pinned': 1} or ready != ended+1 or len(records) != ready*12 or set(os.sched_getaffinity(owned_tid)) != {sequence[ended]}:
                            raise ValueError('affinity/policy changed or incomplete epoch')
                        process.stdin.write(b'E'); process.stdin.flush()
                        ended += 1
        if process.wait(timeout=10) != 0 or ready != len(sequence) or ended != ready:
            raise ValueError('placement worker failed or ended before its full schedule')
        completed = True
        return records, gates
    finally:
        if not completed:
            try: os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError: pass
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL); process.wait(timeout=10)
        for pipe in (process.stdin, process.stdout): pipe.close()
        output.with_suffix('.samples.partial.json').write_text(json.dumps(records, indent=2)+'\n')
        output.with_suffix('.gates.partial.json').write_text(json.dumps(gates, indent=2)+'\n')


def capture(tool, repository, output, group, iterations):
    if type(iterations) is not int or not 1 <= iterations <= 1_000_000:
        raise ValueError('invalid diagnostic iteration count')
    if (tool/'scripts').resolve() != pathlib.Path(__file__).resolve().parent:
        raise ValueError('tool path differs from the imported diagnostic')
    source, controller = evidence.source_info(repository), evidence.source_info(tool)
    if source['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('diagnose the published 9.0.0 baseline first')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_')) and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    context = evidence.environment_context(repository, env, group)
    cpus = context['isolation']['cpu_affinity']
    if len(cpus) > 64: raise ValueError('placement diagnostic supports at most 64 reserved CPUs')
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    files = render(repository)
    metadata = {'schema_version': 'cpu-placement-diagnostic-v1', 'source': source, 'tool': controller,
                'context': context, 'protocol': sampling.PROTOCOL, 'iterations': iterations, 'case': CASE,
                'worker_plan': [{'name': name, 'sequence': sequence} for name, sequence in worker_plan(cpus)],
                'controller_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                'template_sha256': hashlib.sha256(TEMPLATE.read_bytes()).hexdigest(),
                'overlay_sha256': hashlib.sha256(b''.join(n.encode()+b'\0'+s.replace(str(repository), '<SOURCE>').encode() for n, s in sorted(files.items()))).hexdigest(),
                'purpose': 'ordinary serial encode placement diagnosis; cached parallel decision is hypothetical, not actual dispatch',
                'release_approval': False, 'candidate_attribution': None}
    def save(name, value): (output/name).write_text(json.dumps(value, indent=2)+'\n')
    save('provenance.json', metadata)
    workers = []
    with overlay(repository, files):
        with (output/'build.log').open('x') as log:
            subprocess.run([*COMMAND, '--no-run'], cwd=repository, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        for name, sequence in worker_plan(cpus):
            records, gates = run_worker(repository, env, sequence, iterations, output/(name+'.log'), cpus)
            detail = analyze(records, sequence, iterations, context['runtime']['page_size'])
            workers.append({'name': name, 'sequence': sequence, 'gates': gates, **detail})
            save('diagnosis.partial.json', dict(metadata, workers=workers))
            if evidence.source_info(repository) != source or evidence.source_info(tool) != controller or evidence.environment_context(repository, env, group) != context:
                raise ValueError('source or resource context changed')
    fresh = []
    for cpu in cpus:
        pair = [w['groups'][0]['median_ns_per_iter'] for w in workers if w['name'].startswith('fresh-') and w['sequence'] == [cpu]]
        if len(pair) != 2: raise ValueError('both fresh-process waves are required')
        fresh.append({'cpu': cpu, 'absolute_drift': abs(pair[1]/pair[0]-1)})
    report = dict(metadata, workers=workers, fresh_process_repeats=fresh)
    save('diagnosis.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    for name in ('tool', 'repository', 'output', 'isolation-cgroup'):
        parser.add_argument('--'+name, type=pathlib.Path)
    parser.add_argument('--iterations', type=int, default=384)
    args = parser.parse_args()
    if args.check:
        root = pathlib.Path(__file__).resolve().parents[1]
        with overlay(root, render(root, allocation_check=True)):
            subprocess.run([*COMMAND, '--', '--skip', 'release_controlled_profile', '--skip', 'release_cpu_placement'], cwd=root, env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'), check=True)
        return
    if any(getattr(args, n) is None for n in ('tool', 'repository', 'output', 'isolation_cgroup')):
        parser.error('capture requires tool, repository, output and isolation-cgroup')
    capture(args.tool.resolve(), args.repository.resolve(), args.output.resolve(), args.isolation_cgroup, args.iterations)


if __name__ == '__main__': main()
