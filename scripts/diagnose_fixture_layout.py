#!/usr/bin/env python3
"""Inventory retained Vec layouts across versions; never measure release performance."""
from contextlib import ExitStack
import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys

import release_evidence as evidence
import release_sampling as sampling
from diagnostic_overlay import overlay as generated_overlay
import stream_view_audit as view_audit


PROFILES = ('small_file', 'stream')


def case_catalog(profile=None):
    return evidence.expected_cases(profile) if profile else set().union(*(evidence.expected_cases(p) for p in PROFILES))


def comparable_inventory_contract(groups, profile, reference=None):
    """Reuse formal provenance and workload contracts for every inventory mode."""
    records=[row for group in groups.values() for row in group]
    contracts=evidence.observation_contracts(profile,records,reference[0] if reference is not None else None)
    workload=sampling.workload_catalog(records)
    if reference is not None and workload!=reference[1]:raise ValueError('inventory preparation/placement differs between phases')
    return contracts,workload


def analyze(phases, sources, cpus, page_size=None):
    if set(phases) != set(evidence.PHASES):
        raise ValueError('all four layout inventory phases are required')
    counts = {case: 1 for case in case_catalog()}
    layouts = {};contracts={}
    for phase in evidence.PHASES:
        revision = sources['baseline' if phase.startswith('A') else 'candidate']['commit']
        normalize = lambda p, row, rev: next(iter(evidence.indexed_rows(p, [row], rev, counts, complete=False)))
        if any(r.get('case') not in case_catalog() for r in phases[phase]):
            raise ValueError('unexpected inventory case')
        for profile in PROFILES:
            records = [r for r in phases[phase] if r['case'] in case_catalog(profile)]
            groups=sampling.sample_groups(records, profile, revision, case_catalog(profile), counts, normalize, cpus, page_size)
            contracts[profile]=comparable_inventory_contract(groups,profile,contracts.get(profile))
        layouts[phase] = sampling.fixture_catalog(phases[phase])
    checks = []
    for left, right in (('A1', 'A2'), ('B1', 'B2'), ('A1', 'B1'), ('A1', 'B2'), ('A2', 'B1'), ('A2', 'B2')):
        for case in sorted(case_catalog()):
            before, after = layouts[left][case], layouts[right][case]
            changed = []
            for role in ('source', 'scratch'):
                for index, (a, b) in enumerate(zip(before[role], after[role])):
                    fields = sorted(key for key in a if a[key] != b[key])
                    if fields: changed.append({'role': role, 'index': index, 'fields': fields})
            checks.append({'case': case, 'pair': left+'/'+right, 'matches': before == after,
                           'different_buffers': changed})
    return {'purpose': 'one-iteration structural layout inventory; no timing inference',
            'release_approval': False, 'candidate_attribution': None,
            'case_count': len(counts), 'phase_count': len(phases), 'dynamic_output_layouts_observed': False,
            'layout_checks': checks, 'layout_mismatches': [c for c in checks if not c['matches']]}


def analyze_stream_views(phases, views, sources, cpus, page_size, iterations):
    counts={case:iterations for case in case_catalog('stream')}
    if set(phases)!=set(evidence.PHASES) or set(views)!=set(phases):raise ValueError('all four stream-view inventory phases are required')
    signatures={};reference=None
    for phase in evidence.PHASES:
        revision=sources['baseline' if phase.startswith('A') else 'candidate']['commit']
        normalized=view_audit.underlying_records(phases[phase])
        normalize=lambda p,row,rev:next(iter(evidence.indexed_rows(p,[row],rev,counts,complete=False)))
        groups=sampling.sample_groups(normalized,'stream',revision,case_catalog('stream'),counts,normalize,cpus,page_size)
        reference=comparable_inventory_contract(groups,'stream',reference)
        if len(views[phase])!=len(phases[phase]):raise ValueError('missing stream-view observations')
        signatures[phase]={}
        for sample,view in zip(phases[phase],views[phase]):
            case=sample['case']
            view_audit.validate(view,case,iterations,page_size,sample['index'],sample['fixture']['source'],require_counters=True)
            keys=signatures[phase].setdefault(case,set())
            keys.update((e['role'],e['index'],e['len'],e['capacity'],tuple(e['offsets']) if e['offsets'] is not None else None) for e in view['events'])
    checks=[]
    for left,right in (('A1','A2'),('B1','B2'),('A1','B1'),('A1','B2'),('A2','B1'),('A2','B2')):
        for case in sorted(counts):
            a,b=signatures[left][case],signatures[right][case]
            checks.append({'case':case,'pair':left+'/'+right,'matches':a==b,'different_view_shapes':len(a^b)})
    return {'purpose':'instrumented real I/O view/lifecycle inventory; no timing inference',
            'release_approval':False,'candidate_attribution':None,'case_count':len(counts),
            'phase_count':4,'observations':sum(map(len,views.values())),
            'coverage':{'stream_case_lifecycles':15,'caller_owned_destination_cases':10,'internal_read_views':10,'internal_parity_write_views':5,
                        'native_virtual_page_snapshots':True,'former_encode_scratch_ranges':True,'former_writer_ranges':True,
                        'physical_page_identity':False,'first_fault_instruction':False,'internal_verify_scratch':False,'internal_recovery_scratch':False,'non_stream_dynamic_buffers':False,
                        'formal_performance_geometry_equivalence':False},
            'lifecycle_windows':{phase:[{'case':sample['case'],'observation':view['observation'],'calls':view_audit.summarize_windows(view)} for sample,view in zip(phases[phase],views[phase])] for phase in evidence.PHASES},
            'layout_checks':checks,'layout_mismatches':[c for c in checks if not c['matches']]}


def capture(tool, baseline, candidate, output, group, stream_views=False, iterations=2):
    if pathlib.Path(sampling.__file__).resolve().parent != (tool/'scripts').resolve():
        raise ValueError('tool path does not identify the imported sampler')
    if stream_views and (type(iterations) is not int or not 1<=iterations<=16):raise ValueError('bounded audit iterations must be 1..16')
    if output.exists(): raise ValueError('use a new inventory directory')
    output.mkdir(parents=True, mode=0o700)
    env=evidence.capture_environment()
    profiles=('stream',) if stream_views else PROFILES
    count=iterations if stream_views else 1
    renderer=view_audit.render if stream_views else sampling.render
    roots={'baseline':baseline,'candidate':candidate}
    sources={role:evidence.source_info(root) for role,root in roots.items()}
    if sources['baseline']['commit']!=evidence.BASELINE_COMMIT or sources['candidate']['commit']==sources['baseline']['commit']:
        raise ValueError('published 9.0.0 and a different candidate source are required')
    context=evidence.environment_context(candidate,env,group)
    def overlay_digest(root):
        return hashlib.sha256(b''.join(name.encode()+b'\0'+text.replace(str(root),'<SOURCE>').encode()
            for name,text in sorted(renderer(root).items()))).hexdigest()
    overlay=overlay_digest(candidate)
    if overlay_digest(baseline)!=overlay or evidence.harness_digest(baseline)!=evidence.harness_digest(candidate):
        raise ValueError('both sources require identical harness inputs')
    metadata={'schema_version':'stream-view-inventory-v2' if stream_views else 'fixture-layout-inventory-v2',
              'sources':sources,'tool':evidence.source_info(tool),'context':context,
              'measuring_affinity':[min(context['isolation']['cpu_affinity'])],
              'constructor_affinity':context['isolation']['cpu_affinity'],'protocol':sampling.PROTOCOL,
              'overlay_sha256':overlay,'renderer_sha256':sampling.implementation_digest(),
              'view_audit_sha256':view_audit.implementation_digest() if stream_views else None,
              'controller_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
              'iterations':count,'purpose':'structural inventory; no timing inference',
              'candidate_attribution':None,'release_approval':False}
    def save(name,value):(output/name).write_text(json.dumps(value,indent=2)+'\n')
    def unchanged():
        if evidence.environment_context(candidate,env,group)!=context or evidence.source_info(tool)!=metadata['tool'] or any(evidence.source_info(root)!=sources[role] for role,root in roots.items()):
            raise ValueError('source or resource context changed')
    save('provenance.json',metadata)
    counts={}
    for profile in profiles:
        counts[profile]=output/('counts-'+profile+'.tsv')
        counts[profile].write_text(''.join(f'{case}\t{count}\n' for case in sorted(case_catalog(profile))))
    phases,views={},{}
    with ExitStack() as owned:
        for role,root in roots.items():
            owned.enter_context(generated_overlay(root,renderer(root)))
            with (output/(role+'-build.log')).open('x') as log:
                subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel',
                    '--test','rse_release_sampling','--no-run'],cwd=root,env=sampling.build_environment(root,env),
                    stdout=log,stderr=subprocess.STDOUT,check=True)
        for phase in evidence.PHASES:
            role='baseline' if phase.startswith('A') else 'candidate'
            phases[phase],views[phase],timings=[],[],[]
            for profile in profiles:
                log=output/(phase+'-'+profile+'.log')
                try:
                    current=sampling.run_profile(roots[role],env,profile,'sample',log,counts[profile],cases=case_catalog(profile))
                finally:
                    if log.exists():
                        worker_failed=sys.exc_info()[0] is not None
                        text=log.read_text();parsed={};parse_errors=[]
                        for kind,prefix in (('samples','RSE_SAMPLE '),('timing','RSE_TIMING '),*([('views',view_audit.PREFIX)] if stream_views else [])):
                            try:
                                parsed[kind]=sampling.parse_output(text,prefix)
                                save(phase+'-'+profile+'-'+kind+'.partial.json',parsed[kind])
                            except (ValueError,OSError) as error:
                                parse_errors.append(kind)
                                save(phase+'-'+profile+'-'+kind+'.error.json',{'status':'failed','error':str(error)})
                        partial=parsed.get('samples',[]);journal=parsed.get('timing',[]);observed=parsed.get('views',[])
                        save(phase+'-samples.partial.json',phases[phase]+partial)
                        save(phase+'-timing.partial.json',timings+journal)
                        if stream_views:save(phase+'-views.partial.json',views[phase]+observed)
                        if parse_errors and not worker_failed:raise ValueError('malformed inventory journals: '+','.join(parse_errors))
                phases[phase].extend(current);timings.extend(journal);views[phase].extend(observed)
            save(phase+'-samples.json',phases[phase])
            if stream_views:save(phase+'-views.json',views[phase])
            unchanged()
        report=analyze_stream_views(phases,views,sources,context['isolation']['cpu_affinity'],context['runtime']['page_size'],count) if stream_views else analyze(phases,sources,context['isolation']['cpu_affinity'],context['runtime']['page_size'])
    # A successful inventory exists only after every owned overlay closed safely.
    unchanged();save('inventory.json',dict(metadata,**report))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tool', type=pathlib.Path)
    parser.add_argument('--baseline', type=pathlib.Path)
    parser.add_argument('--candidate', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path)
    parser.add_argument('--isolation-cgroup', type=pathlib.Path)
    parser.add_argument('--stream-views', action='store_true', help='audit all stream I/O views and cursor lifecycles; never performance evidence')
    parser.add_argument('--iterations', type=int, default=2, help='bounded calls per instrumented observation (1..16)')
    parser.add_argument('--check-dynamic', action='store_true', help='check remaining buffer lifecycles in source copies; never performance evidence')
    parser.add_argument('--replay-dynamic', type=pathlib.Path, help='strict offline replay of a complete dynamic source-copy inventory')
    parser.add_argument('--check-stream-views', action='store_true', help='compile and execute all stream view workers; correctness only')
    parser.add_argument('--baseline-ref', help='also check the exact published source with its own lockfile')
    args = parser.parse_args()
    if args.check_dynamic or args.replay_dynamic is not None:
        if args.iterations!=2 or args.check_dynamic and args.replay_dynamic is not None or args.stream_views or args.check_stream_views or any(getattr(args,name) is not None for name in ('tool','baseline','candidate','isolation_cgroup')):
            parser.error('dynamic source-copy inventory is separate from formal capture')
        import dynamic_buffer_audit as dynamic
        root=pathlib.Path(__file__).resolve().parents[1]
        if args.replay_dynamic is not None:
            if args.output is not None or args.baseline_ref is not None:parser.error('replay accepts only the completed inventory')
            report=dynamic.verify(args.replay_dynamic.resolve(),root)
        else:
            if args.output is None:parser.error('check-dynamic requires a new private output directory')
            report=dynamic.check(root,args.output.resolve(),args.baseline_ref)
        print(json.dumps({k:v for k,v in report.items() if k not in ('members','diagnostic_geometry_differences')}))
        return
    if args.check_stream_views:
        if args.stream_views or any(getattr(args,name) is not None for name in ('tool','baseline','candidate','output','isolation_cgroup')):
            parser.error('check-stream-views is separate from capture')
        view_audit.check(pathlib.Path(__file__).resolve().parents[1],args.baseline_ref)
        return
    if args.baseline_ref or any(getattr(args,name) is None for name in ('tool','baseline','candidate','output','isolation_cgroup')):
        parser.error('capture requires tool, baseline, candidate, output and isolation-cgroup')
    report = capture(*(p.resolve() for p in (args.tool, args.baseline, args.candidate, args.output)), args.isolation_cgroup, args.stream_views, args.iterations)
    print(json.dumps({'inventory_collected': True, 'layout_mismatches': len(report['layout_mismatches']),
                      'release_approval': False, 'candidate_attribution': None}))


if __name__ == '__main__': main()
