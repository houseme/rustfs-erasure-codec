#!/usr/bin/env python3
"""Collect a complete baseline-only resource diagnosis; never release evidence."""
import argparse
import copy
import json
import os
import pathlib
import statistics
import subprocess

import release_evidence as evidence
import release_sampling as sampling


def basic_records(records):
    """Validate timing with the shared contract while retaining full raw records."""
    result=copy.deepcopy(records)
    for record in result:
        for side in ('before','after'):
            point=record[side]
            process=point.pop('process')
            if process.get('available') is not True or process.get('complete') is not True:
                raise ValueError('incomplete process observation')
            if not process.get('threads') or len({r['slot'] for r in process['threads']})!=len(process['threads']):
                raise ValueError('invalid anonymous thread set')
            if sum(r['measuring_thread'] is True for r in process['threads'])!=1:
                raise ValueError('one measuring thread is required')
            for key in ('minor_faults','major_faults','snapshot_ns'):
                if type(process.get(key)) is not int or process[key]<0:raise ValueError('invalid process counters')
            for thread in process['threads']:
                if type(thread.get('measuring_thread')) is not bool:raise ValueError('invalid measuring-thread flag')
                for key in ('slot','cpu','minor_faults','major_faults','run_ns','wait_ns','timeslices'):
                    if type(thread.get(key)) is not int or thread[key]<0:raise ValueError('invalid thread counters')
    return result


def analyze(records,profile):
    groups={}
    for record in records:
        if record['kind']=='sample':groups.setdefault(record['case'],[]).append(record)
    result=[]
    for case,group in groups.items():
        if len(group)!=sampling.PROTOCOL['samples']:raise ValueError('incomplete diagnostic samples')
        values=[r['timed_ns']/r['iterations'] for r in group]
        median=statistics.median(values);trace=[]
        for record,value in zip(group,values):
            before,after=(record[s]['process'] for s in ('before','after'))
            old={r['slot']:r for r in before['threads']};new={r['slot']:r for r in after['threads']}
            shared=old.keys()&new.keys();stable=old.keys()==new.keys()
            deltas={key:sum(new[t][key]-old[t][key] for t in shared) for key in ('run_ns','wait_ns','timeslices')}
            if any(v<0 for v in deltas.values()):raise ValueError('thread counters reset')
            fault_delta={key:after[key]-before[key] for key in ('minor_faults','major_faults')}
            if any(v<0 for v in fault_delta.values()):raise ValueError('process fault counters reset')
            trace.append({'index':record['index'],'relative_ns':value/median,'timed_ms':record['timed_ns']/1e6,
                'wall_ms':record['wall_ns']/1e6,'interval_sufficient':record['wall_ns']>=sampling.PROTOCOL['minimum_wall_ns'] and record['timed_ns']>=sampling.PROTOCOL['minimum_timed_ns'],'process_faults':fault_delta,'stable_thread_set':stable,
                'thread_count':len(new),'shared_thread_run_ms':deltas['run_ns']/1e6,'shared_thread_wait_ms':deltas['wait_ns']/1e6,
                'thread_cpu_migrations':sum(old[t]['cpu']!=new[t]['cpu'] for t in shared),
                'observer_ms':(before['snapshot_ns']+after['snapshot_ns'])/1e6})
        ordered=sorted(values)
        result.append({'profile':profile,'case':case,'central_ns_spread':(ordered[-2]-ordered[1])/median,'samples':trace})
    return result


def capture(baseline,output,group):
    if output.exists():raise ValueError('a new diagnostic directory is required')
    output.mkdir(parents=True,mode=0o700)
    env={k:v for k,v in os.environ.items() if not k.startswith(('RSE_','RS_','CARGO_PROFILE_')) and k not in ('CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER')}
    env.update(evidence.SAMPLING_ENV);env['RSE_SAMPLER_DIAGNOSTIC_PROCESS']='1'
    source=evidence.source_info(baseline)
    if source['commit']!=evidence.BASELINE_COMMIT:raise ValueError('diagnose the published 9.0.0 baseline only')
    context=evidence.environment_context(baseline,env,group)
    sampling.generate(baseline)
    with (output/'build.log').open('w') as log:
        subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=baseline,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    report={'schema_version':'baseline-vm-diagnostic-v1','purpose':'within-version diagnosis only; never release acceptance',
            'candidate_attribution':None,'source':source,'context':context,'diagnostic_tool':evidence.source_info(pathlib.Path(__file__).resolve().parents[1]),'renderer_sha256':sampling.implementation_digest(),
            'diagnostic_process_observer':True,'observer':'anonymous live threads plus process minor/major faults; snapshots outside timers; counters may include observation/setup effects',
            'cases':[]}
    (output/'diagnosis.partial.json').write_text(json.dumps(report,indent=2)+'\n')
    counts={}
    for profile in evidence.FILES:
        records=sampling.run_profile(baseline,env,profile,'calibrate',output/(profile+'-calibration.log'),cases=evidence.expected_cases(profile))
        (output/(profile+'-calibration.json')).write_text(json.dumps(records,indent=2)+'\n')
        counts[profile]=evidence.calibration_counts(basic_records(records),profile,source['commit'])
        if evidence.environment_context(baseline,env,group)!=context:raise ValueError('resource context changed during calibration')
    for profile in evidence.FILES:
        path=output/(profile+'-counts.tsv');path.write_text(''.join(f'{k}\t{v}\n' for k,v in sorted(counts[profile].items())))
        records=sampling.run_profile(baseline,env,profile,'sample',output/(profile+'.log'),path,cases=evidence.expected_cases(profile))
        (output/(profile+'-samples.json')).write_text(json.dumps(records,indent=2)+'\n')
        normalize=lambda p,row,rev: next(iter(evidence.indexed_rows(p,[row],rev,counts[p],complete=False)))
        sampling.sample_groups(basic_records(records),profile,source['commit'],evidence.expected_cases(profile),counts[profile],normalize,context['isolation']['cpu_affinity'])
        report['cases'].extend(analyze(records,profile))
        (output/'diagnosis.partial.json').write_text(json.dumps(report,indent=2)+'\n')
        if evidence.environment_context(baseline,env,group)!=context:raise ValueError('resource context changed')
    if evidence.source_info(baseline)!=source:raise ValueError('baseline source changed')
    report['status']='collected; diagnostic only'
    (output/'diagnosis.json').write_text(json.dumps(report,indent=2)+'\n')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=pathlib.Path,required=True)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--isolation-cgroup',type=pathlib.Path,required=True)
    args=parser.parse_args()
    try:capture(args.baseline.resolve(),args.output.resolve(),args.isolation_cgroup)
    except (OSError,ValueError,KeyError,TypeError,subprocess.CalledProcessError) as error:
        parser.exit(1,f'diagnostic capture failed; observations preserved: {error}\n')

if __name__=='__main__':main()
