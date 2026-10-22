"""Replayable stream owner lifecycles in disposable codec source copies.

These deliberately instrumented runs are never formal performance evidence.
The original per-call allocation/free work, loops and API results are retained.
"""
from contextlib import contextmanager
import argparse
import json
import os
import pathlib
import native_tools
import subprocess
import tempfile

import release_evidence as e
import release_sampling as sampling
import stream_view_audit as views
import verify_scratch_audit as ordinary
from diagnostic_overlay import overlay
from native_tools import STREAM_CORE_MODULE as MODULE, stream_source_patch as source_patch, verify_par_patch

PREFIX='RSE_STREAM_CORE '
NAMES=('src/lib.rs','src/core/stream.rs','src/core/verify.rs')


def native_template():
    return native_tools.native_template('stream')


def render(root):
    return native_tools.diagnostic_render(root, 'stream')


def patch_inputs(original):
    return {'src/lib.rs':original['src/lib.rs']+'\n#[doc(hidden)]\npub mod '+MODULE+';\n',
            'src/core/stream.rs':source_patch(original['src/core/stream.rs']),
            'src/core/verify.rs':verify_par_patch(original['src/core/verify.rs']),
            'src/'+MODULE+'.rs':native_template()}


def bindings(original):
    return {n:{'original_sha256':e.digest(original[n].encode()) if n in original else None,
               'diagnostic_sha256':e.digest(t.encode())} for n,t in patch_inputs(original).items()}


@contextmanager
def source_overlay(root):
    if any((root/n).is_symlink() or not (root/n).is_file() for n in NAMES):raise ValueError('require regular source-copy inputs')
    original={n:(root/n).read_text() for n in NAMES};changes=patch_inputs(original)
    added=root/('src/'+MODULE+'.rs')
    if added.exists() or added.is_symlink():raise ValueError('refuse an existing source observer')
    try:
        for n,t in changes.items():(root/n).write_text(t)
        yield bindings(original)
    finally:
        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text()!=t for n,t in changes.items()):
            raise ValueError('unexpected diagnostic source mutation; copy preserved')
        for n,t in original.items():(root/n).write_text(t)
        added.unlink()


def _shape(api,current,phase,slot,block,k,m,size,chunk):
    """Expected initialized/visible shape, occupancy, and minimum capacity.

    Capacity is measured, never inferred from the request. A minimum of zero
    means require empty storage; moving an owner clears its previous slot.
    """
    role,index=divmod(slot,48);blocks=(size+chunk-1)//chunk
    amount=min(chunk,size-(block-1)*chunk) if 1<=block<=blocks else 0
    previous=min(chunk,size-(block-2)*chunk) if block>1 else 0
    if api=='verify_par':
        length=m*amount
        return (0 if phase=='reserved' else length,0 if phase=='reserved' else length,True,length)
    if api in ('encode_stream','verify_stream'):
        if role==0:return (0 if phase=='reserved' else chunk,0 if phase=='reserved' else amount,True,chunk)
        if api=='verify_stream' and phase=='reserved':return (0,0,True,0)
        length=0 if phase=='reserved' or phase=='read' and block==1 else previous if phase=='read' else amount
        return length,length,True,chunk if api=='encode_stream' or length else 0
    missing=index in (0,k)
    if phase=='reserved':return 0,0,role==0,chunk if role==0 and not missing else 0
    if current:
        length=previous if phase=='read' and missing else amount
        return length,length,True,chunk if not missing or block>1 or phase!='read' else 0
    if role==1:
        if phase in ('read','recycled') or phase=='ready' and missing:return 0,0,False,0
        return amount,amount,True,chunk if not missing else amount
    if phase in ('ready','coded'):
        if not missing:return 0,0,True,0
        return previous,previous,True,previous
    length=previous if phase=='read' and missing else amount
    return length,length,True,chunk if not missing or block>1 or phase!='read' else 0


def validate(data,case,iterations,page,current,native=False):
    if case not in e.expected_cases('stream') or type(iterations) is not int or not 1<=iterations<=2 or not sampling.valid_page_size(page):raise ValueError('invalid stream diagnostic contract')
    if not isinstance(data,dict) or set(data)!={'schema','page_size','errors','calls','release_approval','events'} or data['schema']!='stream-core-lifecycle-v1' or data['release_approval'] is not False or type(data['page_size']) is not int or data['page_size']!=page or type(data['errors']) is not int or data['errors']!=0 or data['calls']!=[iterations]*12 or any(type(n) is not int for n in data['calls']):raise ValueError('invalid or incomplete stream-core ledger')
    events=data['events']
    if not isinstance(events,list) or not 1<=len(events)<=32768:raise ValueError('missing bounded source lifecycle')
    api,k,m,size,chunk=case.split(':');k,m,size,chunk=map(int,(k,m,size,chunk));blocks=(size+chunk-1)//chunk
    slots=list(range(k if api=='encode_stream' else k+m))
    if api=='encode_stream' or api=='verify_stream' and current:slots+=list(range(48,48+m))
    if api=='reconstruct_stream' and not current:slots+=list(range(48,48+k+m))
    variants={'encode_stream':'retained-prefix' if current else 'vec-views','verify_stream':'retained-scratch' if current else 'per-block-verify-par','reconstruct_stream':'planned-retained' if current else 'option-transfer','verify_par':'smallvec-per-block'}
    expected=[];scope=0
    # Expected former ranges are determined by the last observed owner shape,
    # not a guess based on allocation requests or the previous block's pointer.
    for observation in range(12):
        for call in range(1,iterations+1):
            scope+=1;outer=scope;latest={}
            def emit(name,token,parent,block,phase,slot=None,complete=False,valid=None):
                expected.append((observation,call,token,parent,name,variants[name],block,phase,slot,complete,valid))
            def group(phase,block):
                for slot in slots:
                    emit(api,outer,0,block,phase,slot)
                    latest[slot]=_shape(api,current,phase,slot,block,k,m,size,chunk)
            emit(api,outer,0,0,'enter');group('reserved',0)
            for block in range(1,blocks+2):
                group('read',block)
                if block>blocks:break
                group('ready',block)
                if api=='verify_stream' and not current:
                    scope+=1;child=scope;emit('verify_par',child,outer,block,'enter')
                    for phase in ('reserved','initialized','coded'):emit('verify_par',child,outer,block,phase,0)
                    emit('verify_par',child,outer,block,'released',0,True,True)
                    emit('verify_par',child,outer,block,'exit',None,True,True)
                group('coded',block)
                if api=='reconstruct_stream' and not current:
                    if block>1:
                        for index in (0,k):emit(api,outer,0,block,'replaced',index)
                    group('recycled',block)
            valid=True if api=='verify_stream' else None
            for slot,shape in latest.items():
                if shape[3]:emit(api,outer,0,blocks+1,'released',slot,True,valid)
            emit(api,outer,0,blocks+1,'exit',None,True,valid)
    if len(events)!=len(expected):raise ValueError('missing source owner/block/release boundary')
    fields={'observation','call','scope','parent','api','variant','block','phase','slot','len','capacity','visible_len','occupied','offsets','elapsed_ns','faults','snapshot','complete','valid'}
    order=('observation','call','scope','parent','api','variant','block','phase','slot','complete','valid')
    last_time=-1;last_fault=None;available=set();last_ranges={}
    for event,wanted in zip(events,expected):
        if not isinstance(event,dict) or set(event)!=fields or tuple(event[n] for n in order)!=wanted:raise ValueError('reordered or incomplete source scope')
        if any(type(event[n]) is not int or event[n]<0 for n in ('observation','call','scope','parent','block','len','capacity','visible_len','elapsed_ns')) or type(event['occupied']) is not bool or type(event['complete']) is not bool or event['valid'] is not None and type(event['valid']) is not bool or event['slot'] is not None and type(event['slot']) is not int:raise ValueError('invalid source event types')
        if not 0<=event['visible_len']<=event['len']<=event['capacity']<2**64:raise ValueError('invalid source capacity or initialized prefix')
        slot=event['slot'];phase=event['phase'];cap=event['capacity'];offset=event['offsets']
        if cap:
            if not isinstance(offset,list) or len(offset)!=3 or any(type(n) is not int or not 0<=n<mod for n,mod in zip(offset,(64,4096,page))) or offset[1]%64!=offset[0] or offset[2]%min(page,4096)!=offset[1]%min(page,4096):raise ValueError('invalid source offsets')
        elif offset is not None:raise ValueError('empty slot exports a dangling address')
        key=(event['scope'],slot)
        shape=tuple(event[n] for n in ('len','capacity','visible_len','occupied','offsets'))
        if slot is None:
            if shape!=(0,0,0,False,None):raise ValueError('source scope control contains storage')
        elif phase in ('released','replaced'):
            if key not in last_ranges or shape!=last_ranges[key] or not cap:raise ValueError('former range differs from last actual owner')
        else:
            length,visible,occupied,minimum=_shape(event['api'],current,phase,slot,event['block'],k,m,size,chunk)
            if (event['len'],event['visible_len'],event['occupied'])!=(length,visible,occupied) or (cap<minimum if minimum else cap!=0):raise ValueError('source container occupancy/initialized shape differs')
            last_ranges[key]=shape
        snap=event['snapshot'];pages=(offset[2]+cap+page-1)//page if cap else 0
        if not isinstance(snap,dict) or set(snap)!={'status','pages','resident_pages','errno'}:raise ValueError('invalid source residency')
        if snap['status']=='unsupported' and not native and cap:
            if any(snap[n] is not None for n in ('pages','resident_pages','errno')):raise ValueError('fabricated nonnative source residency')
        elif not cap:
            if type(snap['pages']) is not int or type(snap['resident_pages']) is not int or snap!={'status':'empty','pages':0,'resident_pages':0,'errno':None}:raise ValueError('invalid empty source range')
        elif snap['status']=='mapped':
            if type(snap['pages']) is not int or snap['pages']!=pages or type(snap['resident_pages']) is not int or not 0<=snap['resident_pages']<=pages or snap['errno'] is not None:raise ValueError('invalid native source page count')
        elif phase in ('released','replaced') and snap['status']=='not_fully_mapped':
            if type(snap['pages']) is not int or snap['pages']!=pages or snap['resident_pages'] is not None or type(snap['errno']) is not int or snap['errno']!=12:raise ValueError('invalid former source range query')
        else:raise ValueError('unavailable live source range')
        if event['elapsed_ns']<last_time:raise ValueError('nonmonotonic source boundary')
        last_time=event['elapsed_ns'];fault=event['faults'];available.add(fault is not None)
        if fault is not None:
            if not isinstance(fault,list) or len(fault)!=2 or any(type(n) is not int or n<0 for n in fault) or last_fault is not None and any(a>b for a,b in zip(last_fault,fault)):raise ValueError('invalid source fault counters')
            last_fault=fault
        elif native:raise ValueError('missing native source fault counters')
    if len(available)!=1:raise ValueError('changing source counter availability')
    return {'calls':12*iterations,'events':len(events),'nested_verify_calls':sum(v['api']=='verify_par' and v['phase']=='enter' for v in events)}


def worker_report(rows,io,core,case,source,page,cpus,current):
    normalized=views.underlying_records(rows)
    sampling.validate_workload_sequence(normalized,'stream')
    if len(rows)!=12 or len(io)!=12 or [r['kind'] for r in rows]!=['warmup']*3+['sample']*9:raise ValueError('incomplete original stream worker')
    views.validate_samples(io,rows,2,page,require_counters=cpus is not None)
    for index,(row,raw,view) in enumerate(zip(normalized,rows,io)):
        if any(type(row[n]) is not int for n in ('index','iterations','calibration_batch')) or row['index']!=index or row['case']!=case or row['iterations']!=2 or row['calibration_batch']!=0 or row['isolated_case'] is not True or row['revision']!=source['commit']:raise ValueError('wrong original stream observation')
        if set(e.indexed_rows('stream',row['rows'],source['commit'],{case:2},complete=False))!={case}:raise ValueError('wrong stream case rows')
        sampling.validate_fixture(row,'stream',page);sampling.validate_timing(row,'stream',2)
        if cpus is not None:
            sampling.validate_telemetry(row,cpus);sampling.validate_placement(row,cpus)
            for window in view['windows']:
                for counters in (window['before'],window['after']):
                    for i,field in enumerate(('process_minor_faults','process_major_faults','run_ns','wait_ns')):
                        if not row['before'][field]<=counters[i]<=row['after'][field]:raise ValueError('I/O window counters escape original observation')
        elif any(raw[n] is not None for n in ('before','after','placement')):raise ValueError('native stream samples require recorded affinity')
    summary=validate(core,case,2,page,current,native=cpus is not None)
    for event in core['events']:
        if cpus is None:
            if event['faults'] is not None:raise ValueError('unrecorded native source counters')
        else:
            row=rows[event['observation']]
            for i,field in enumerate(('process_minor_faults','process_major_faults')):
                if not row['before'][field]<=event['faults'][i]<=row['after'][field]:raise ValueError('source faults escape original stream observation')
    e.observation_contracts('stream',rows)
    return summary


def _original(repository,source):
    return {n:subprocess.check_output(['git','show',source['commit']+':'+n],cwd=repository).decode() for n in NAMES}


def worker_data(output,name,entry):
    member=output/(name+'.json');numeric=output/(name+'-journal.jsonl')
    if entry.get('path')!=member.name or entry.get('numeric_journal_path')!=numeric.name:raise ValueError('invalid stream member path')
    for path,digest in ((member,entry.get('sha256')),(numeric,entry.get('numeric_journal_sha256'))):
        if path.is_symlink() or not path.is_file() or not 0<path.stat().st_size<=e.MAX_BYTES or e.digest(path.read_bytes())!=digest:raise ValueError('stream diagnostic member digest differs')
    private=output/(name+'.log')
    if not e.DIGEST.fullmatch(entry.get('raw_log_sha256','')) or private.exists() and (private.is_symlink() or e.digest(private.read_bytes())!=entry['raw_log_sha256']):raise ValueError('private original stream log differs')
    data=e.json_data(member);raw=numeric.read_text();rows=sampling.parse_output(raw);io=sampling.parse_output(raw,views.PREFIX);core=sampling.parse_output(raw,PREFIX);journal=sampling.parse_output(raw,'RSE_TIMING ')
    if set(data)!={'samples','views','core'} or rows!=data['samples'] or io!=data['views'] or core!=[data['core']] or len(journal)!=12 or any(any(j[n]!=r[n] for n in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')) for j,r in zip(journal,rows)):raise ValueError('stream numerical journal differs from raw observations')
    return data,rows,io


def verify(output,repository):
    if any((output/n).exists() for n in ('failure.json','diagnostic-status.json')):raise ValueError('failed diagnostic remains unaccepted')
    report=e.json_data(output/'result.json')
    if report.get('schema')!='stream-core-check-v1' or report.get('release_approval') is not False or report.get('candidate_attribution') is not None or report.get('formal_performance_geometry_equivalence') is not False or report.get('source_copies') is not True:raise ValueError('invalid diagnostic-only source report')
    if report.get('native_sha256')!=e.digest(native_template().encode()) or report.get('sampler_implementation_sha256')!=sampling.implementation_digest() or not e.DIGEST.fullmatch(report.get('controller_sha256','')) or report.get('source_patcher_sha256')!=native_tools.implementation_digest():raise ValueError('source diagnostic implementation differs')
    page=report.get('page_size');cpus=report.get('cpu_affinity')
    if not sampling.valid_page_size(page) or cpus is not None and (not isinstance(cpus,list) or not cpus or any(type(c) is not int or c<0 for c in cpus) or cpus!=sorted(set(cpus))):raise ValueError('invalid native page/affinity contract')
    if report.get('harness_sha256')!={r:e.harness_digest(repository) for r in ('baseline','candidate')} or report.get('overlay_sha256')!={r:ordinary.overlay_digest(repository,render(repository)) for r in ('baseline','candidate')}:raise ValueError('stream workers differ')
    if set(report.get('sources',{}))!={'baseline','candidate'} or set(report.get('cases',{}))!={'baseline','candidate'}:raise ValueError('require both actual codec sources')
    if report['sources']['baseline']['commit']!=e.BASELINE_COMMIT or report['sources']['candidate']['commit']==e.BASELINE_COMMIT:raise ValueError('wrong published baseline or candidate')
    contracts={};workloads={};members={};total=0
    for role,source in report['sources'].items():
        if not isinstance(source,dict) or set(source)!={'commit','tree','lock_sha256'} or not e.REVISION.fullmatch(source.get('commit','')) or e.source_at(repository,source['commit'])!=source:raise ValueError('original stream source identity differs')
        original=_original(repository,source);current='struct StreamReadBuffer {' in original['src/core/stream.rs']
        if report.get('source_overlays',{}).get(role)!=bindings(original):raise ValueError('stream source overlays differ')
        if set(report['cases'][role])!=e.expected_cases('stream'):raise ValueError('incomplete original stream catalog')
        for case,entry in report['cases'][role].items():
            name=role+'-stream-'+case.replace(':','_')
            data,rows,io=worker_data(output,name,entry)
            summary=worker_report(rows,io,data['core'],case,source,page,cpus,current)
            if any(entry.get(n)!=v for n,v in summary.items()):raise ValueError('stream source summary differs')
            contracts[case]=e.observation_contracts('stream',rows,contracts.get(case));workload=sampling.workload_catalog(rows)
            if case in workloads and workloads[case]!=workload:raise ValueError('prepared stream workload differs across sources')
            workloads[case]=workload;members[entry['path']]=entry['sha256'];total+=summary['events']
    return {'schema':'stream-core-replay-v1','source_count':2,'case_count':15,'events':total,'members':members,
            'release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,
            'coverage':{'read_owners':True,'encode_parity_owners':True,'stream_verify_scratch':True,'stream_recovery_owners':True,'codec_internal_decode_scratch':False,'physical_allocation_identity':False,'first_fault_instruction':False},
            'auditor_sha256':e.digest(pathlib.Path(__file__).read_bytes())}


def _check(repository,output,baseline_ref):
    roles={r:e.source_at(repository,rev)['commit'] for r,rev in {'candidate':'HEAD','baseline':baseline_ref}.items()}
    if roles['baseline']!=e.BASELINE_COMMIT or roles['candidate']==roles['baseline']:raise ValueError('require published 9.0.0 and a distinct source')
    report={'schema':'stream-core-check-v1','source_copies':True,'release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,
            'sources':{},'cases':{},'source_overlays':{},'harness_sha256':{},'overlay_sha256':{},
            'sampler_implementation_sha256':sampling.implementation_digest(),'controller_sha256':e.digest(pathlib.Path(__file__).read_bytes()),
            'native_sha256':e.digest(native_template().encode()),'source_patcher_sha256':native_tools.implementation_digest(),
            'page_size':sampling.base_page_size(),'cpu_affinity':sorted(os.sched_getaffinity(0)) if hasattr(os,'sched_getaffinity') else None}
    env=dict(e.capture_environment(),RUSTUP_TOOLCHAIN='stable')
    for role,revision in roles.items():
        root=output/role;sampling.checkout_baseline(repository,root,revision);source=e.source_info(root);report['sources'][role]=source;report['cases'][role]={}
        files=render(root);report['harness_sha256'][role]=e.harness_digest(root);report['overlay_sha256'][role]=ordinary.overlay_digest(root,files)
        current='struct StreamReadBuffer {' in (root/'src/core/stream.rs').read_text()
        with source_overlay(root) as patches,overlay(root,files):
            report['source_overlays'][role]=patches
            with (output/(role+'-build.log')).open('x') as log:
                subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=root,env=sampling.build_environment(root,env),stdout=log,stderr=subprocess.STDOUT,check=True)
            for case in sorted(e.expected_cases('stream')):
                settings=dict(sampling.worker_environment(env),RSE_SAMPLER_PROFILE='stream',RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='2',RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(report['page_size']))
                name=role+'-stream-'+case.replace(':','_');path=output/(name+'.log');numeric=output/(name+'-journal.jsonl')
                try:
                    with path.open('x') as log:
                        proc=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=sampling.build_environment(root,settings),stdout=log,stderr=subprocess.STDOUT)
                finally:
                    if path.exists():numeric.write_text(''.join(line for line in path.read_text().splitlines(keepends=True) if line.startswith(('RSE_TIMING ','RSE_SAMPLE ',views.PREFIX,PREFIX))))
                proc.check_returncode();text=numeric.read_text();rows=sampling.parse_output(text);io=sampling.parse_output(text,views.PREFIX);core=sampling.parse_output(text,PREFIX)
                if len(core)!=1:raise ValueError('missing original stream source journal')
                summary=worker_report(rows,io,core[0],case,source,report['page_size'],report['cpu_affinity'],current)
                member=output/(name+'.json');member.write_text(json.dumps({'samples':rows,'views':io,'core':core[0]},indent=2)+'\n')
                report['cases'][role][case]=dict(summary,path=member.name,sha256=e.digest(member.read_bytes()),raw_log_sha256=e.digest(path.read_bytes()),numeric_journal_path=numeric.name,numeric_journal_sha256=e.digest(numeric.read_bytes()))
        if e.source_info(root)!=source:raise ValueError('original source was not restored')
    (output/'result.json').write_text(json.dumps(report,indent=2)+'\n');replay=verify(output,repository)
    (output/'replay.json').write_text(json.dumps(replay,indent=2)+'\n');return report


def check(repository,output,baseline_ref):
    output.mkdir(parents=True)
    try:return _check(repository,output,baseline_ref)
    except BaseException as error:
        (output/'diagnostic-status.json').write_text(json.dumps({'status':'diagnostic_failed','release_approval':False,'original_records_preserved':True})+'\n')
        (output/'failure.json').write_text(json.dumps({'error':str(error),'release_approval':False},indent=2)+'\n')
        raise


def main():
    parser=argparse.ArgumentParser(description=__doc__);modes=parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--check',action='store_true');modes.add_argument('--replay',type=pathlib.Path)
    parser.add_argument('--baseline-ref',default=e.BASELINE_COMMIT);parser.add_argument('--output',type=pathlib.Path);args=parser.parse_args()
    root=pathlib.Path(__file__).resolve().parents[1]
    if args.replay:
        if args.output is not None:parser.error('--output requires --check')
        print(json.dumps(verify(args.replay.resolve(),root),indent=2));return
    output=args.output or pathlib.Path(tempfile.mkdtemp(prefix='rse-stream-core-'))/'check'
    report=check(root,output.resolve(),args.baseline_ref)
    print(json.dumps({'source_count':2,'case_count':15,'release_approval':False,'output':str(output)}))

if __name__=='__main__':main()
