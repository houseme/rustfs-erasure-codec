#!/usr/bin/env python3
"""Replay complete baseline samples to describe variability, never approve a release."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import native_tools

import diagnose_adjacent_baseline as adjacent

import release_evidence as evidence
import release_sampling as sampling


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
    descriptions = []
    for profile in evidence.FILES:
        calibration = load(f'calibration/{profile}-samples.json')
        counts = evidence.calibration_counts(calibration, profile, evidence.BASELINE_COMMIT, page_size, allowed_cpus=cpus)
        layouts = sampling.fixture_catalog(calibration, calibration=True)
        contracts = evidence.observation_contracts(profile, [r for r in calibration if r['complete']])
        workload = sampling.workload_catalog(calibration, calibration=True)
        inputs = sampling.workload_traces(calibration, calibration=True)
        for phase in phases:
            records = load(f'{phase}/{profile}-samples.json')
            details = sampling.describe_groups(records, profile, evidence.BASELINE_COMMIT,
                                               evidence.expected_cases(profile), counts, layouts, cpus, page_size)
            evidence.observation_contracts(profile, records, contracts)
            if (sampling.workload_catalog(records) != workload or
                    sampling.workload_traces(records) != inputs):
                raise ValueError(f'{phase}/{profile}: preparation or input replay differs from calibration')
            descriptions.extend(dict(profile=profile, phase=phase, **detail) for detail in details)
    summary = native_tools.request('baseline-analysis', request=dict(operation='report', phases=list(phases), groups=descriptions))
    return {
        'schema_version': 'baseline-variability-analysis-v1',
        'purpose': 'descriptive replay of every baseline case; no causal or release inference',
        'release_approval': False, 'candidate_attribution': None,
        'baseline_commit': evidence.BASELINE_COMMIT, 'measured_tool': provenance['tool'],
        'protocol': protocol, 'renderer_sha256': renderer, 'capture_schema': schema,
        'kernel_memory_policy_recorded': schema == evidence.QUALIFICATION_SCHEMA,
        'analyzer_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'input_sha256': members, 'phase_order': list(phases),
        **summary,
        'limitations': [
            'Equal CPU endpoints do not exclude intermediate migration or prove affinity pinning.',
            'Main-thread counters omit Rayon worker scheduling and physical-host interference.',
            'Counters bracket the wall timer; run/wall ratios can exceed one and are not clamped.',
            'Matching caller offsets do not establish equal physical pages, cache placement or dynamic outputs.',
            'This replay does not replace original provenance/isolation checks or strict release preflight.',
        ],
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
    descriptions = []
    for profile in evidence.FILES:
        groups = []
        for entry in entries:
            step = entry['step']
            if step['profile'] != profile: continue
            groups.append(dict(profile=profile, case=step['case'], phase=step['phase'],
                records=json_bytes(original[f"adjacent/workers/{step['ordinal']:03}.json"]),
                fixture=calibration[profile][1].get(step['case']),
                entry={key: entry[key] for key in ('start_ns', 'end_ns')}))
        descriptions.extend(native_tools.request('baseline-analysis', request=dict(operation='describe', groups=groups)))
    summary = native_tools.request('baseline-analysis', request=dict(operation='report', phases=list(adjacent.PHASES),
        groups=descriptions, frozen={key: validated[key] for key in ('drift_failures','spread_failures')}))
    return {'schema_version':'baseline-adjacent-service-analysis-v1',
            'purpose':'complete-catalog descriptive service and conservative workload-window separation; no causal or release inference',
            'release_approval':False,'candidate_attribution':None,'formal_qualification':False,'new_acceptance_gate':False,
            'baseline_commit':evidence.BASELINE_COMMIT,'measured_tool':plan['tool'],'frozen_replay_source':frozen,
            'protocol':sampling.PROTOCOL,'renderer_sha256':sampling.implementation_digest(),
            'capture_schema':plan['schema'],'kernel_memory_policy_recorded':True,
            'prior_kernel_memory_policy_recorded':'kernel_memory_policy' in provenance['context']['runtime'],
            'analyzer_sha256':evidence.digest(pathlib.Path(__file__).read_bytes()),
            'input_sha256':{name:evidence.digest(raw) for name,raw in original.items()},
            'resources_restored':True,'frozen_replay_summary':validated,'phase_order':['A1','A2'],**summary,
            'inner_api_gap_measured':False,
            'limitations':['Bounds are from the last A1 wall window to the first A2 warmup or measured-sample wall window, not inner API timestamps.',
                           'The pair envelope contains every workload window in both workers; median ranks are not treated as timestamps.',
                           'Both workers contribute all setup, teardown and between-observation overhead, making the bound conservative.',
                           'Observed bounds do not introduce or retroactively satisfy a new timing gate.',
                           'Historical missing kernel policy stays unknown; cross-campaign causal equivalence is not established.',
                           'Source instrumentation, dynamic/internal geometry and physical-host interference are not resolved by this timing bound.'],
            }


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
