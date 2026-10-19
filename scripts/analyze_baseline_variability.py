#!/usr/bin/env python3
"""Replay complete baseline samples to describe variability, never approve a release."""
import argparse
from collections import Counter
import hashlib
import json
import pathlib
import statistics
import subprocess
import sys

import diagnose_adjacent_baseline as adjacent

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


def repeat_counts(cases, phases):
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
    return {key: {'cases': count, 'repeat_failures': failures[key]} for key, count in sorted(categories.items())}


def add_group(cases, summaries, phase, profile, case, group, layouts):
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


def analyze(capture):
    members = {}
    def load(name):
        raw = (capture/name).read_bytes()
        members[name] = hashlib.sha256(raw).hexdigest()
        return json.loads(raw)

    provenance = load('provenance.json')
    schema = provenance.get('schema_version')
    if schema not in ('baseline-basic-controls-v1', 'baseline-qualification-v1', evidence.QUALIFICATION_SCHEMA) or provenance['source']['commit'] != evidence.BASELINE_COMMIT:
        raise ValueError('a complete published-9.0.0 baseline control is required')
    if schema in ('baseline-qualification-v1', evidence.QUALIFICATION_SCHEMA):
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
    if schema == evidence.QUALIFICATION_SCHEMA:
        evidence.validate_runtime(context.get('runtime'))
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
                add_group(cases, summaries, phase, profile, case, group, layouts)
    endpoints = repeat_counts(cases, phases)
    return {
        'schema_version': 'baseline-variability-analysis-v1',
        'purpose': 'descriptive replay of every baseline case; no causal or release inference',
        'release_approval': False, 'candidate_attribution': None,
        'baseline_commit': evidence.BASELINE_COMMIT, 'measured_tool': provenance['tool'],
        'protocol': protocol, 'renderer_sha256': renderer, 'capture_schema': schema,
        'kernel_memory_policy_recorded': schema == evidence.QUALIFICATION_SCHEMA,
        'analyzer_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'input_sha256': members, 'phase_order': list(phases), 'case_count': len(cases),
        'phase_quality': summaries,
        'repeat_endpoint_counts': endpoints,
        'limitations': [
            'Equal CPU endpoints do not exclude intermediate migration or prove affinity pinning.',
            'Main-thread counters omit Rayon worker scheduling and physical-host interference.',
            'Counters bracket the wall timer; run/wall ratios can exceed one and are not clamped.',
            'Matching caller offsets do not establish equal physical pages, cache placement or dynamic outputs.',
            'This replay does not replace original provenance/isolation checks or strict release preflight.',
        ],
        'cases': [cases[key] for key in sorted(cases)],
    }



SNAPSHOT_LIMIT = 256 * 1024 * 1024


def json_bytes(raw):
    return json.loads(raw, parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite JSON')))


def adjacent_members(capture, prior):
    if (capture/'adjacent').is_symlink():raise ValueError('symlinked adjacent capture')
    result = {name: adjacent.member(capture, name) for name in ('reservation/status.json','reservation/cleanup.json')}
    for name in ('plan.json','result.json','execution.jsonl'):
        result['adjacent/'+name] = capture/'adjacent'/name
    for index in range(252):
        for suffix in ('json','log'):
            name=f'workers/{index:03}.{suffix}'
            result['adjacent/'+name]=adjacent.member(capture/'adjacent',name)
    for name in ('provenance.json','qualification.partial.json'):
        result['prior/'+name]=prior/name
    for profile in evidence.FILES:
        name='calibration/'+profile+'-samples.json';result['prior/'+name]=adjacent.member(prior,name)
    return result


def snapshot_members(paths):
    total=0
    for name,path in paths.items():
        if path.is_symlink() or not path.is_file() or not 0<path.stat().st_size<=evidence.MAX_BYTES:raise ValueError('bounded regular original members required')
        total+=path.stat().st_size
        if total>SNAPSHOT_LIMIT:raise ValueError('adjacent snapshot exceeds byte budget')
    total=0
    for name,path in paths.items():
        with path.open('rb') as stream:raw=stream.read(evidence.MAX_BYTES+1)
        total+=len(raw)
        if not 0<len(raw)<=evidence.MAX_BYTES or total>SNAPSHOT_LIMIT:raise ValueError('adjacent snapshot changed beyond byte budget')
        yield name,raw


def frozen_source(tool, plan):
    recorded=plan.get('tool')
    if not isinstance(recorded,dict) or set(recorded)!={'commit','tree','lock_sha256'} or any(not evidence.REVISION.fullmatch(recorded.get(k,'')) for k in ('commit','tree')) or not evidence.DIGEST.fullmatch(recorded.get('lock_sha256','')):raise ValueError('invalid frozen source identity')
    actual=evidence.source_info(tool)
    if evidence.source_at(tool,recorded['commit'])!=recorded or any(actual[k]!=recorded[k] for k in ('tree','lock_sha256')):raise ValueError('frozen tool tree/lock differs')
    path=tool/'scripts/diagnose_adjacent_baseline.py'
    if path.is_symlink() or not path.is_file():raise ValueError('regular frozen controller required')
    committed=subprocess.check_output(['git','show',actual['commit']+':scripts/diagnose_adjacent_baseline.py'],cwd=tool)
    if path.read_bytes()!=committed or evidence.digest(committed)!=plan.get('controller_sha256'):raise ValueError('frozen controller bytes differ')
    return actual,path


def replay_frozen(tool, capture, prior, plan):
    before,path=frozen_source(tool,plan)
    # Explicit trusted checkout only: never fetch or execute a path from input data.
    launcher="import pathlib,runpy,sys; sys.path.insert(0,str(pathlib.Path(sys.argv[1]).parent)); sys.argv=sys.argv[1:]; runpy.run_path(sys.argv[0],run_name='__main__')"
    process=subprocess.run([sys.executable,'-I','-S','-B','-c',launcher,str(path),'replay','--prior',str(prior),'--output',str(capture/'adjacent')],cwd=tool,capture_output=True,text=True,check=True,timeout=120)
    summary=json_bytes(process.stdout)
    if not isinstance(summary,dict) or summary.get('release_approval') is not False or summary.get('formal_qualification') is not False or type(summary.get('cases')) is not int or summary['cases']!=126 or type(summary.get('observations')) is not int or summary['observations']!=3024 or any(type(summary.get(k)) is not int or not 0<=summary[k]<=126 for k in ('drift_failures','spread_failures')):raise ValueError('invalid frozen replay outcome')
    if frozen_source(tool,plan)[0]!=before:raise ValueError('frozen source changed during replay')
    return before,summary


def workload_gap(first, second):
    """Conservative wall-window bound; no inner API timestamps are inferred."""
    budgets=[]
    for entry,rows in (first,second):
        start,end=entry['start_ns'],entry['end_ns']
        if type(start) is not int or type(end) is not int or not 0<=start<end or not rows or any(type(r.get('wall_ns')) is not int or r['wall_ns']<=0 or r.get('kind') not in ('warmup','sample') for r in rows):raise ValueError('invalid integer wall/worker interval')
        wall=sum(r['wall_ns'] for r in rows);elapsed=end-start
        if wall>elapsed:raise ValueError('wall windows exceed containing worker')
        budgets.append({'worker_elapsed_ns':elapsed,'recorded_wall_ns':wall,'outside_wall_ns':elapsed-wall})
    gap=second[0]['start_ns']-first[0]['end_ns']
    if gap<0:raise ValueError('overlapping adjacent workers')
    bound=sum(v['outside_wall_ns'] for v in budgets)+gap
    warmup=sum(r['wall_ns'] for r in second[1] if r['kind']=='warmup')
    return {'parent_interworker_gap_ns':gap,'phases':dict(zip(('A1','A2'),budgets)),
            'pair_envelope_ns':second[0]['end_ns']-first[0]['start_ns'],
            'a2_warmup_wall_ns':warmup,'first_observation_gap_upper_ns':bound,
            'first_sample_gap_upper_ns':bound+warmup}


def refuse_failed_capture(capture):
    for name in ('failure.json','diagnostic-status.json'):
        path=capture/'adjacent'/name
        if path.exists() or path.is_symlink():raise ValueError('failed adjacent capture remains unaccepted')


def validate_resources(status, cleanup):
    if status.get('status')!='collected-diagnostic-only' or status.get('error') is not None or status.get('release_approval') is not False or type(status.get('isolation_checks')) is not int or status['isolation_checks']<=0 or any(cleanup.get(k) is not True for k in ('child_removed','controller_restored','parent_affinities_restored')) or cleanup.get('error') is not None:raise ValueError('complete capture and restored resources required')


def analyze_adjacent(capture, prior, tool):
    capture,prior,tool=(p.resolve() for p in (capture,prior,tool))
    refuse_failed_capture(capture)
    status=evidence.json_data(adjacent.member(capture,'reservation/status.json'))
    cleanup=evidence.json_data(adjacent.member(capture,'reservation/cleanup.json'))
    validate_resources(status,cleanup)
    paths=adjacent_members(capture,prior);original=dict(snapshot_members(paths))
    validate_resources(json_bytes(original['reservation/status.json']),json_bytes(original['reservation/cleanup.json']))
    plan=json_bytes(original['adjacent/plan.json'])
    if plan.get('schema')!=adjacent.SCHEMA:raise ValueError('require adjacent baseline diagnostic')
    provenance,counts,calibration,_=adjacent.inputs(prior,tool)
    adjacent.compare_context(plan['context'],provenance['context'])
    frozen,validated=replay_frozen(tool,capture,prior,plan)
    refuse_failed_capture(capture)
    # Compute exclusively from cached original bytes, after checking that the
    # entire validation interval preserved every capture and prior input member.
    for name,raw in snapshot_members(paths):
        if original[name]!=raw:raise ValueError('original members changed during frozen replay')
    entries=[json_bytes(line) for line in original['adjacent/execution.jsonl'].splitlines()]
    if len(entries)!=252 or not adjacent.same([v['step'] for v in entries],adjacent.schedule(counts)):raise ValueError('incomplete/reordered original adjacent catalog')
    cases={};summaries={p:{'wide_cases':[],'short_samples':0,'layout_mismatches':[]} for p in ('A1','A2')}
    for offset in range(0,252,2):
        pair=[]
        for entry in entries[offset:offset+2]:
            step=entry['step'];profile,case,phase=step['profile'],step['case'],step['phase']
            rows=json_bytes(original[f"adjacent/workers/{step['ordinal']:03}.json"])
            add_group(cases,summaries,phase,profile,case,rows,calibration[profile][1]);pair.append((entry,rows))
        cases[(profile,case)]['window_separation']=workload_gap(*pair)
    endpoints=repeat_counts(cases,('A1','A2'))
    if sum(r['repeat']['exceeds_repeat_limit'] for r in cases.values())!=validated['drift_failures'] or sum(any(r['phases'][p]['central_spread']>sampling.PROTOCOL['central_spread_limit'] for p in ('A1','A2')) for r in cases.values())!=validated['spread_failures']:raise ValueError('replayed statistics differ from frozen outcome')
    return {'schema_version':'baseline-adjacent-service-analysis-v1',
            'purpose':'complete-catalog descriptive service and conservative workload-window separation; no causal or release inference',
            'release_approval':False,'candidate_attribution':None,'formal_qualification':False,'new_acceptance_gate':False,
            'baseline_commit':evidence.BASELINE_COMMIT,'measured_tool':plan['tool'],'frozen_replay_source':frozen,
            'protocol':sampling.PROTOCOL,'renderer_sha256':sampling.implementation_digest(),
            'capture_schema':plan['schema'],'kernel_memory_policy_recorded':True,
            'prior_kernel_memory_policy_recorded':'kernel_memory_policy' in provenance['context']['runtime'],
            'analyzer_sha256':evidence.digest(pathlib.Path(__file__).read_bytes()),
            'input_sha256':{name:evidence.digest(raw) for name,raw in original.items()},
            'resources_restored':True,'frozen_replay_summary':validated,'phase_order':['A1','A2'],'case_count':126,
            'phase_quality':summaries,'repeat_endpoint_counts':endpoints,
            'maximum_pair_envelope_ns':max(r['window_separation']['pair_envelope_ns'] for r in cases.values()),
            'maximum_first_observation_gap_upper_ns':max(r['window_separation']['first_observation_gap_upper_ns'] for r in cases.values()),
            'maximum_first_sample_gap_upper_ns':max(r['window_separation']['first_sample_gap_upper_ns'] for r in cases.values()),
            'inner_api_gap_measured':False,
            'limitations':['Bounds are from the last A1 wall window to the first A2 warmup or measured-sample wall window, not inner API timestamps.',
                           'The pair envelope contains every workload window in both workers; median ranks are not treated as timestamps.',
                           'Both workers contribute all setup, teardown and between-observation overhead, making the bound conservative.',
                           'Observed bounds do not introduce or retroactively satisfy a new timing gate.',
                           'Historical missing kernel policy stays unknown; cross-campaign causal equivalence is not established.',
                           'Source instrumentation, dynamic/internal geometry and physical-host interference are not resolved by this timing bound.'],
            'cases':[cases[key] for key in sorted(cases)]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes=parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--capture', type=pathlib.Path)
    modes.add_argument('--adjacent-capture', type=pathlib.Path)
    parser.add_argument('--prior', type=pathlib.Path)
    parser.add_argument('--frozen-tool', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.output.exists() or args.output.is_symlink():parser.error('output must be a new file; original reports are preserved')
    if args.adjacent_capture:
        if not args.prior or not args.frozen_tool:parser.error('adjacent replay requires --prior and an explicit trusted --frozen-tool')
        result=analyze_adjacent(args.adjacent_capture,args.prior,args.frozen_tool)
    else:
        if args.prior or args.frozen_tool:parser.error('adjacent options require --adjacent-capture')
        result=analyze(args.capture)
    # Never overwrite a prior diagnosis or raw capture.
    with args.output.open('x') as output:
        output.write(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'analysis_written': True, 'case_count': result['case_count'],
                      'phase_order': result['phase_order'], 'release_approval': False}))


if __name__ == '__main__':
    main()
