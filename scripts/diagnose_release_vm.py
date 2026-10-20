#!/usr/bin/env python3
"""Collect a complete baseline-only resource diagnosis; never release evidence."""
import argparse
from contextlib import ExitStack
import time
import diagnose_adjacent_baseline as adjacent
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


FREQUENCY_SCHEMA='baseline-worker-frequency-v1'
FREQUENCY_REPETITIONS=4
MSRS={'tsc_before':0x10,'aperf':0xe8,'mperf':0xe7,'tsc_after':0x10}


class MsrReader:
    """Read-only guest-CPU counters; deliberately outside the Rust worker."""
    def __init__(self,stack,cpus):
        self.descriptors={}
        for cpu in cpus:
            fd=os.open(f'/dev/cpu/{cpu}/msr',os.O_RDONLY)
            stack.callback(os.close,fd);self.descriptors[str(cpu)]=fd

    def snapshot(self):
        result={'start_ns':time.monotonic_ns(),'cpus':{}}
        for cpu,fd in self.descriptors.items():
            point={};result['cpus'][cpu]=point
            for name,index in MSRS.items():
                raw=os.pread(fd,8,index)
                if len(raw)!=8:raise ValueError('short MSR read')
                point[name]=int.from_bytes(raw,'little')
        result['end_ns']=time.monotonic_ns()
        return result


def frequency_deltas(before,after,cpus,start,end):
    """Keep non-atomic read bounds and raw ratios; never scale API timings."""
    keys={str(c) for c in cpus}
    for snapshot in (before,after):
        if set(snapshot)!={'start_ns','end_ns','cpus'} or set(snapshot['cpus'])!=keys or any(type(snapshot[k]) is not int or snapshot[k]<0 for k in ('start_ns','end_ns')) or snapshot['end_ns']<snapshot['start_ns']:raise ValueError('invalid counter snapshot')
        for point in snapshot['cpus'].values():
            if set(point)!=set(MSRS) or any(type(v) is not int or not 0<=v<2**64 for v in point.values()) or point['tsc_after']<point['tsc_before']:raise ValueError('invalid counter point')
    if any(type(v) is not int or v<0 for v in (start,end)) or not before['end_ns']<=start<end<=after['start_ns']:raise ValueError('counter interval does not contain worker')
    result={}
    for cpu in sorted(keys,key=int):
        old,new=before['cpus'][cpu],after['cpus'][cpu]
        low=new['tsc_before']-old['tsc_after'];high=new['tsc_after']-old['tsc_before']
        aperf=new['aperf']-old['aperf'];mperf=new['mperf']-old['mperf']
        if low<=0 or aperf<0 or mperf<0 or (mperf==0)!=(aperf==0):raise ValueError('counter reset, wrap or invalid delta')
        result[cpu]={'aperf_delta':aperf,'mperf_delta':mperf,'tsc_delta_bounds':[low,high],
                     'aperf_per_mperf':aperf/mperf if mperf else None,
                     'mperf_per_tsc_bounds':[mperf/high,mperf/low],
                     'read_skew_tsc':[old['tsc_after']-old['tsc_before'],new['tsc_after']-new['tsc_before']]}
    if not sum(v['mperf_delta'] for v in result.values()):raise ValueError('no active counter delta')
    return result


def frequency_inputs(capture,prior,frozen):
    import analyze_baseline_variability as analysis
    report=analysis.analyze_adjacent(capture,prior,frozen)
    prov,counts,calibration,digests=adjacent.inputs(prior,frozen)
    raw=(capture/'adjacent/plan.json').read_bytes()
    if any(report['input_sha256'].get('prior/'+n)!=h for n,h in digests.items()) or evidence.digest(raw)!=report['input_sha256'].get('adjacent/plan.json'):raise ValueError('full input changed after replay')
    context=analysis.json_bytes(raw)['context']
    failed=sorted((v['profile'],v['case']) for v in report['cases'] if v['repeat']['exceeds_repeat_limit'])
    if not failed:raise ValueError('no full-catalog failures to diagnose')
    steps=[{'ordinal':i,'profile':p,'case':c,'repetition':r,'iterations':counts[p][c]}
           for i,(r,p,c) in enumerate((r,p,c) for r in range(FREQUENCY_REPETITIONS) for p,c in failed)]
    return prov,calibration,{'prior_and_capture_sha256':report['input_sha256'],'failed_cases':[p+'/'+c for p,c in failed],
                             'repetitions':FREQUENCY_REPETITIONS,'steps':steps},context


def frequency_result(output,plan,calibration,context):
    journal=output/'execution.jsonl'
    if journal.is_symlink() or not journal.is_file() or not 0<journal.stat().st_size<=evidence.MAX_BYTES:raise ValueError('bounded frequency journal required')
    entries=[json.loads(line) for line in journal.read_text().splitlines()]
    if len(entries)!=len(plan['cohort']['steps']):raise ValueError('incomplete frequency cohort')
    reports=[];last=0
    for step,entry in zip(plan['cohort']['steps'],entries):
        if set(entry)!={'step','before','after','start_ns','end_ns','counters_before','counters_after','samples_sha256','log_sha256'} or entry['step']!=step or entry['before']!=context or entry['after']!=context:raise ValueError('frequency order/context differs')
        stem=f"{step['ordinal']:03}";rows_path=adjacent.member(output,'workers/'+stem+'.json');log=adjacent.member(output,'workers/'+stem+'.log')
        rows=evidence.json_data(rows_path)
        if not log.is_file() or not 0<log.stat().st_size<=evidence.MAX_BYTES or evidence.digest(rows_path.read_bytes())!=entry['samples_sha256'] or evidence.digest(log.read_bytes())!=entry['log_sha256']:raise ValueError('frequency original members differ')
        begin=evidence.json_data(adjacent.member(output,'workers/'+stem+'.start.json'))
        if begin!={k:entry[k] for k in ('step','before','start_ns','counters_before')}:raise ValueError('frequency start journal differs')
        start,end=entry['start_ns'],entry['end_ns']
        if type(start) is not int or start<last:raise ValueError('frequency workers overlap')
        counters=frequency_deltas(entry['counters_before'],entry['counters_after'],context['isolation']['cpu_affinity'],start,end);last=end
        if end-start<sum(r['wall_ns'] for r in rows):raise ValueError('frequency worker cannot contain original windows')
        summary=adjacent.check_worker(rows,log.read_text(),step,calibration[step['profile']],context)
        cpu=str(rows[0]['placement']['cpu'])
        if counters[cpu]['mperf_delta']<=0:raise ValueError('no measuring-CPU counter activity')
        reports.append({'step':step,'original_metric':summary,'worker_elapsed_ns':end-start,
                        'original_wall_ns':sum(r['wall_ns'] for r in rows),'original_timed_ns':sum(r['timed_ns'] for r in rows),
                        'service':evidence.baseline_service_times(rows)[step['case']],'measuring_cpu':int(cpu),'counters':counters})
    return {'schema':FREQUENCY_SCHEMA,'release_approval':False,'formal_qualification':False,'candidate_attribution':None,
            'exclusive_api_cycles':False,'time_normalization':False,'scope':plan['scope'],
            'cases':len(plan['cohort']['failed_cases']),'observations':12*len(reports),'workers':reports,
            'plan_sha256':evidence.digest((output/'plan.json').read_bytes()),'execution_sha256':evidence.digest(journal.read_bytes())}


def replay_frequency(repository,capture,prior,frozen,output):
    if (output/'failure.json').exists():raise ValueError('failed frequency capture retained')
    prov,calibration,cohort,context=frequency_inputs(capture,prior,frozen)
    plan=evidence.json_data(output/'plan.json');tool=evidence.source_info(repository)
    if plan.get('schema')!=FREQUENCY_SCHEMA or plan.get('release_approval') is not False or plan.get('candidate_attribution') is not None or plan.get('cohort')!=cohort or plan.get('baseline')!=prov['source'] or plan.get('context')!=context or plan.get('controller_sha256')!=evidence.digest(pathlib.Path(__file__).read_bytes()) or plan.get('scope')!=FREQUENCY_SCOPE:raise ValueError('frequency plan differs')
    if evidence.source_at(repository,plan['tool']['commit'])!=plan['tool'] or any(tool[k]!=plan['tool'][k] for k in ('tree','lock_sha256')):raise ValueError('frozen frequency tool differs')
    result=frequency_result(output,plan,calibration,context)
    if evidence.json_data(output/'result.json')!=result:raise ValueError('frequency result differs')
    return result


FREQUENCY_SCOPE='whole original worker on guest CPUs, including setup, warmups, samples, export, kernel and other guest activity; virtualized counter fidelity and physical-host interference unproven; no exclusive API cycles, GHz inference or timing normalization'


def capture_frequency(repository,baseline,capture,prior,frozen,output,group):
    output.mkdir(parents=True,mode=0o700)
    try:
        prov,calibration,cohort,context=frequency_inputs(capture,prior,frozen)
        tool=evidence.source_info(repository);env=evidence.capture_environment()
        if evidence.source_info(baseline)!=prov['source'] or evidence.harness_digest(baseline)!=evidence.harness_digest(repository) or sampling.overlay_digest(baseline)!=sampling.overlay_digest(repository):raise ValueError('original baseline/harness/worker changed')
        if evidence.environment_context(baseline,env,group)!=context:raise ValueError('frequency context differs')
        plan={'schema':FREQUENCY_SCHEMA,'release_approval':False,'candidate_attribution':None,'baseline':prov['source'],
              'tool':tool,'controller_sha256':evidence.digest(pathlib.Path(__file__).read_bytes()),'cohort':cohort,'context':context,'scope':FREQUENCY_SCOPE}
        adjacent.dump(output/'plan.json',plan);(output/'workers').mkdir();(output/'counts').mkdir()
        with ExitStack() as stack:
            reader=MsrReader(stack,context['isolation']['cpu_affinity'])
            evidence.prepare_capture_source(stack,baseline,env,output/'build.log')
            with (output/'execution.jsonl').open('x') as ledger:
                for step in cohort['steps']:
                    if evidence.source_info(repository)!=tool or evidence.source_info(baseline)!=prov['source'] or evidence.environment_context(baseline,env,group)!=context:raise ValueError('frequency source/context changed')
                    stem=f"{step['ordinal']:03}";counts=output/'counts'/(stem+'.tsv');counts.write_text(step['case']+'\t'+str(step['iterations'])+'\n')
                    log=output/'workers'/(stem+'.log');point=reader.snapshot();start=time.monotonic_ns()
                    begin={'step':step,'before':context,'start_ns':start,'counters_before':point}
                    adjacent.dump(output/'workers'/(stem+'.start.json'),begin)
                    rows=sampling.run_profile(baseline,env,step['profile'],'sample',log,counts,cases={step['case']})
                    end=time.monotonic_ns()
                    try:after=reader.snapshot()
                    finally:adjacent.dump(output/'workers'/(stem+'.json'),rows)
                    entry=dict(begin,end_ns=end,counters_after=after,after=evidence.environment_context(baseline,env,group),
                               samples_sha256=evidence.digest((output/'workers'/(stem+'.json')).read_bytes()),log_sha256=evidence.digest(log.read_bytes()))
                    ledger.write(json.dumps(entry,allow_nan=False)+'\n');ledger.flush()
                    frequency_deltas(point,after,context['isolation']['cpu_affinity'],start,end)
                    adjacent.check_worker(rows,log.read_text(),step,calibration[step['profile']],context)
                    if entry['after']!=context:raise ValueError('frequency runtime changed')
        if evidence.source_info(baseline)!=prov['source'] or evidence.source_info(repository)!=tool or evidence.environment_context(baseline,env,group)!=context:raise ValueError('frequency source/context not restored')
        result=frequency_result(output,plan,calibration,context);adjacent.dump(output/'result.json',result)
        adjacent.dump(output/'replay.json',replay_frequency(repository,capture,prior,frozen,output));return result
    except BaseException as error:
        adjacent.dump(output/'failure.json',{'error':str(error),'release_approval':False,'candidate_attribution':None,'original_records_preserved':True});raise


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=pathlib.Path)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--isolation-cgroup',type=pathlib.Path)
    parser.add_argument('--frequency-from-adjacent',type=pathlib.Path)
    parser.add_argument('--prior',type=pathlib.Path)
    parser.add_argument('--frozen-tool',type=pathlib.Path)
    parser.add_argument('--replay-frequency',action='store_true')
    args=parser.parse_args()
    if args.frequency_from_adjacent:
        if not args.prior or not args.frozen_tool:parser.error('frequency mode requires --prior and trusted --frozen-tool')
        if not args.replay_frequency and (not args.baseline or not args.isolation_cgroup):parser.error('capture requires baseline and isolation cgroup')
    elif args.replay_frequency or args.prior or args.frozen_tool or not args.baseline or not args.isolation_cgroup:parser.error('legacy capture requires baseline/isolation; frequency options require --frequency-from-adjacent')
    try:
        if args.frequency_from_adjacent:
            repository=pathlib.Path(__file__).resolve().parents[1]
            if args.replay_frequency:result=replay_frequency(repository,args.frequency_from_adjacent.resolve(),args.prior.resolve(),args.frozen_tool.resolve(),args.output.resolve())
            else:result=capture_frequency(repository,args.baseline.resolve(),args.frequency_from_adjacent.resolve(),args.prior.resolve(),args.frozen_tool.resolve(),args.output.resolve(),args.isolation_cgroup.resolve())
            print(json.dumps({'cases':result['cases'],'observations':result['observations'],'release_approval':False,'formal_qualification':False}))
        else:capture(args.baseline.resolve(),args.output.resolve(),args.isolation_cgroup)
    except (OSError,ValueError,KeyError,TypeError,subprocess.CalledProcessError) as error:
        parser.exit(1,f'diagnostic capture failed; observations preserved: {error}\n')

if __name__=='__main__':main()
