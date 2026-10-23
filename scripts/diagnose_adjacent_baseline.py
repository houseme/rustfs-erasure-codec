#!/usr/bin/env python3
"""All-case adjacent same-source diagnostic; never qualification or release evidence."""
import argparse
from contextlib import ExitStack
import json
import pathlib
import time
import native_tools

import release_evidence as e
import release_sampling as s

def same(a,b):
    return json.dumps(a,sort_keys=True,separators=(',',':'))==json.dumps(b,sort_keys=True,separators=(',',':'))


PHASES=('A1','A2')
SCHEMA='adjacent-baseline-diagnostic-v1'
MAX_PAIR_GAP_NS=10_000_000_000


def dump(path,value):
    with path.open('x') as stream:stream.write(json.dumps(value,indent=2,allow_nan=False)+'\n')


def without_policy(context):
    import copy
    result=copy.deepcopy(context);result['runtime'].pop('kernel_memory_policy',None);return result


def compare_context(context,prior):
    """Missing historical policy stays unknown; recorded policy must match."""
    e.validate_runtime(context.get('runtime'))
    if 'kernel_memory_policy' in prior['runtime']:
        e.validate_runtime(prior['runtime'])
        matches=same(context,prior)
    else:matches=same(without_policy(context),prior)
    if not matches:raise ValueError('recorded environment or kernel policy differs')


def member(root,name):
    parts=pathlib.PurePosixPath(name).parts
    if len(parts)!=2 or any(v in ('.','..') for v in parts) or pathlib.PurePosixPath(name).is_absolute():raise ValueError('invalid diagnostic member path')
    path=root
    for part in parts:
        path/=part
        if path.is_symlink():raise ValueError('diagnostic member is symlinked')
    return path


def inputs(prior,repository):
    """Old calibration is only a frozen workload budget, not new calibration."""
    prov=e.json_data(prior/'provenance.json')
    document=e.json_data(prior/'qualification.partial.json')
    if prov.get('schema_version') not in ('baseline-qualification-v1',e.QUALIFICATION_SCHEMA) or prov.get('release_approval') is not False or prov.get('candidate_attribution') is not None:raise ValueError('require original diagnostic baseline provenance')
    if prov['schema_version']==e.QUALIFICATION_SCHEMA:e.validate_runtime(prov['context']['runtime'])
    if prov.get('source')!=e.source_at(repository,e.BASELINE_COMMIT) or prov.get('tool')!=e.source_at(repository,prov['tool']['commit']):raise ValueError('original baseline/tool identity differs')
    if document.get('source')!=prov['source'] or document.get('context')!=prov['context'] or document.get('tool')!=prov['tool']:raise ValueError('prior control metadata differs')
    contract=prov['sampling']
    if any(contract.get(k)!=v for k,v in s.PROTOCOL.items()) or contract.get('implementation_sha256')!=s.implementation_digest() or contract.get('overlay_sha256')!=s.overlay_digest(repository) or prov.get('harness_sha256')!=e.harness_digest(repository):raise ValueError('frozen worker protocol/harness differs')
    names=['provenance.json','qualification.partial.json'];counts={};records={}
    for profile in e.FILES:
        name='calibration/'+profile+'-samples.json';entry=document['calibration'][profile]
        if entry.get('path')!=name:raise ValueError('prior calibration member differs')
        path=member(prior,name);rows=e.json_data(path)
        if e.digest(path.read_bytes())!=entry.get('sha256'):raise ValueError('prior calibration member differs')
        counts[profile]=e.calibration_counts(rows,profile,e.BASELINE_COMMIT,prov['context']['runtime']['page_size'],allowed_cpus=prov['context']['isolation']['cpu_affinity'])
        records[profile]=(e.observation_contracts(profile,[r for r in rows if r['complete']]),s.fixture_catalog(rows,calibration=True),s.workload_catalog(rows,calibration=True),s.workload_traces(rows,calibration=True));names.append(name)
    if document['sampling'].get('common_iterations')!=counts:raise ValueError('prior frozen counts differ')
    return prov,counts,records,{n:e.digest((prior/n).read_bytes()) for n in names}


def schedule(counts):
    return native_tools.request('baseline-analysis', request=dict(operation='schedule', counts=counts))


def check_worker(rows,raw,step,calibration,context):
    profile=step['profile'];case=step['case'];counts={case:step['iterations']}
    timing=s.parse_output(raw,'RSE_TIMING ')
    if not same(s.parse_output(raw),rows) or len(timing)!=12 or any(not same({k:t[k] for k in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')},{k:r[k] for k in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')}) for t,r in zip(timing,rows)):raise ValueError('raw original worker journal differs')
    selected,widths=s.selected_row(rows,profile,e.METRICS[profile],e.BASELINE_COMMIT,{case},counts,context['isolation']['cpu_affinity'],context['runtime']['page_size'])
    reference,fixtures,workloads,traces=calibration
    e.observation_contracts(profile,rows,reference)
    for actual,wanted,label in ((s.fixture_catalog(rows),fixtures,'caller geometry'),(s.workload_catalog(rows),workloads,'workload'),(s.workload_traces(rows),traces,'input replay')):
        if actual.get(case)!=wanted.get(case):raise ValueError(label+' differs from frozen input; no temporal attribution')
    if [r['kind'] for r in rows]!=['warmup']*3+['sample']*9 or any(r['calibration_batch']!=0 for r in rows):raise ValueError('original warmup/sample schedule differs')
    return {'metric':e.METRICS[profile],'value':float(selected[0][e.METRICS[profile]]),'central_spread':widths[0]['central_spread']}


def replay(output,prior,repository):
    if (output/'failure.json').exists():raise ValueError('failed diagnostic is retained, not accepted')
    prov,counts,calibration,digests=inputs(prior,repository);plan=e.json_data(output/'plan.json');result=e.json_data(output/'result.json')
    if plan.get('schema')!=SCHEMA or plan.get('release_approval') is not False or plan.get('candidate_attribution') is not None or not same(plan.get('steps'),schedule(counts)) or plan.get('prior_inputs')!=digests or plan.get('baseline')!=prov['source'] or plan.get('max_pair_gap_ns')!=MAX_PAIR_GAP_NS:raise ValueError('predeclared adjacent plan differs')
    if plan.get('controller_sha256')!=e.digest(pathlib.Path(__file__).read_bytes()) or not isinstance(plan.get('tool'),dict) or e.source_at(repository,plan['tool']['commit'])!=plan['tool'] or any(plan['tool'][k]!=e.source_info(repository)[k] for k in ('tree','lock_sha256')):raise ValueError('use the exact frozen diagnostic tool source')
    context=plan['context'];compare_context(context,prov['context'])
    journal=output/'execution.jsonl'
    if journal.is_symlink() or not journal.is_file() or not 0<journal.stat().st_size<=e.MAX_BYTES:raise ValueError('invalid bounded execution journal')
    entries=[json.loads(line) for line in journal.read_text().splitlines()]
    if len(entries)!=252:raise ValueError('incomplete original adjacent case catalog')
    summaries={};last_end=0;gaps=[]
    for step,entry in zip(plan['steps'],entries):
        if set(entry)!={'step','before','after','start_ns','end_ns','samples_sha256','log_sha256'} or not same(entry['step'],step) or not same(entry['before'],context) or not same(entry['after'],context):raise ValueError('execution order or runtime context differs')
        start,end=entry['start_ns'],entry['end_ns']
        if type(start) is not int or type(end) is not int or not last_end<=start<end:raise ValueError('invalid worker execution interval')
        gap=start-last_end
        if step['phase']=='A2':
            if gap>MAX_PAIR_GAP_NS:raise ValueError('paired workers are not temporally adjacent')
            gaps.append(gap)
        last_end=end
        stem=f"{step['ordinal']:03}";path=member(output,'workers/'+stem+'.json');log=member(output,'workers/'+stem+'.log')
        rows=e.json_data(path)
        if log.is_symlink() or not log.is_file() or not 0<log.stat().st_size<=e.MAX_BYTES or e.digest(path.read_bytes())!=entry['samples_sha256'] or e.digest(log.read_bytes())!=entry['log_sha256']:raise ValueError('original worker members differ')
        if end-start<sum(r['wall_ns'] for r in rows):raise ValueError('worker interval cannot contain its original observations')
        summary=check_worker(rows,log.read_text(),step,calibration[step['profile']],context)
        summaries.setdefault(step['profile']+'/'+step['case'],{})[step['phase']]=summary
    expected=summarize(output,prov,summaries,gaps)
    if not same(result,expected):raise ValueError('diagnostic summary differs from original workers')
    return expected


def summarize(output,prov,summaries,gaps):
    statistics=native_tools.request('baseline-analysis', request=dict(operation='adjacent-checks', summaries=list(summaries.items()), gaps=gaps))
    return {'schema':SCHEMA,'release_approval':False,'candidate_attribution':None,'formal_qualification':False,'baseline':prov['source'],'cases':126,'observations':3024,'execution_sha256':e.digest((output/'execution.jsonl').read_bytes()),'plan_sha256':e.digest((output/'plan.json').read_bytes()),**statistics}


def capture(repository,baseline,prior,output,group):
    output.mkdir(parents=True)
    try:return _capture(repository,baseline,prior,output,group)
    except BaseException as error:
        dump(output/'failure.json',{'status':'failed','error':str(error),'release_approval':False,'candidate_attribution':None,'original_records_preserved':True});raise


def _capture(repository,baseline,prior,output,group):
    prov,counts,calibration,digests=inputs(prior,repository)
    if e.source_info(baseline)!=prov['source'] or e.harness_digest(baseline)!=e.harness_digest(repository) or s.overlay_digest(baseline)!=s.overlay_digest(repository):raise ValueError('require unchanged original baseline/worker/harness')
    env=e.capture_environment();context=e.environment_context(baseline,env,group);tool=e.source_info(repository)
    compare_context(context,prov['context'])
    plan={'schema':SCHEMA,'release_approval':False,'candidate_attribution':None,'baseline':prov['source'],'tool':tool,'controller_sha256':e.digest(pathlib.Path(__file__).read_bytes()),'prior_inputs':digests,'context':context,'max_pair_gap_ns':MAX_PAIR_GAP_NS,'steps':schedule(counts)}
    dump(output/'plan.json',plan);(output/'workers').mkdir();(output/'counts').mkdir();summaries={};last_end=None;gaps=[]
    with ExitStack() as stack:
        e.prepare_capture_source(stack,baseline,env,output/'build.log')
        with (output/'execution.jsonl').open('x') as ledger:
            for step in plan['steps']:
                if e.source_info(repository)!=tool or e.source_info(baseline)!=prov['source']:raise ValueError('diagnostic source changed')
                before=e.environment_context(baseline,env,group)
                if before!=context:raise ValueError('runtime context changed')
                stem=f"{step['ordinal']:03}";count_file=output/'counts'/(stem+'.tsv');count_file.write_text(step['case']+'\t'+str(step['iterations'])+'\n')
                log=output/'workers'/(stem+'.log');start=time.monotonic_ns()
                if step['phase']=='A2' and start-last_end>MAX_PAIR_GAP_NS:raise ValueError('paired worker start gap exceeded; no retry')
                rows=s.run_profile(baseline,env,step['profile'],'sample',log,count_file,cases={step['case']})
                end=time.monotonic_ns();after=e.environment_context(baseline,env,group)
                member=output/'workers'/(stem+'.json');dump(member,rows)
                entry={'step':step,'before':before,'after':after,'start_ns':start,'end_ns':end,'samples_sha256':e.digest(member.read_bytes()),'log_sha256':e.digest(log.read_bytes())}
                ledger.write(json.dumps(entry)+'\n');ledger.flush()
                if after!=context:raise ValueError('runtime context changed; original worker retained')
                summary=check_worker(rows,log.read_text(),step,calibration[step['profile']],context)
                summaries.setdefault(step['profile']+'/'+step['case'],{})[step['phase']]=summary
                if step['phase']=='A2':gaps.append(start-last_end)
                last_end=end
                progress=output/'progress.tmp';progress.write_text(json.dumps({'completed_steps':step['ordinal']+1,'total_steps':252,'release_approval':False})+'\n');progress.replace(output/'progress.json')
    if e.source_info(baseline)!=prov['source'] or e.source_info(repository)!=tool or e.environment_context(baseline,env,group)!=context:raise ValueError('source/context not restored')
    result=summarize(output,prov,summaries,gaps)
    dump(output/'result.json',result);dump(output/'replay.json',replay(output,prior,repository));return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('mode',choices=('capture','replay'));parser.add_argument('--prior',type=pathlib.Path,required=True);parser.add_argument('--output',type=pathlib.Path,required=True);parser.add_argument('--baseline',type=pathlib.Path);parser.add_argument('--isolation-cgroup',type=pathlib.Path);args=parser.parse_args();repository=pathlib.Path(__file__).resolve().parents[1]
    if args.mode=='capture':
        if not args.baseline or not args.isolation_cgroup:parser.error('capture requires --baseline and --isolation-cgroup')
        result=capture(repository,args.baseline.resolve(),args.prior.resolve(),args.output.resolve(),args.isolation_cgroup.resolve())
    else:result=replay(args.output.resolve(),args.prior.resolve(),repository)
    print(json.dumps({'cases':result['cases'],'observations':result['observations'],'release_approval':False,'formal_qualification':False,'drift_failures':sum(c['drift']>s.REPEAT_DRIFT_LIMIT for c in result['checks']),'spread_failures':sum(any(v>s.PROTOCOL['central_spread_limit'] for v in c['widths']) for c in result['checks'])}))

if __name__=='__main__':main()
