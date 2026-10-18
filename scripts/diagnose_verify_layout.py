#!/usr/bin/env python3
"""Reproduce the complete fixed-buffer layout diagnostic; never release evidence."""
import argparse
import hashlib
import json
import math
import os
import pathlib
import statistics
import subprocess

import release_evidence as evidence
import release_sampling as sampling

OFFSETS = (0, 16, 32, 48)
LENGTHS = (1024, 4096)
PREFIX = "RSE_LAYOUT_SAMPLE "


def parse_output(output):
    return sampling.parse_output(output, PREFIX)


def analyze(records, iterations):
    """Reject missing/reordered layouts; retain every warmup and observation."""
    if type(iterations) is not int or not 1 <= iterations <= 1_000_000_000:
        raise ValueError("invalid bounded iteration count")
    width = sampling.PROTOCOL['warmups'] + sampling.PROTOCOL['samples']
    grid = [(a, b) for a in OFFSETS for b in OFFSETS]
    schedule = [(length, order, a, b, index)
                for length in LENGTHS for order in (0, 1)
                for a, b in (grid if order == 0 else list(reversed(grid)))
                for index in range(width)]
    if len(records) != len(schedule):
        raise ValueError("complete forward/reverse grid and every observation are required")
    groups = {}
    for record, (length, order, source, scratch, index) in zip(records, schedule):
        fields = {'len': length, 'pass': order, 'source_offset': source,
                  'scratch_offset': scratch, 'index': index, 'iterations': iterations}
        if any(type(record.get(k)) is not int or record[k] != v for k, v in fields.items()):
            raise ValueError("layout order or iteration count differs from the complete grid")
        if record.get('entry') != 'slice_views' or record.get('fixture_lifetime') != 'same_backing_per_length':
            raise ValueError("explicit slice fixture contract is required")
        if record.get('kind') != ('warmup' if index < sampling.PROTOCOL['warmups'] else 'sample'):
            raise ValueError("all fixed warmups must precede the samples")
        wall, ns = record.get('wall_ns'), record.get('ns_per_iter')
        if type(wall) is not int or wall <= 0 or type(ns) not in (float, int) or not math.isfinite(ns) or ns <= 0 or not math.isclose(ns * iterations, wall, rel_tol=1e-12, abs_tol=1):
            raise ValueError("invalid layout timing")
        for key, count, offset in (('input_page_offsets', 6, source), ('scratch_page_offsets', 2, scratch)):
            positions = record.get(key)
            if not isinstance(positions, list) or len(positions) != count or any(type(p) is not int or not 0 <= p < 4096 or p % 64 != offset for p in positions):
                raise ValueError("actual buffer offsets differ from the selected layout")
        groups.setdefault((length, order, source, scratch), []).append(record)
    cases, repeats = [], []
    for key, group in groups.items():
        if any((r['input_page_offsets'], r['scratch_page_offsets']) != (group[0]['input_page_offsets'], group[0]['scratch_page_offsets']) for r in group):
            raise ValueError("buffer layout changed inside an observation group")
        values = sorted(r['ns_per_iter'] for r in group[sampling.PROTOCOL['warmups']:])
        median = statistics.median(values)
        cases.append(dict(zip(('len', 'pass', 'source_offset', 'scratch_offset'), key),
                          median_ns=median, central_spread=(values[-2]-values[1])/median))
    for length in LENGTHS:
        for a, b in grid:
            forward, reverse = (groups[(length, order, a, b)][0] for order in (0, 1))
            if (forward['input_page_offsets'], forward['scratch_page_offsets']) != (reverse['input_page_offsets'], reverse['scratch_page_offsets']):
                raise ValueError("forward/reverse passes did not reuse the same buffer layout")
            values = [statistics.median(r['ns_per_iter'] for r in groups[(length, order, a, b)][sampling.PROTOCOL['warmups']:]) for order in (0, 1)]
            repeats.append({'len': length, 'source_offset': a, 'scratch_offset': b,
                            'drift': abs(values[1]/values[0]-1)})
    return {'purpose': 'within-version layout diagnosis only; never release acceptance',
            'candidate_attribution': None, 'release_approval': False,
            'case_count': len(cases), 'formal_samples': len(cases)*sampling.PROTOCOL['samples'],
            'cases': cases, 'repeat_checks': repeats,
            'wide_cases': [r for r in cases if r['central_spread'] > .10],
            'repeat_failures': [r for r in repeats if r['drift'] > .05]}


def capture(repository, output, group, iterations):
    if type(iterations) is not int or not 1 <= iterations <= 1_000_000_000:
        raise ValueError("invalid bounded iteration count")
    if output.exists(): raise ValueError("use a new diagnostic directory")
    output.mkdir(mode=0o700, parents=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_'))
           and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    context = evidence.environment_context(repository, env, group)
    source = evidence.source_info(repository)
    measuring_cpu = min(context['isolation']['cpu_affinity'])
    template = sampling.TEMPLATES/'verify_layout.rs'
    target = repository/'tests/rse_verify_layout_diagnostic.rs'
    if target.exists(): raise ValueError("refuse to overwrite a diagnostic input")
    metadata = {'schema_version': 'verify-layout-diagnostic-v1', 'source': source,
                'tool': evidence.source_info(pathlib.Path(__file__).resolve().parents[1]),
                'context': context, 'iterations': iterations, 'measuring_cpu': measuring_cpu,
                'template_sha256': hashlib.sha256(template.read_bytes()).hexdigest(),
                'purpose': 'within-version layout diagnosis only; never release acceptance',
                'candidate_attribution': None, 'release_approval': False}
    (output/'provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
    # An exclusively created input is the only file removed by this controller.
    with target.open('x') as entry: entry.write(template.read_text())
    try:
        command = ['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
                   '--test', 'rse_verify_layout_diagnostic']
        with (output/'build.log').open('x') as log:
            subprocess.run([*command, '--no-run'], cwd=repository, env=env,
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        env['RSE_LAYOUT_ITERATIONS'] = str(iterations)
        try:
            with (output/'measurement.log').open('x') as log:
                subprocess.run(['taskset', '--cpu-list', str(measuring_cpu), *command,
                                'verify_alignment_control', '--', '--exact', '--ignored',
                                '--nocapture', '--test-threads=1'], cwd=repository, env=env,
                               stdout=log, stderr=subprocess.STDOUT, check=True)
        finally:
            records = parse_output((output/'measurement.log').read_text())
            (output/'samples.partial.json').write_text(json.dumps(records, indent=2)+'\n')
        (output/'samples.json').write_text(json.dumps(records, indent=2)+'\n')
        report = analyze(records, iterations)
        if evidence.source_info(repository) != source or evidence.environment_context(repository, env, group) != context:
            raise ValueError("source or resource context changed during diagnosis")
        (output/'diagnosis.json').write_text(json.dumps(dict(metadata, **report), indent=2)+'\n')
    finally:
        target.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--isolation-cgroup', type=pathlib.Path, required=True)
    parser.add_argument('--iterations', type=int, default=262144)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 1_000_000_000: parser.error('iteration count is outside the bounded range')
    capture(args.repository.resolve(), args.output.resolve(), args.isolation_cgroup, args.iterations)


if __name__ == '__main__': main()
