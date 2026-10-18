#!/usr/bin/env python3
"""Inventory retained Vec layouts across versions; never measure release performance."""
import argparse
import hashlib
import json
import os
import pathlib
import subprocess

import release_evidence as evidence
import release_sampling as sampling


def case_catalog():
    return {case for case in evidence.expected_cases('small_file')
            if case.startswith('verify_with_buffer:')}


def analyze(phases, sources, cpus):
    if set(phases) != set(evidence.PHASES):
        raise ValueError('all four layout inventory phases are required')
    counts = {case: 1 for case in case_catalog()}
    layouts = {}
    for phase in evidence.PHASES:
        revision = sources['baseline' if phase.startswith('A') else 'candidate']['commit']
        normalize = lambda p, row, rev: next(iter(evidence.indexed_rows(p, [row], rev, counts, complete=False)))
        sampling.sample_groups(phases[phase], 'small_file', revision, case_catalog(), counts, normalize, cpus)
        layouts[phase] = sampling.fixture_catalog(phases[phase])
    checks = []
    for left, right in (('A1', 'A2'), ('B1', 'B2'), ('A1', 'B1'), ('A1', 'B2'), ('A2', 'B1'), ('A2', 'B2')):
        for case in sorted(case_catalog()):
            before, after = layouts[left][case], layouts[right][case]
            changed = []
            for role in ('source', 'scratch'):
                for index, (a, b) in enumerate(zip(before[role], after[role])):
                    fields = sorted(key for key in a if a[key] != b[key])
                    if fields: changed.append({'role': role, 'index': index, 'fields': fields})
            checks.append({'case': case, 'pair': left+'/'+right, 'matches': before == after,
                           'different_buffers': changed})
    return {'purpose': 'one-iteration structural layout inventory; no timing inference',
            'release_approval': False, 'candidate_attribution': None,
            'case_count': len(counts), 'phase_count': len(phases),
            'layout_checks': checks, 'layout_mismatches': [c for c in checks if not c['matches']]}


def capture(tool, baseline, candidate, output, group):
    if output.exists(): raise ValueError('use a new inventory directory')
    output.mkdir(parents=True, mode=0o700)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_'))
           and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    roots = {'baseline': baseline, 'candidate': candidate}
    sources = {role: evidence.source_info(root) for role, root in roots.items()}
    if sources['baseline']['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('the published 9.0.0 baseline is required')
    context = evidence.environment_context(candidate, env, group)
    cpu = min(context['isolation']['cpu_affinity'])
    overlay = sampling.overlay_digest(candidate)
    if sampling.overlay_digest(baseline) != overlay or evidence.harness_digest(baseline) != evidence.harness_digest(candidate):
        raise ValueError('both sources require identical harness inputs')
    metadata = {'schema_version': 'fixture-layout-inventory-v1', 'sources': sources,
                'tool': evidence.source_info(tool), 'context': context, 'measuring_affinity': [cpu],
                'protocol': sampling.PROTOCOL, 'overlay_sha256': overlay,
                'renderer_sha256': sampling.implementation_digest(),
                'controller_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                'purpose': 'one-iteration structural layout inventory; no timing inference',
                'candidate_attribution': None, 'release_approval': False}
    def save(name, value):
        (output/name).write_text(json.dumps(value, indent=2)+'\n')
    def unchanged():
        if evidence.environment_context(candidate, env, group) != context or any(evidence.source_info(root) != sources[role] for role, root in roots.items()):
            raise ValueError('source or resource context changed')
    save('provenance.json', metadata)
    counts = output/'counts.tsv'
    counts.write_text(''.join(case+'\t1\n' for case in sorted(case_catalog())))
    owned, phases = [], {}
    try:
        for role, root in roots.items():
            sampling.generate(root); owned.append(root)
            with (output/(role+'-build.log')).open('x') as log:
                subprocess.run(['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
                                '--test', 'rse_release_sampling', '--no-run'], cwd=root, env=env,
                               stdout=log, stderr=subprocess.STDOUT, check=True)
        for phase in evidence.PHASES:
            role = 'baseline' if phase.startswith('A') else 'candidate'
            affinity = os.sched_getaffinity(0)
            log = output/(phase+'.log')
            try:
                os.sched_setaffinity(0, {cpu})
                phases[phase] = sampling.run_profile(roots[role], env, 'small_file', 'sample', log,
                                                    counts, cases=case_catalog())
            finally:
                os.sched_setaffinity(0, affinity)
                if log.exists(): save(phase+'-samples.partial.json', sampling.parse_output(log.read_text()))
            save(phase+'-samples.json', phases[phase]); unchanged()
        report = analyze(phases, sources, {cpu})
        save('inventory.json', dict(metadata, **report)); unchanged()
        return report
    finally:
        for root in owned:
            directory = root/'tests/rse_release_sampling'
            expected = set(sampling.render(root))-{'root.rs'}
            if {p.name for p in directory.iterdir()} != expected:
                raise ValueError('unexpected generated input; preserve it for inspection')
            for path in directory.iterdir(): path.unlink()
            directory.rmdir(); (root/'tests/rse_release_sampling.rs').unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tool', type=pathlib.Path, required=True)
    parser.add_argument('--baseline', type=pathlib.Path, required=True)
    parser.add_argument('--candidate', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--isolation-cgroup', type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = capture(*(p.resolve() for p in (args.tool, args.baseline, args.candidate, args.output)), args.isolation_cgroup)
    print(json.dumps({'inventory_collected': True, 'layout_mismatches': len(report['layout_mismatches']),
                      'release_approval': False, 'candidate_attribution': None}))


if __name__ == '__main__': main()
