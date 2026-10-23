"""Unified remaining byte-storage/allocator inventory; invoked by fixture inventory.

All modifications are reversible source-copy observers. No production allocator,
sampler, algorithm, original allocation statement or release gate is changed.
"""
import json
import os
import pathlib
import native_tools
import subprocess

import release_evidence as e
import release_sampling as s
from diagnostic_overlay import overlay, source_overlay as owned_source_overlay

MODULE = '__rse_dynamic_buffers'
PREFIX = 'RSE_DYNAMIC_BUFFERS '
_catalog = native_tools.request('diagnostic-ledger', request=dict(kind='dynamic-catalog'))
PROFILES = tuple(_catalog['profiles'])
MODES = tuple(_catalog['modes'])
SUPPLEMENTS = _catalog['supplements']
_STEPS = json.dumps(_catalog.pop('steps'),separators=(',',':'))
NAMES = ('src/lib.rs', 'src/matrix.rs', 'src/core/reconstruct.rs', 'src/galois_8/policy.rs') + native_tools.LEOPARD_FILES


def native_template():
    return native_tools.native_template('dynamic')


def patch_inputs(original):
    return native_tools.source_plan('dynamic', original, output='files')


def bindings(original):
    return native_tools.source_plan('dynamic', original, output='bindings')


def source_overlay(root):
    return owned_source_overlay(root, NAMES, MODULE, 'dynamic')


def render(root, output='files'):
    return native_tools.diagnostic_render(root, 'dynamic', output=output)
# Shared strict capture/replay contract.


def inventory_steps():
    return json.loads(_STEPS)


def supplement_report(header,ledger,case,page,cpus,source,*,journal=None):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='dynamic-supplement', header=header, data=ledger, case=case, page=page, journal=journal,
        cpus=cpus, legacy=source['commit']==e.BASELINE_COMMIT))


def validate_ledger(data,profile,case,page,native=False,legacy=False):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='dynamic', data=data, profile=profile, case=case, page=page, native=native, legacy=legacy))


def validate_envelope(report,output,repository):
    sources=native_tools.request('diagnostic-ledger',request=dict(kind='dynamic-envelope',request=dict(
        report=report,plan=e.json_data(output/'plan.json'),execution=e.member_text(output/'execution.jsonl'))))
    for source in sources.values():
        if source!=e.source_at(repository,source['commit']):raise ValueError('original dynamic source/tool differs')


def worker_report(rows,ledger,profile,case,source,page,cpus,mode,*,journal=None,reference=None):
    return native_tools.request('diagnostic-worker', request=dict(rows=rows, profile=profile, case=case,
        revision=source['commit'], page=page, cpus=None if cpus is None else list(cpus), mode=mode, journal=journal, reference=reference,
        data=dict(kind='dynamic', ledger=ledger, legacy=source['commit']==e.BASELINE_COMMIT)))


def original_inputs(repository,source):
    return {name:subprocess.check_output(['git','show',source['commit']+':'+name],cwd=repository).decode() for name in NAMES}


def verify(output,repository):
    if any((output/name).exists() for name in ('failure.json','diagnostic-status.json')):raise ValueError('failed dynamic inventory is retained, not accepted')
    report=e.json_data(output/'result.json')
    if report.get('schema')!='dynamic-buffer-check-v1' or report.get('release_approval') is not False or report.get('candidate_attribution') is not None or report.get('formal_performance_geometry_equivalence') is not False:raise ValueError('invalid diagnostic-only dynamic inventory')
    if report.get('native_sha256')!=e.digest(native_template().encode()) or report.get('sampler_implementation_sha256')!=s.implementation_digest() or report.get('controller_sha256')!=e.digest(pathlib.Path(__file__).read_bytes()):raise ValueError('dynamic diagnostic implementation differs')
    validate_envelope(report,output,repository)
    page=report.get('page_size');cpus=report.get('cpu_affinity')
    sources=report['sources']
    if sources['baseline']['commit']!=e.BASELINE_COMMIT or sources['candidate']['commit']==e.BASELINE_COMMIT:raise ValueError('require both actual codec sources')
    expected={v['mode']+'/'+v['profile']+'/'+v['case'] for v in inventory_steps() if v['role']=='candidate'}
    contracts={};workloads={};traces={};members={};totals={'workers':0,'observations':0,'iterations':0,'events':0};differences={}
    for role,source in sources.items():
        if report['source_overlays'][role]!=bindings(original_inputs(repository,source)):raise ValueError('original dynamic source overlay differs')
        if report['harness_sha256'][role]!=e.harness_digest(repository) or report['overlay_sha256'][role]!=overlay_digest(repository):raise ValueError('dynamic original worker/harness differs')
        cases=report['cases'][role]
        if set(cases)!=expected:raise ValueError('incomplete dynamic mode/profile/catalog')
        for key,entry in cases.items():
            mode,profile,case=key.split('/',2);name=role+'-'+mode+'-'+profile+'-'+case.replace(':','_')
            member=output/(name+'.json');journal=output/(name+'-journal.jsonl')
            if entry.get('path')!=member.name or entry.get('numeric_journal_path')!=journal.name:raise ValueError('invalid dynamic member path')
            data=e.json_data(member,entry.get('sha256'));bound=dict(path=str(journal.resolve()),sha256=entry.get('numeric_journal_sha256'))
            private=output/(name+'.log')
            if not e.DIGEST.fullmatch(entry.get('raw_log_sha256','')) or private.exists() and (private.is_symlink() or e.digest(private.read_bytes())!=entry['raw_log_sha256']):raise ValueError('original private dynamic log changed')
            if profile=='decoder':
                if set(data)!={'supplement','dynamic'}:raise ValueError('invalid decoder supplement member')
                summary=supplement_report(data['supplement'],data['dynamic'],case,page,cpus,source,journal=bound)
            else:
                if set(data)!={'samples','dynamic'}:raise ValueError('invalid dynamic worker member')
                contract_key=profile+'/'+case
                analysis=worker_report(data['samples'],data['dynamic'],profile,case,source,page,cpus,mode,
                    journal=bound,reference=contracts.get(contract_key))
                summary=analysis['summary'];contracts[contract_key]=analysis['contract']
                workload=analysis['preparation']['workloads'];trace=analysis['workload_trace']
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
    return render(root,output='digest')


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
                            summary=worker_report(rows,ledgers[0],profile,case,source,report['page_size'],report['cpu_affinity'],mode)['summary']
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
