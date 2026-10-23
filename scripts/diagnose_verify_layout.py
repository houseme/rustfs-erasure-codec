#!/usr/bin/env python3
"""Capture and replay complete paired slice layouts; never replace Vec release evidence."""
import argparse
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import tempfile
import native_tools

from diagnostic_overlay import overlay
import release_evidence as evidence
import release_sampling as sampling

PAIRED_SCHEMA = 'verify-layout-paired-v2'
PHASE_SCHEMA = 'verify-layout-diagnostic-v3'
PREFIX = "RSE_LAYOUT_SAMPLE "
TEMPLATE = pathlib.Path(__file__).resolve().parent/'diagnostics/verify_layout.rs'
TOOL = pathlib.Path(__file__).resolve().parents[1]
COMMAND = ['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
           '--test', 'rse_release_sampling']
LAYOUT_PROTOCOL = native_tools.request('diagnostic-layout', request=dict(operation='protocol'))
OFFSETS = tuple(LAYOUT_PROTOCOL['offsets'])
LENGTHS = tuple(LAYOUT_PROTOCOL['lengths'])


def parse_output(output):
    return sampling.parse_output(output, PREFIX)


def analyze(records, iterations, page_size=None, measuring_cpu=None):
    return native_tools.request('diagnostic-layout', request=dict(operation='verify', records=records,
        iterations=iterations, page_size=page_size, measuring_cpu=measuring_cpu))


def save(output, name, value):
    (output/name).write_text(json.dumps(value, indent=2, allow_nan=False)+'\n')


def tool_contract():
    return {'protocol': LAYOUT_PROTOCOL,
            'template_sha256': hashlib.sha256(TEMPLATE.read_bytes()).hexdigest(),
            'controller_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
            'sampling_implementation_sha256': sampling.implementation_digest()}


def run_checked(command, **kwargs):
    """Cancel and reap only this invocation's owned process group before cleanup."""
    cancelled, process = False, None
    def cancel(signum, frame):
        nonlocal cancelled
        cancelled = True
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
    try:
        for sig in previous: signal.signal(sig, cancel)
        # The handler only sets a flag, including while Popen assigns ownership.
        process = subprocess.Popen(command, start_new_session=True, **kwargs)
        while process.poll() is None and not cancelled:
            try: process.wait(timeout=.1)
            except subprocess.TimeoutExpired: pass
        if cancelled: raise KeyboardInterrupt('layout worker cancelled')
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, command)
    finally:
        try:
            if process is not None and process.poll() is None:
                try: os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError: pass
                try: process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=2)
        finally:
            for sig, handler in previous.items(): signal.signal(sig, handler)


def capture(repository, output, group, iterations):
    if type(iterations) is not int or not 1 <= iterations <= 1_000_000_000:
        raise ValueError("invalid bounded iteration count")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_'))
           and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    env = sampling.build_environment(repository, env)
    context = evidence.environment_context(repository, env, group)
    source, tool = evidence.source_info(repository), evidence.source_info(TOOL)
    measuring_cpu = min(context['isolation']['cpu_affinity'])
    page_size = context['runtime']['page_size']
    metadata = {'schema_version': PHASE_SCHEMA, 'source': source,
                'tool': tool, 'context': context, 'iterations': iterations,
                'measuring_cpu': measuring_cpu, **tool_contract(),
                'purpose': 'supplemental controlled slices; never replace original Vec release cases',
                'candidate_attribution': None, 'release_approval': False}
    save(output, 'provenance.json', metadata)
    # Reuse the same exact-byte ownership contract as formal release captures.
    with overlay(repository, {'root.rs': TEMPLATE.read_text()}):
        with (output/'build.log').open('x') as log:
            run_checked([*COMMAND, '--no-run'], cwd=repository, env=env,
                           stdout=log, stderr=subprocess.STDOUT)
        env.update(RSE_LAYOUT_ITERATIONS=str(iterations), RSE_LAYOUT_PAGE_SIZE=str(page_size),
                   RSE_LAYOUT_CPU=str(measuring_cpu))
        log_path = output/'measurement.log'
        records = None
        try:
            with log_path.open('x') as log:
                run_checked(['taskset', '--cpu-list', str(measuring_cpu), *COMMAND,
                                'verify_alignment_control', '--', '--exact', '--ignored',
                                '--nocapture', '--test-threads=1'], cwd=repository, env=env,
                               stdout=log, stderr=subprocess.STDOUT)
        finally:
            # Keep complete rows from an interrupted worker before propagating failure.
            if log_path.exists():
                try:
                    records = parse_output(log_path.read_text())
                    save(output, 'samples.partial.json', records)
                except (ValueError, TypeError) as error:
                    (output/'extraction-error.txt').write_text(str(error)+'\n')
        if records is None:
            raise ValueError('invalid numeric journal; preserve the raw log and extraction error')
        save(output, 'samples.json', records)
        report = analyze(records, iterations, page_size, measuring_cpu)
        if evidence.source_info(repository) != source or evidence.source_info(TOOL) != tool or evidence.environment_context(repository, env, group) != context:
            raise ValueError("source or resource context changed during diagnosis")
    # A cleanup failure must never leave a completed report.
    save(output, 'diagnosis.json', dict(metadata, **report))
    return report


def phase_passed(report):
    return not any(report[k] for k in ('short_samples', 'wide_cases', 'repeat_failures'))


def analyze_paired(phases, iterations, page_size, measuring_cpu, *, diagnostic=False):
    return native_tools.request('diagnostic-layout', request=dict(operation='paired', phases=phases,
        iterations=iterations, page_size=page_size, measuring_cpu=measuring_cpu, diagnostic=diagnostic))


def verify_paired(directory, candidate, compiler=True, *, diagnostic=False, manifest_sha256=evidence._UNBOUND):
    """Bind Git/compiler state around one native original-directory replay."""
    request=dict(directory=str(directory.resolve()),diagnostic=diagnostic,contract=tool_contract(),
                 baseline=evidence.source_at(candidate,evidence.BASELINE_COMMIT),
                 candidate=evidence.source_info(candidate),tool=evidence.source_info(TOOL))
    if manifest_sha256 is not evidence._UNBOUND:request['manifest_sha256']=manifest_sha256
    result=native_tools.request('diagnostic-layout',request=dict(operation='paired-replay',request=request))
    if compiler and result['context']['compiler'] != evidence.command(['rustc', '--version'], candidate, dict(os.environ, RUSTUP_TOOLCHAIN='stable')):
        raise ValueError('paired compiler differs from the current compiler')
    return result


def capture_paired(baseline, candidate, output, group, iterations, *, diagnostic=False):
    sources = {role: evidence.source_info(root) for role, root in (('baseline', baseline), ('candidate', candidate))}
    if sources['baseline']['commit'] != evidence.BASELINE_COMMIT or sources['baseline']['tree'] == sources['candidate']['tree']:
        raise ValueError('pair the published 9.0.0 with a different candidate')
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    document = {'schema_version': PAIRED_SCHEMA, 'phase_order': list(evidence.PHASES),
                'contract': tool_contract(), 'sources': sources, 'tool': evidence.source_info(TOOL),
                'files': {}, 'release_approval': False}
    if diagnostic: document['diagnostic_only'] = True
    save(output, 'manifest.partial.json', document)
    try:
        context = None
        for phase in evidence.PHASES:
            root = baseline if phase.startswith('A') else candidate
            report = capture(root, output/phase, group, iterations)
            metadata = evidence.json_data(output/phase/'provenance.json')
            if metadata['source'] != sources['baseline' if phase.startswith('A') else 'candidate'] or metadata['tool'] != document['tool'] or context is not None and metadata['context'] != context:
                raise ValueError('paired source/tool/resource context changed')
            context = metadata['context']
            document['files'][phase] = {name: {'path': phase+'/'+name, 'sha256': evidence.digest((output/phase/name).read_bytes())}
                                        for name in ('provenance.json', 'samples.json', 'measurement.log')}
            save(output, 'manifest.partial.json', document)
            if diagnostic:
                print(json.dumps({'diagnostic_layout_phase': phase, 'case_count': report['case_count'],
                                  'quality_passed': phase_passed(report)}), flush=True)
            if not diagnostic and not phase_passed(report):
                raise evidence.EvidenceFailure('layout phase quality failed; stop later phases', {'phase': phase, 'report': report})
        save(output, 'diagnostic-manifest.json' if diagnostic else 'manifest.json', document)
        report = verify_paired(output, candidate, diagnostic=True) if diagnostic else verify_paired(output, candidate)
        save(output, 'pairing.json', report)
        if not diagnostic and not report['layout_gate_passed']:
            raise evidence.EvidenceFailure('paired layout drift or regression failed', report)
        return report
    except (OSError, ValueError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        save(output, 'failure.json', {'error': str(error), 'details': getattr(error, 'details', None),
                                     'candidate_attribution': None, 'release_approval': False})
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--verify', type=pathlib.Path)
    for name in ('repository', 'baseline', 'candidate', 'output', 'isolation-cgroup'):
        parser.add_argument('--'+name, type=pathlib.Path)
    parser.add_argument('--iterations', type=int, default=262144)
    args = parser.parse_args()
    if args.check:
        with overlay(TOOL, {'root.rs': TEMPLATE.read_text()}):
            env = dict(os.environ, RUSTUP_TOOLCHAIN='stable')
            run_checked(COMMAND, cwd=TOOL, env=env)
            if hasattr(os, 'sched_getaffinity'):
                # Exercise real Linux worker wiring and JSON, with one iteration
                # for correctness only. This is never a performance capture.
                cpu, page_size = min(os.sched_getaffinity(0)), sampling.base_page_size()
                env.update(RSE_LAYOUT_ITERATIONS='1', RSE_LAYOUT_PAGE_SIZE=str(page_size), RSE_LAYOUT_CPU=str(cpu))
                with tempfile.TemporaryFile(mode='w+') as log:
                    run_checked(['taskset', '--cpu-list', str(cpu), *COMMAND,
                                 'verify_alignment_control', '--', '--exact', '--ignored',
                                 '--nocapture', '--test-threads=1'], cwd=TOOL, env=env,
                                stdout=log, stderr=subprocess.STDOUT)
                    log.seek(0)
                    report = analyze(parse_output(log.read()), 1, page_size, cpu)
                    assert report['release_approval'] is False
                print('Complete Linux layout worker/schema checked at one iteration; no performance acceptance')
        return
    if args.verify:
        if not args.candidate: parser.error('--verify requires --candidate')
        report = verify_paired(args.verify.resolve(), args.candidate.resolve())
        print(json.dumps(report))
        if not report['layout_gate_passed']: raise SystemExit(1)
        return
    if not args.output or not args.isolation_cgroup or not 1 <= args.iterations <= 1_000_000_000:
        parser.error('capture requires a new output, reserved cgroup and bounded iterations')
    if args.baseline and args.candidate and not args.repository:
        capture_paired(args.baseline.resolve(), args.candidate.resolve(), args.output.resolve(), args.isolation_cgroup, args.iterations)
    elif args.repository and not args.baseline and not args.candidate:
        capture(args.repository.resolve(), args.output.resolve(), args.isolation_cgroup, args.iterations)
    else:
        parser.error('choose --repository or both --baseline and --candidate')


if __name__ == '__main__': main()
