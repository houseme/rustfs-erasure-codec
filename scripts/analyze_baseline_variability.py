#!/usr/bin/env python3
"""Replay complete baseline samples to describe variability, never approve a release."""
import argparse
from collections import Counter
import hashlib
import json
import pathlib
import statistics

import release_evidence as evidence
import release_sampling as sampling


def describe(group, metric):
    """Retain warmups too; endpoint equality cannot exclude intermediate migration."""
    trace = []
    for record in group:
        before, after = record['before'], record['after']
        trace.append({
            'index': record['index'], 'kind': record['kind'],
            'iterations': record['iterations'], 'metric': record['rows'][0][metric],
            'wall_ns': record['wall_ns'], 'timed_ns': record['timed_ns'],
            'cpu_before': before['cpu'], 'cpu_after': after['cpu'],
            'main_run_ns': after['run_ns']-before['run_ns'],
            'main_wait_ns': after['wait_ns']-before['wait_ns'],
            'main_switches': after['switches']-before['switches'],
            'process_minor_faults': after['process_minor_faults']-before['process_minor_faults'],
            'process_major_faults': after['process_major_faults']-before['process_major_faults'],
            'main_run_to_wall': (after['run_ns']-before['run_ns'])/record['wall_ns'],
            'main_wait_to_wall': (after['wait_ns']-before['wait_ns'])/record['wall_ns'],
        })
    measured = trace[sampling.PROTOCOL['warmups']:]
    values = sorted(r['metric'] for r in measured)
    median = statistics.median(values)
    return {
        'median': median, 'central_spread': (values[-2]-values[1])/median,
        'short_samples': sum(r['wall_ns'] < sampling.PROTOCOL['minimum_wall_ns'] or
                             r['timed_ns'] < sampling.PROTOCOL['minimum_timed_ns'] for r in measured),
        'measured_endpoint_cpus': sorted({r[key] for r in measured for key in ('cpu_before', 'cpu_after')}),
        'measured_endpoint_change_count': sum(r['cpu_before'] != r['cpu_after'] for r in measured),
        'maximum_measured_main_wait_to_wall': max(r['main_wait_to_wall'] for r in measured),
        'trace': trace,
    }


def analyze(capture):
    members = {}
    def load(name):
        raw = (capture/name).read_bytes()
        members[name] = hashlib.sha256(raw).hexdigest()
        return json.loads(raw)

    provenance = load('provenance.json')
    schema = provenance.get('schema_version')
    if schema not in ('baseline-basic-controls-v1', 'baseline-qualification-v1') or provenance['source']['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('a complete published-9.0.0 baseline control is required')
    if schema == 'baseline-qualification-v1':
        contract = provenance.get('sampling', {})
        if (set(contract) != set(sampling.PROTOCOL) | {'implementation_sha256', 'overlay_sha256'}
                or not evidence.DIGEST.fullmatch(contract.get('overlay_sha256', ''))
                or provenance.get('release_approval') is not False or provenance.get('candidate_attribution') is not None):
            raise ValueError('invalid baseline qualification provenance')
        protocol = {k: contract[k] for k in sampling.PROTOCOL}
        renderer = contract['implementation_sha256']
    else:
        protocol, renderer = provenance['protocol'], provenance['renderer_sha256']
    if protocol != sampling.PROTOCOL or renderer != sampling.implementation_digest():
        raise ValueError('replay with the matching frozen protocol and renderer')
    context = provenance['context']
    cpus = context['isolation']['cpu_affinity']
    page_size = context['runtime']['page_size']
    phases = ('A1', 'A2') if (capture/'A2').exists() else ('A1',)
    cases, summaries = {}, {phase: {'wide_cases': [], 'short_samples': 0, 'layout_mismatches': []} for phase in phases}
    for profile in evidence.FILES:
        calibration = load(f'calibration/{profile}-samples.json')
        counts = evidence.calibration_counts(calibration, profile, evidence.BASELINE_COMMIT, page_size)
        for record in calibration:
            sampling.validate_telemetry(record, cpus)
            sampling.validate_placement(record, cpus)
        layouts = sampling.fixture_catalog(calibration, calibration=True)
        contracts = evidence.observation_contracts(profile, [r for r in calibration if r['complete']])
        workload = sampling.workload_catalog(calibration, calibration=True)
        inputs = sampling.workload_traces(calibration, calibration=True)
        normalize = lambda p, row, rev: next(iter(evidence.indexed_rows(p, [row], rev, counts, complete=False)))
        for phase in phases:
            records = load(f'{phase}/{profile}-samples.json')
            groups = sampling.sample_groups(records, profile, evidence.BASELINE_COMMIT,
                                            evidence.expected_cases(profile), counts, normalize, cpus, page_size)
            evidence.observation_contracts(profile, records, contracts)
            if (sampling.workload_catalog(records) != workload or
                    sampling.workload_traces(records) != inputs):
                raise ValueError(f'{phase}/{profile}: preparation or input replay differs from calibration')
            for case, group in sorted(groups.items()):
                key = (profile, case)
                detail = describe(group, evidence.METRICS[profile])
                detail['caller_layout_recorded'] = group[0]['fixture'] is not None
                detail['caller_matches_calibration'] = (group[0]['fixture'] == layouts[case]) if case in layouts else None
                cases.setdefault(key, {'profile': profile, 'case': case, 'metric': evidence.METRICS[profile], 'phases': {}})['phases'][phase] = detail
                summary = summaries[phase]
                if detail['central_spread'] > sampling.PROTOCOL['central_spread_limit']:
                    summary['wide_cases'].append({'profile': profile, 'case': case, 'central_spread': detail['central_spread']})
                summary['short_samples'] += detail['short_samples']
                if detail['caller_matches_calibration'] is False:
                    summary['layout_mismatches'].append({'profile': profile, 'case': case})
    categories, failures = Counter(), Counter()
    for result in cases.values():
        if len(phases) == 1:
            result['repeat'] = None
            continue
        first, last = (result['phases'][phase] for phase in phases)
        a, b = first['measured_endpoint_cpus'], last['measured_endpoint_cpus']
        category = ('single_endpoint_cpu_same' if a == b else 'single_endpoint_cpu_changed') if len(a) == len(b) == 1 else 'multiple_endpoint_cpus'
        drift = abs(last['median']-first['median'])/first['median']
        result['repeat'] = {'absolute_metric_drift': drift, 'exceeds_repeat_limit': drift > .05,
                            'endpoint_category': category}
        categories[category] += 1
        failures[category] += drift > .05
    return {
        'schema_version': 'baseline-variability-analysis-v1',
        'purpose': 'descriptive replay of every baseline case; no causal or release inference',
        'release_approval': False, 'candidate_attribution': None,
        'baseline_commit': evidence.BASELINE_COMMIT, 'measured_tool': provenance['tool'],
        'protocol': protocol, 'renderer_sha256': renderer, 'capture_schema': schema,
        'analyzer_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'input_sha256': members, 'phase_order': list(phases), 'case_count': len(cases),
        'phase_quality': summaries,
        'repeat_endpoint_counts': {key: {'cases': count, 'repeat_failures': failures[key]} for key, count in sorted(categories.items())},
        'limitations': [
            'Equal CPU endpoints do not exclude intermediate migration or prove affinity pinning.',
            'Main-thread counters omit Rayon worker scheduling and physical-host interference.',
            'Counters bracket the wall timer; run/wall ratios can exceed one and are not clamped.',
            'Matching caller offsets do not establish equal physical pages, cache placement or dynamic outputs.',
            'This replay does not replace original provenance/isolation checks or strict release preflight.',
        ],
        'cases': [cases[key] for key in sorted(cases)],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    result = analyze(args.capture)
    # Never overwrite a prior diagnosis or raw capture.
    with args.output.open('x') as output:
        output.write(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'analysis_written': True, 'case_count': result['case_count'],
                      'phase_order': result['phase_order'], 'release_approval': False}))


if __name__ == '__main__':
    main()
