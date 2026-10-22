"""Unified remaining byte-storage/allocator inventory; invoked by fixture inventory.

All modifications are reversible source-copy observers. No production allocator,
sampler, algorithm, original allocation statement or release gate is changed.
"""
from contextlib import contextmanager
import json
import os
import pathlib
import native_tools
import subprocess

import release_evidence as e
import release_sampling as s
from diagnostic_overlay import overlay

MODULE = '__rse_dynamic_buffers'
PREFIX = 'RSE_DYNAMIC_BUFFERS '
_catalog = native_tools.request('diagnostic-ledger', request=dict(kind='dynamic-catalog'))
PROFILES = tuple(_catalog['profiles'])
MODES = tuple(_catalog['modes'])
SUPPLEMENTS = _catalog['supplements']
NAMES = ('src/lib.rs', 'src/matrix.rs', 'src/core/reconstruct.rs', 'src/galois_8/policy.rs') + native_tools.LEOPARD_FILES


def native_template():
    return native_tools.native_template('dynamic')


def patch_matrix(text):
    return native_tools.request('matrix-source', text=text)


def patch_decode(text):
    return native_tools.request('decode-source', text=text)


def patch_inputs(original):
    return native_tools.request('dynamic-source', original=original, tool=str(native_tools.ROOT))


def bindings(original):
    return {name: {'original_sha256': e.digest(original[name].encode()) if name in original else None,
                   'diagnostic_sha256': e.digest(text.encode())} for name, text in patch_inputs(original).items()}


@contextmanager
def source_overlay(root):
    if any((root/n).is_symlink() or not (root/n).is_file() for n in NAMES): raise ValueError('require regular source-copy inputs')
    original = {n: (root/n).read_text() for n in NAMES}
    changes = patch_inputs(original)
    added = root/('src/'+MODULE+'.rs')
    if added.exists() or added.is_symlink(): raise ValueError('preserve an existing dynamic observer')
    try:
        for name, text in changes.items(): (root/name).write_text(text)
        yield bindings(original)
    finally:
        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text() != text for n, text in changes.items()):
            raise ValueError('unexpected diagnostic source change; preserve copy')
        for name, text in original.items(): (root/name).write_text(text)
        added.unlink()


def render(root):
    return native_tools.diagnostic_render(root, 'dynamic')
# Shared strict capture/replay contract.


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= 2**64-1


def inventory_steps():
    steps=[]
    for role in ('candidate','baseline'):
        steps.extend({'role':role,'mode':mode,'profile':profile,'case':case}
                     for mode in MODES for profile in PROFILES for case in sorted(e.expected_cases(profile)))
        steps.extend({'role':role,'mode':'structure','profile':'decoder','case':case} for case in SUPPLEMENTS)
    return steps


def supplement_report(header,ledger,case,page,cpus,source):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='dynamic-supplement', header=header, data=ledger, case=case, page=page,
        cpus=cpus, legacy=source['commit']==e.BASELINE_COMMIT))


def validate_ledger(data,profile,case,page,native=False,legacy=False):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='dynamic', data=data, profile=profile, case=case, page=page, native=native, legacy=legacy))


def underlying_records(records):
    """Validate a private copy; raw stored observations retain their marker."""
    return native_tools.request('diagnostic-ledger', request=dict(kind='dynamic-records', records=records))


def validate_envelope(report,output,repository):
    fields={'schema','release_approval','candidate_attribution','formal_performance_geometry_equivalence','tool','sources','cases','source_overlays','harness_sha256','overlay_sha256','context','page_size','cpu_affinity','native_sha256','sampler_implementation_sha256','controller_sha256'}
    if set(report)!=fields:raise ValueError('unexpected dynamic inventory envelope')
    context=report['context']
    if not isinstance(context,dict) or set(context)!={'compiler','machine','cpu_identity','cpu_affinity','page_size','kernel_memory_policy','libc','allocator_environment','settings'}:raise ValueError('missing dynamic runtime provenance')
    if any(not isinstance(context[k],str) or not context[k] for k in ('compiler','machine','cpu_identity')) or context['cpu_affinity']!=report['cpu_affinity'] or context['page_size']!=report['page_size'] or context['settings']!=e.SAMPLING_ENV:raise ValueError('dynamic runtime/settings differ')
    if not isinstance(context['libc'],list) or len(context['libc'])!=2 or any(not isinstance(v,str) for v in context['libc']) or not isinstance(context['allocator_environment'],dict) or any(not e.allocator_override_key(k) or not isinstance(v,str) for k,v in context['allocator_environment'].items()):raise ValueError('invalid dynamic allocator provenance')
    if report['cpu_affinity'] is not None:native_tools.validate_memory_policy(context['kernel_memory_policy'],report['page_size'])
    elif context['kernel_memory_policy'] is not None:raise ValueError('unrecorded native memory policy')
    tool=report['tool']
    for source in (tool,*report['sources'].values()):
        if not isinstance(source,dict) or set(source)!={'commit','tree','lock_sha256'} or not isinstance(source['commit'],str) or not e.REVISION.fullmatch(source['commit']) or not isinstance(source['tree'],str) or not e.REVISION.fullmatch(source['tree']) or not isinstance(source['lock_sha256'],str) or not e.DIGEST.fullmatch(source['lock_sha256']):raise ValueError('invalid source/tool identity')
        if source!=e.source_at(repository,source['commit']):raise ValueError('original dynamic source/tool differs')
    if tool!=report['sources']['candidate']:raise ValueError('dynamic tool/candidate identity differs')
    for key in ('cases','source_overlays','harness_sha256','overlay_sha256'):
        if set(report[key])!={'candidate','baseline'}:raise ValueError('incomplete per-source dynamic envelope')
    plan=e.json_data(output/'plan.json')
    steps=inventory_steps()
    if set(plan)!=fields|{'steps'} or plan['steps']!=steps or plan['cases']!={'candidate':{},'baseline':{}} or any(plan[k]!=report[k] for k in fields-{'cases'}):raise ValueError('frozen complete dynamic plan differs')
    path=output/'execution.jsonl'
    if path.is_symlink() or not path.is_file() or not 0<path.stat().st_size<=e.MAX_BYTES:raise ValueError('bounded original execution journal required')
    entries=[json.loads(line) for line in path.read_text().splitlines()]
    if len(entries)!=2*len(steps):raise ValueError('incomplete original execution journal')
    for i,step in enumerate(steps):
        begin,end=entries[2*i:2*i+2]
        entry=report['cases'][step['role']][step['mode']+'/'+step['profile']+'/'+step['case']]
        if begin!={'stage':'begin','step':step,'context':context} or end!={'stage':'end','step':step,'context':context,'entry':entry}:raise ValueError('dynamic execution order/context/member differs')

def worker_report(rows,ledger,profile,case,source,page,cpus,mode):
    rows=underlying_records(rows)
    if mode not in MODES or len(rows)!=12:raise ValueError('require complete diagnostic mode observations')
    kinds=(['calibration_warmup']*3+['calibration']*9) if mode=='calibrate' else (['warmup']*3+['sample']*9)
    if [r['kind'] for r in rows]!=kinds:raise ValueError('wrong diagnostic mode schedule')
    s.validate_workload_sequence(rows,profile)
    for index,row in enumerate(rows):
        if row['revision']!=source['commit'] or row['case']!=case or row['index']!=index or type(row['index']) is not int or type(row['iterations']) is not int or row['iterations']!=2 or type(row['calibration_batch']) is not int or row['calibration_batch']!=(1 if mode=='calibrate' else 0) or row['isolated_case'] is not True:raise ValueError('wrong original dynamic worker')
        if set(e.indexed_rows(profile,row['rows'],source['commit'],{case:2},complete=False))!={case}:raise ValueError('dynamic original raw case differs')
        s.validate_fixture(row,profile,page);s.validate_timing(row,profile,2)
        if cpus is not None:s.validate_telemetry(row,cpus);s.validate_placement(row,cpus)
        elif any(row[k] is not None for k in ('before','after','placement')):raise ValueError('native worker lacks recorded affinity')
    if mode=='calibrate':s.calibration_batch(rows,2,1)
    e.observation_contracts(profile,rows)
    result=validate_ledger(ledger,profile,case,page,native=cpus is not None,legacy=source['commit']==e.BASELINE_COMMIT)
    for event in ledger['events']:
        if event['kind']=='view' and event['call'] and event['role'] in ('source','scratch') and event['stage']=='retained':
            row=rows[event['observation']];position=row['fixture'][event['role']][event['index']]
            if event['len']!=position['len'] or event['capacity']!=position['capacity'] or event['offsets']!=[position['offset_64'],position['offset_4096'],position['offset_page']]:raise ValueError('dynamic view differs from original retained caller fixture')
    if cpus is not None:
        for event in ledger['events']:
            if event['kind']=='view' and event['call']:
                row=rows[event['observation']]
                for i,field in enumerate(('process_minor_faults','process_major_faults')):
                    if not row['before'][field]<=event['faults'][i]<=row['after'][field]:raise ValueError('dynamic counters escape original observation')
    return result


def original_inputs(repository,source):
    return {name:subprocess.check_output(['git','show',source['commit']+':'+name],cwd=repository).decode() for name in NAMES}


def verify(output,repository):
    if any((output/name).exists() for name in ('failure.json','diagnostic-status.json')):raise ValueError('failed dynamic inventory is retained, not accepted')
    report=e.json_data(output/'result.json')
    if report.get('schema')!='dynamic-buffer-check-v1' or report.get('release_approval') is not False or report.get('candidate_attribution') is not None or report.get('formal_performance_geometry_equivalence') is not False:raise ValueError('invalid diagnostic-only dynamic inventory')
    if report.get('native_sha256')!=e.digest(native_template().encode()) or report.get('sampler_implementation_sha256')!=s.implementation_digest() or report.get('controller_sha256')!=e.digest(pathlib.Path(__file__).read_bytes()):raise ValueError('dynamic diagnostic implementation differs')
    validate_envelope(report,output,repository)
    page=report.get('page_size');cpus=report.get('cpu_affinity')
    if not s.valid_page_size(page) or cpus is not None and (not isinstance(cpus,list) or not cpus or any(not integer(c) for c in cpus) or cpus!=sorted(set(cpus))):raise ValueError('invalid dynamic page/CPU contract')
    sources=report.get('sources',{})
    if set(sources)!={'baseline','candidate'} or sources['baseline']['commit']!=e.BASELINE_COMMIT or sources['candidate']['commit']==e.BASELINE_COMMIT:raise ValueError('require both actual codec sources')
    expected={v['mode']+'/'+v['profile']+'/'+v['case'] for v in inventory_steps() if v['role']=='candidate'}
    contracts={};workloads={};traces={};members={};totals={'workers':0,'observations':0,'iterations':0,'events':0};differences={}
    for role,source in sources.items():
        if source!=e.source_at(repository,source['commit']) or report['source_overlays'][role]!=bindings(original_inputs(repository,source)):raise ValueError('original dynamic source/lock/overlay differs')
        if report['harness_sha256'][role]!=e.harness_digest(repository) or report['overlay_sha256'][role]!=overlay_digest(repository):raise ValueError('dynamic original worker/harness differs')
        cases=report['cases'][role]
        if set(cases)!=expected:raise ValueError('incomplete dynamic mode/profile/catalog')
        for key,entry in cases.items():
            mode,profile,case=key.split('/',2);name=role+'-'+mode+'-'+profile+'-'+case.replace(':','_')
            member=output/(name+'.json');journal=output/(name+'-journal.jsonl')
            if entry.get('path')!=member.name or entry.get('numeric_journal_path')!=journal.name:raise ValueError('invalid dynamic member path')
            for p,h in ((member,entry.get('sha256')),(journal,entry.get('numeric_journal_sha256'))):
                if p.is_symlink() or not p.is_file() or not 0<p.stat().st_size<=e.MAX_BYTES or e.digest(p.read_bytes())!=h:raise ValueError('dynamic member changed')
            private=output/(name+'.log')
            if not e.DIGEST.fullmatch(entry.get('raw_log_sha256','')) or private.exists() and (private.is_symlink() or e.digest(private.read_bytes())!=entry['raw_log_sha256']):raise ValueError('original private dynamic log changed')
            data=e.json_data(member);raw=journal.read_text();ledgers=s.parse_output(raw,PREFIX)
            if profile=='decoder':
                headers=s.parse_output(raw,'RSE_DYNAMIC_SUPPLEMENT ')
                if set(data)!={'supplement','dynamic'} or headers!=[data['supplement']] or ledgers!=[data['dynamic']]:raise ValueError('decoder supplement journal differs')
                summary=supplement_report(data['supplement'],data['dynamic'],case,page,cpus,source)
                rows=None
            else:
                rows=s.parse_output(raw);timing=s.parse_output(raw,'RSE_TIMING ')
            if profile!='decoder':
                if set(data)!={'samples','dynamic'} or rows!=data['samples'] or ledgers!=[data['dynamic']] or len(timing)!=12 or any(any(t[n]!=r[n] for n in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')) for t,r in zip(timing,rows)):raise ValueError('dynamic original journals differ')
                summary=worker_report(rows,data['dynamic'],profile,case,source,page,cpus,mode)
                contract_key=profile+'/'+case
                original=underlying_records(rows)
                contracts[contract_key]=e.observation_contracts(profile,original,contracts.get(contract_key))
                workload=s.workload_catalog(original);trace=[r['workload'] for r in original]
                if contract_key in workloads and (workloads[contract_key]!=workload or traces[contract_key]!=trace):raise ValueError('dynamic calibration/measurement input differs')
                workloads[contract_key]=workload;traces[contract_key]=trace
            if any(entry.get(k)!=v for k,v in summary.items()):raise ValueError('dynamic summary differs')
            differences[role+'/'+key]=sorted({(v['role'],v['stage'],v['index'],v['len'],v['capacity'],v['storage'],v['element_size'],tuple(v['shape']) if v['shape'] is not None else (),v['occupied'],v['present'],tuple(v['offsets']) if v['offsets'] is not None else ()) for v in data['dynamic']['events'] if v['kind']=='view'})
            members[member.name]=entry['sha256'];members[journal.name]=entry['numeric_journal_sha256']
            totals['workers']+=1;totals['observations']+=12;totals['iterations']+=summary['iterations'];totals['events']+=summary['events']
    mismatches=[]
    for key in sorted(expected):
        if differences['baseline/'+key]!=differences['candidate/'+key]:mismatches.append(key)
    return {'schema':'dynamic-buffer-replay-v1','release_approval':False,'candidate_attribution':None,
            'formal_performance_geometry_equivalence':False,'case_count':132,'original_case_count':126,'decoder_supplement_case_count':6,'source_count':2,'modes':list(MODES),**totals,
            'members':members,'diagnostic_geometry_differences':mismatches,
            'coverage':{'classic_decoder_matrix_storage':True,'decoder_heap_request_generations':True,'non_stream_dynamic_payloads':True,
                        'smoke_hotspot_source_positions':True,'calibration_and_measurement_lifetimes':True,
                        'leopard_decode_work_leases':True,'leopard_locator_conversion_output_scratch':True,'physical_allocation_identity':False,'first_fault_instruction':False,'formal_performance_acceptance':False}}


def overlay_digest(root):
    return e.digest(b''.join(n.encode()+b'\0'+t.replace(str(root),'<SOURCE>').encode() for n,t in sorted(render(root).items())))


def dynamic_context(root,env):
    import platform
    cpus=sorted(os.sched_getaffinity(0)) if hasattr(os,'sched_getaffinity') else None
    linux=platform.system()=='Linux'
    return {'compiler':e.command(['rustc','--version'],root,env),'machine':platform.machine(),
            'cpu_identity':e.cpu_identity(pathlib.Path('/proc/cpuinfo').read_text(),platform.machine(),cpus) if linux else platform.processor() or platform.machine(),
            'cpu_affinity':cpus,'page_size':s.base_page_size(),
            'kernel_memory_policy':native_tools.capture_memory_policy() if linux else None,
            'libc':list(platform.libc_ver()),'allocator_environment':{k:v for k,v in sorted(env.items()) if e.allocator_override_key(k)},
            'settings':{k:env[k] for k in e.SAMPLING_ENV}}


def check(repository,output,baseline_ref=None):
    if output.resolve().is_relative_to(repository.resolve()):raise ValueError('keep dynamic raw evidence outside the source repository')
    output.mkdir(parents=True,mode=0o700)
    try:
        tool=e.source_info(repository);env=e.capture_environment();context=dynamic_context(repository,env)
        revisions={'candidate':tool['commit'],'baseline':e.source_at(repository,baseline_ref or e.BASELINE_COMMIT)['commit']}
        if revisions['baseline']!=e.BASELINE_COMMIT or revisions['candidate']==e.BASELINE_COMMIT:raise ValueError('require published baseline and distinct candidate')
        report={'schema':'dynamic-buffer-check-v1','release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,
                'tool':tool,'sources':{},'cases':{},'source_overlays':{},'harness_sha256':{},'overlay_sha256':{},'context':context,
                'page_size':context['page_size'],'cpu_affinity':context['cpu_affinity'],
                'native_sha256':e.digest(native_template().encode()),'sampler_implementation_sha256':s.implementation_digest(),
                'controller_sha256':e.digest(pathlib.Path(__file__).read_bytes())}
        roots={}
        # Resolve every original source and reversible transform before running
        # either version; a failure cannot erase source/protocol provenance.
        for role,revision in revisions.items():
            root=output/role;s.checkout_baseline(repository,root,revision);roots[role]=root
            source=e.source_info(root);report['sources'][role]=source;report['cases'][role]={}
            report['source_overlays'][role]=bindings(original_inputs(repository,source))
            report['harness_sha256'][role]=e.harness_digest(root);report['overlay_sha256'][role]=overlay_digest(root)
        if len(set(report['harness_sha256'].values()))!=1 or len(set(report['overlay_sha256'].values()))!=1:raise ValueError('original dynamic harness differs across versions')
        steps=inventory_steps()
        (output/'plan.json').write_text(json.dumps(dict(report,steps=steps),indent=2)+'\n')
        with (output/'execution.jsonl').open('x') as execution:
            for role,root in roots.items():
                source=report['sources'][role]
                with source_overlay(root) as patches,overlay(root,render(root)):
                    if patches!=report['source_overlays'][role]:raise ValueError('dynamic source transform changed')
                    patched=patch_inputs(original_inputs(repository,source))
                    def unchanged():
                        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text()!=text for n,text in patched.items()):raise ValueError('dynamic source changed during observation')
                        if set(e.command(['git','diff','--name-only'],root).splitlines())!=set(NAMES):raise ValueError('foreign tracked diagnostic source change')
                        if e.source_info(repository)!=tool or dynamic_context(repository,env)!=context:raise ValueError('dynamic tool or runtime context changed')
                    unchanged()
                    with (output/(role+'-build.log')).open('x') as log:
                        subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=root,env=s.build_environment(root,env),stdout=log,stderr=subprocess.STDOUT,check=True)
                    with (output/(role+'-semantics.log')).open('x') as log:
                        controls=dict(s.build_environment(root,env),RSE_DYNAMIC_CURRENT_SOURCE='0' if role=='baseline' else '1')
                        subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','dynamic_semantics::active_observer_preserves_normal_selective_invalid_and_late_failure_semantics','--','--exact','--nocapture','--test-threads=1'],cwd=root,env=controls,stdout=log,stderr=subprocess.STDOUT,check=True)
                    for step in (v for v in steps if v['role']==role):
                        mode,profile,case=(step[k] for k in ('mode','profile','case'));unchanged()
                        execution.write(json.dumps({'stage':'begin','step':step,'context':context})+'\n');execution.flush()
                        settings=dict(s.worker_environment(env),RSE_SAMPLER_PROFILE=profile,RSE_SAMPLER_MODE=mode,RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='2',RSE_SAMPLER_BATCH='1' if mode=='calibrate' else '0',RSE_SAMPLER_PAGE_SIZE=str(report['page_size']))
                        if profile=='decoder':settings['RSE_DYNAMIC_DECODER_CASE']=case
                        name=role+'-'+mode+'-'+profile+'-'+case.replace(':','_');log=output/(name+'.log');numeric=output/(name+'-journal.jsonl')
                        try:
                            with log.open('x') as stream:
                                proc=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_dynamic_decoder_profile' if profile=='decoder' else 'release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=s.build_environment(root,settings),stdout=stream,stderr=subprocess.STDOUT)
                        finally:
                            if log.exists():numeric.write_text(''.join(line for line in log.read_text().splitlines(keepends=True) if line.startswith(('RSE_TIMING ','RSE_SAMPLE ','RSE_DYNAMIC_SUPPLEMENT ',PREFIX))))
                        proc.check_returncode();raw=numeric.read_text();rows=s.parse_output(raw);ledgers=s.parse_output(raw,PREFIX)
                        if len(ledgers)!=1:
                            (output/(name+'.partial.json')).write_text(json.dumps({'samples':rows,'dynamic':ledgers},indent=2)+'\n')
                            raise ValueError('missing complete dynamic allocation ledger')
                        member=output/(name+'.json')
                        if profile=='decoder':
                            headers=s.parse_output(raw,'RSE_DYNAMIC_SUPPLEMENT ')
                            if len(headers)!=1:raise ValueError('missing fixed decoder supplement header')
                            member.write_text(json.dumps({'supplement':headers[0],'dynamic':ledgers[0]},indent=2)+'\n')
                            summary=supplement_report(headers[0],ledgers[0],case,report['page_size'],report['cpu_affinity'],source)
                        else:
                            member.write_text(json.dumps({'samples':rows,'dynamic':ledgers[0]},indent=2)+'\n')
                            summary=worker_report(rows,ledgers[0],profile,case,source,report['page_size'],report['cpu_affinity'],mode)
                        unchanged()
                        entry=dict(summary,path=member.name,sha256=e.digest(member.read_bytes()),numeric_journal_path=numeric.name,numeric_journal_sha256=e.digest(numeric.read_bytes()),raw_log_sha256=e.digest(log.read_bytes()))
                        report['cases'][role][mode+'/'+profile+'/'+case]=entry
                        execution.write(json.dumps({'stage':'end','step':step,'context':context,'entry':entry})+'\n');execution.flush()
                if e.source_info(root)!=source:raise ValueError('original dynamic source was not restored')
        if e.source_info(repository)!=tool or dynamic_context(repository,env)!=context:raise ValueError('dynamic source/context changed before replay')
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
        result=verify(output,repository);(output/'replay.json').write_text(json.dumps(result,indent=2)+'\n');return result
    except BaseException as error:
        (output/'diagnostic-status.json').write_text(json.dumps({'status':'failed','release_approval':False,'original_records_preserved':True})+'\n')
        (output/'failure.json').write_text(json.dumps({'error':str(error),'release_approval':False})+'\n')
        raise
