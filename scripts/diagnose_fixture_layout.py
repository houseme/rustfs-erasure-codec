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


PROFILES = ('small_file', 'stream')


def case_catalog(profile=None):
    return evidence.expected_cases(profile) if profile else set().union(*(evidence.expected_cases(p) for p in PROFILES))


def analyze(phases, sources, cpus, page_size=None):
    if set(phases) != set(evidence.PHASES):
        raise ValueError('all four layout inventory phases are required')
    counts = {case: 1 for case in case_catalog()}
    layouts = {}
    for phase in evidence.PHASES:
        revision = sources['baseline' if phase.startswith('A') else 'candidate']['commit']
        normalize = lambda p, row, rev: next(iter(evidence.indexed_rows(p, [row], rev, counts, complete=False)))
        if any(r.get('case') not in case_catalog() for r in phases[phase]):
            raise ValueError('unexpected inventory case')
        for profile in PROFILES:
            records = [r for r in phases[phase] if r['case'] in case_catalog(profile)]
            sampling.sample_groups(records, profile, revision, case_catalog(profile), counts, normalize, cpus, page_size)
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
            'case_count': len(counts), 'phase_count': len(phases), 'dynamic_output_layouts_observed': False,
            'layout_checks': checks, 'layout_mismatches': [c for c in checks if not c['matches']]}


def capture(tool, baseline, candidate, output, group):
    if pathlib.Path(sampling.__file__).resolve().parent != (tool/'scripts').resolve():
        raise ValueError('tool path does not identify the imported sampler')
    if output.exists(): raise ValueError('use a new inventory directory')
    output.mkdir(parents=True, mode=0o700)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RSE_', 'RS_', 'CARGO_PROFILE_'))
           and k not in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV)
    roots = {'baseline': baseline, 'candidate': candidate}
    sources = {role: evidence.source_info(root) for role, root in roots.items()}
    if sources['baseline']['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('the published 9.0.0 baseline is required')
    if sources['candidate']['commit'] == sources['baseline']['commit']:
        raise ValueError('a different candidate source is required')
    context = evidence.environment_context(candidate, env, group)
    cpu = min(context['isolation']['cpu_affinity'])
    overlay = sampling.overlay_digest(candidate)
    if sampling.overlay_digest(baseline) != overlay or evidence.harness_digest(baseline) != evidence.harness_digest(candidate):
        raise ValueError('both sources require identical harness inputs')
    metadata = {'schema_version': 'fixture-layout-inventory-v2', 'sources': sources,
                'tool': evidence.source_info(tool), 'context': context, 'measuring_affinity': [cpu],
                'constructor_affinity': context['isolation']['cpu_affinity'],
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
    counts = {}
    for profile in PROFILES:
        counts[profile] = output/('counts-'+profile+'.tsv')
        counts[profile].write_text(''.join(case+'\t1\n' for case in sorted(case_catalog(profile))))
    owned, phases = [], {}
    try:
        for role, root in roots.items():
            rendered = sampling.render(root)
            sampling.generate(root); owned.append((root, rendered))
            with (output/(role+'-build.log')).open('x') as log:
                subprocess.run(['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
                                '--test', 'rse_release_sampling', '--no-run'], cwd=root, env=sampling.build_environment(root,env),
                               stdout=log, stderr=subprocess.STDOUT, check=True)
        for phase in evidence.PHASES:
            role = 'baseline' if phase.startswith('A') else 'candidate'
            phases[phase], timings = [], []
            for profile in PROFILES:
                log = output/(phase+'-'+profile+'.log')
                try:
                    current = sampling.run_profile(roots[role], env, profile, 'sample', log,
                                                   counts[profile], cases=case_catalog(profile))
                finally:
                    if log.exists():
                        text = log.read_text()
                        partial = sampling.parse_output(text)
                        journal = sampling.parse_output(text, 'RSE_TIMING ')
                        save(phase+'-'+profile+'-samples.partial.json', partial)
                        save(phase+'-'+profile+'-timing.partial.json', journal)
                        save(phase+'-samples.partial.json', phases[phase]+partial)
                        save(phase+'-timing.partial.json', timings+journal)
                phases[phase].extend(current); timings.extend(journal)
            save(phase+'-samples.json', phases[phase]); unchanged()
        report = analyze(phases, sources, context['isolation']['cpu_affinity'], context['runtime']['page_size'])
        save('inventory.json', dict(metadata, **report)); unchanged()
        return report
    finally:
        for root, rendered in owned:
            directory = root/'tests/rse_release_sampling'
            expected = set(rendered)-{'root.rs'}
            if {p.name for p in directory.iterdir()} != expected or any((root/'tests/rse_release_sampling.rs' if name == 'root.rs' else directory/name).read_text() != value for name, value in rendered.items()):
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
