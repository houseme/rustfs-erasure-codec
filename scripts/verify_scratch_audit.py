"""Observe real ordinary-verify scratch in reversible diagnostic source copies.

Never a formal performance capture. Original API loops and all non-observer
statements are retained. No production checkout is patched by this command.
"""
import argparse
import json
import os
import pathlib
import native_tools
import subprocess
import tempfile

import release_evidence as evidence
import release_sampling as sampling
from diagnostic_overlay import overlay, source_overlay as owned_source_overlay

PREFIX = 'RSE_VERIFY_SCRATCH '
MODULE = '__rse_verify_scratch'


def native_template():
    return native_tools.native_template('verify')


def render(root):
    return native_tools.diagnostic_render(root, 'verify')


def bindings(original):
    return native_tools.source_plan('verify-scratch', original, output='bindings')


def overlay_digest(root,files):
    return evidence.digest(b''.join(name.encode()+b'\0'+text.replace(str(root),'<SOURCE>').encode() for name,text in sorted(files.items())))


def source_overlay(root):
    return owned_source_overlay(root, ('src/lib.rs','src/core/verify.rs'), MODULE, 'verify-scratch')


def cases(profile):return {c for c in evidence.expected_cases(profile) if c.startswith('verify:')}


def validate(data,case,iterations,page,native=False):
    return native_tools.request('diagnostic-ledger', request=dict(
        kind='verify-scratch', data=data, case=case, iterations=iterations, page=page, native=native))


def worker_report(rows,observed,profile,case,source,page,cpus):
    return native_tools.request('diagnostic-worker', request=dict(rows=rows, profile=profile, case=case,
        revision=source['commit'], page=page, cpus=None if cpus is None else list(cpus),
        data=dict(kind='verify-scratch', ledger=observed)))


def verify(output,repository):
    if (output/'failure.json').exists() or (output/'diagnostic-status.json').exists():raise ValueError('failed diagnostic is preserved, not accepted')
    report=evidence.json_data(output/'result.json')
    if report.get('schema')!='verify-scratch-check-v1' or report.get('release_approval') is not False or report.get('candidate_attribution') is not None or report.get('formal_performance_geometry_equivalence') is not False or report.get('source_copies') is not True:
        raise ValueError('invalid diagnostic-only report')
    if report.get('native_sha256')!=evidence.digest(native_template().encode()) or report.get('sampler_implementation_sha256')!=sampling.implementation_digest():raise ValueError('diagnostic implementation differs')
    if not evidence.DIGEST.fullmatch(report.get('controller_sha256','')):raise ValueError('missing recorded controller identity')
    expected={p+'/'+c for p in ('smoke','small_file') for c in cases(p)}
    page=report.get('page_size');cpus=report.get('cpu_affinity')
    if not sampling.valid_page_size(page):raise ValueError('invalid diagnostic page size')
    if cpus is not None and (not isinstance(cpus,list) or not cpus or any(type(c) is not int or c<0 for c in cpus) or cpus!=sorted(set(cpus))):raise ValueError('invalid recorded native affinity')
    if report.get('harness_sha256')!={r:evidence.harness_digest(repository) for r in ('baseline','candidate')} or report.get('overlay_sha256')!={r:overlay_digest(repository,render(repository)) for r in ('baseline','candidate')}:raise ValueError('original harness or diagnostic worker differs')
    contracts={};workloads={};signatures={};members={}
    if set(report.get('sources',{}))!={'baseline','candidate'} or set(report.get('cases',{}))!={'baseline','candidate'}:raise ValueError('both diagnostic sources required')
    if report['sources']['baseline']['commit']!=evidence.BASELINE_COMMIT:raise ValueError('wrong published baseline')
    if report['sources']['candidate']['commit']==evidence.BASELINE_COMMIT:raise ValueError('require a distinct actual candidate source')
    for role,source in report['sources'].items():
        if not isinstance(source,dict) or set(source)!={'commit','tree','lock_sha256'} or not isinstance(source['commit'],str) or len(source['commit'])!=40 or any(c not in '0123456789abcdef' for c in source['commit']):raise ValueError('invalid original source identity')
        if evidence.source_at(repository,source['commit'])!=source:raise ValueError('original diagnostic source identity differs')
        original={name:subprocess.check_output(['git','show',source['commit']+':'+name],cwd=repository).decode() for name in ('src/lib.rs','src/core/verify.rs')}
        if report.get('source_overlays',{}).get(role)!=bindings(original):raise ValueError('diagnostic source overlay differs')
        if set(report['cases'][role])!=expected:raise ValueError('incomplete original verify catalog')
        signatures[role]={}
        for key,entry in report['cases'][role].items():
            profile,case=key.split('/',1);name=role+'-'+profile+'-'+case.replace(':','_')
            if entry.get('path')!=name+'.json':raise ValueError('invalid diagnostic member path')
            member=output/entry['path'];data=evidence.json_data(member,entry.get('sha256'))
            if entry.get('numeric_journal_path')!=name+'-journal.jsonl':raise ValueError('invalid numeric journal path')
            numeric=output/entry['numeric_journal_path']
            raw=evidence.member_text(numeric,entry.get('numeric_journal_sha256'))
            log=output/(name+'.log')
            if not evidence.DIGEST.fullmatch(entry.get('raw_log_sha256','')):raise ValueError('missing original private log binding')
            if log.exists() and (log.is_symlink() or evidence.digest(log.read_bytes())!=entry['raw_log_sha256']):raise ValueError('original private log changed')
            rows=sampling.parse_output(raw);scratch=sampling.parse_output(raw,PREFIX);journal=sampling.parse_output(raw,'RSE_TIMING ')
            if rows!=data['samples'] or scratch!=[data['scratch']] or len(journal)!=12 or any(any(j[field]!=row[field] for field in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')) for j,row in zip(journal,rows)):
                raise ValueError('diagnostic raw rows differ from original journals')
            summary=worker_report(rows,data['scratch'],profile,case,source,page,cpus)
            if any(entry.get(k)!=v for k,v in summary.items()):raise ValueError('diagnostic summary differs from raw rows')
            contracts[key]=evidence.observation_contracts(profile,rows,contracts.get(key))
            workload=sampling.workload_catalog(rows)
            if key in workloads and workload!=workloads[key]:raise ValueError('diagnostic preparation differs across sources')
            workloads[key]=workload
            signatures[role][key]={(e['phase'],e['len'],e['capacity'],tuple(e['offsets']),e['inline']) for e in data['scratch']['events']}
            members[entry['path']]=entry['sha256']
    return {'schema':'verify-scratch-replay-v1','release_approval':False,'candidate_attribution':None,
            'formal_performance_geometry_equivalence':False,'source_count':2,'case_count':len(expected),
            'members':members,'auditor_sha256':evidence.digest(pathlib.Path(__file__).read_bytes()),
            'layout_differences':[{'case':key,'different_shapes':len(signatures['baseline'][key]^signatures['candidate'][key])} for key in sorted(expected) if signatures['baseline'][key]!=signatures['candidate'][key]]}


def _check(repository,output,baseline_ref):
    roles={r:evidence.source_at(repository,rev)['commit'] for r,rev in {'candidate':'HEAD','baseline':baseline_ref}.items()}
    if roles['baseline']!=evidence.BASELINE_COMMIT or roles['candidate']==roles['baseline']:raise ValueError('require published 9.0.0 and a distinct candidate before any build')
    report={'schema':'verify-scratch-check-v1','release_approval':False,'candidate_attribution':None,
            'formal_performance_geometry_equivalence':False,'source_copies':True,'sources':{},'cases':{},'sampler_implementation_sha256':sampling.implementation_digest(),
            'controller_sha256':evidence.digest(pathlib.Path(__file__).read_bytes()),
            'native_sha256':evidence.digest(native_template().encode())}
    env=evidence.capture_environment();env['RUSTUP_TOOLCHAIN']='stable'
    page=sampling.base_page_size();report['page_size']=page
    report['cpu_affinity']=sorted(os.sched_getaffinity(0)) if hasattr(os,'sched_getaffinity') else None
    for role,revision in roles.items():
        root=output/role;sampling.checkout_baseline(repository,root,revision)
        source=evidence.source_info(root);report['sources'][role]=source;report['cases'][role]={}
        if role=='baseline' and source['commit']!=evidence.BASELINE_COMMIT:raise ValueError('require actual published 9.0.0')
        files=render(root)
        report.setdefault('harness_sha256',{})[role]=evidence.harness_digest(root)
        report.setdefault('overlay_sha256',{})[role]=overlay_digest(root,files)
        with source_overlay(root) as patches,overlay(root,files):
            report.setdefault('source_overlays',{})[role]=patches
            with (output/(role+'-build.log')).open('x') as log:
                subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=root,env=sampling.build_environment(root,env),stdout=log,stderr=subprocess.STDOUT,check=True)
            for profile in ('smoke','small_file'):
                for case in sorted(cases(profile)):
                    settings=dict(sampling.worker_environment(env),RSE_SAMPLER_PROFILE=profile,RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='2',RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(page))
                    name=role+'-'+profile+'-'+case.replace(':','_')
                    path=output/(name+'.log')
                    numeric=output/(name+'-journal.jsonl')
                    try:
                        with path.open('x') as log:
                            process=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=sampling.build_environment(root,settings),stdout=log,stderr=subprocess.STDOUT)
                    finally:
                        if path.exists():
                            numeric.write_text(''.join(line for line in path.read_text().splitlines(keepends=True) if line.startswith(('RSE_TIMING ','RSE_SAMPLE ',PREFIX))))
                    process.check_returncode()
                    text=numeric.read_text();rows=sampling.parse_output(text);observed=sampling.parse_output(text,PREFIX)
                    if len(observed)!=1:raise ValueError('missing complete native scratch journal')
                    summary=worker_report(rows,observed[0],profile,case,source,page,report['cpu_affinity'])
                    member=output/(name+'.json')
                    member.write_text(json.dumps({'samples':rows,'scratch':observed[0]},indent=2)+'\n')
                    report['cases'][role][profile+'/'+case]=dict(summary,path=member.name,sha256=evidence.digest(member.read_bytes()),raw_log_sha256=evidence.digest(path.read_bytes()),numeric_journal_path=numeric.name,numeric_journal_sha256=evidence.digest(numeric.read_bytes()))
        if evidence.source_info(root)!=source:raise ValueError('original source/lock not restored')
    if report['cases']['candidate'].keys()!=report['cases']['baseline'].keys():raise ValueError('incomplete two-source catalog')
    if len(set(report['harness_sha256'].values()))!=1 or len(set(report['overlay_sha256'].values()))!=1:raise ValueError('source copies require identical diagnostic workers')
    (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    replay=verify(output,repository)
    (output/'replay.json').write_text(json.dumps(replay,indent=2)+'\n')
    return report


def check(repository,output,baseline_ref):
    # Atomic creation defines ownership. Existing captures are never modified.
    output.mkdir(parents=True)
    try:
        return _check(repository,output,baseline_ref)
    except BaseException as error:
        (output/'diagnostic-status.json').write_text(json.dumps({'status':'diagnostic_failed','release_approval':False,'candidate_attribution':None,'original_records_preserved':True})+'\n')
        with (output/'failure.json').open('x') as target:
            target.write(json.dumps({'error':str(error),'release_approval':False},indent=2)+'\n')
        raise


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    modes=parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--check',action='store_true')
    modes.add_argument('--replay',type=pathlib.Path)
    parser.add_argument('--baseline-ref',default=evidence.BASELINE_COMMIT)
    parser.add_argument('--output',type=pathlib.Path)
    args=parser.parse_args()
    if args.replay:
        if args.output is not None:parser.error('--output is only for a new check')
        print(json.dumps(verify(args.replay.resolve(),pathlib.Path(__file__).resolve().parents[1]),indent=2));return
    destination=args.output or pathlib.Path(tempfile.mkdtemp(prefix='rse-verify-scratch-'))/'check'
    report=check(pathlib.Path(__file__).resolve().parents[1],destination.resolve(),args.baseline_ref)
    print(json.dumps({'case_count':len(report['cases']['candidate']),'source_count':2,'release_approval':False,'output':str(destination)}))

if __name__=='__main__':main()
