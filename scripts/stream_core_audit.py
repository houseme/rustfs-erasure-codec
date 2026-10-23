"""Replayable stream owner lifecycles in disposable codec source copies.

These deliberately instrumented runs are never formal performance evidence.
The original per-call allocation/free work, loops and API results are retained.
"""
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
from diagnostic_overlay import overlay, source_overlay as owned_source_overlay
from native_tools import STREAM_CORE_MODULE as MODULE

PREFIX='RSE_STREAM_CORE '
NAMES=('src/lib.rs','src/core/stream.rs','src/core/verify.rs')


def native_template():
    return native_tools.native_template('stream')


def render(root):
    return native_tools.diagnostic_render(root, 'stream')


def bindings(original):
    return native_tools.source_plan('stream-core', original, output='bindings')


def source_overlay(root):
    return owned_source_overlay(root, NAMES, MODULE, 'stream-core')


def validate(data,case,iterations,page,current,native=False):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='stream-core', data=data, case=case, iterations=iterations, page=page,
        current=current, native=native))


def worker_report(rows,io,core,case,source,page,cpus,current,*,journal=None,reference=None):
    return native_tools.request('diagnostic-worker', request=dict(rows=rows, profile='stream', case=case,
        revision=source['commit'], page=page, cpus=None if cpus is None else list(cpus), journal=journal, reference=reference,
        data=dict(kind='stream-core', ledger=core, views=io, current=current)))


def _original(repository,source):
    return {n:subprocess.check_output(['git','show',source['commit']+':'+n],cwd=repository).decode() for n in NAMES}


def worker_data(output,name,entry):
    member=output/(name+'.json');numeric=output/(name+'-journal.jsonl')
    if entry.get('path')!=member.name or entry.get('numeric_journal_path')!=numeric.name:raise ValueError('invalid stream member path')
    data=e.json_data(member,entry.get('sha256'))
    private=output/(name+'.log')
    if not e.DIGEST.fullmatch(entry.get('raw_log_sha256','')) or private.exists() and (private.is_symlink() or e.digest(private.read_bytes())!=entry['raw_log_sha256']):raise ValueError('private original stream log differs')
    if set(data)!={'samples','views','core'}:raise ValueError('invalid stream numerical member')
    return data,dict(path=str(numeric.resolve()),sha256=entry.get('numeric_journal_sha256'))



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
            data,journal=worker_data(output,name,entry)
            analysis=worker_report(data['samples'],data['views'],data['core'],case,source,page,cpus,current,journal=journal,reference=contracts.get(case))
            summary=analysis['summary']
            if any(entry.get(n)!=v for n,v in summary.items()):raise ValueError('stream source summary differs')
            contracts[case]=analysis['contract'];workload=analysis['preparation']['workloads']
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
                summary=worker_report(rows,io,core[0],case,source,report['page_size'],report['cpu_affinity'],current)['summary']
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
