"""The remaining scope is a single bounded diagnostic, never release timing."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import dynamic_buffer_audit as audit
import release_sampling as sampling

ROOT = pathlib.Path(__file__).resolve().parents[2]


class DynamicScopeTests(unittest.TestCase):
    def test_complete_original_catalog_and_formal_renderer_are_preserved(self):
        before = sampling.render(ROOT)
        observed = audit.render(ROOT)
        self.assertEqual(before, sampling.render(ROOT))
        self.assertEqual(audit.PROFILES, ('smoke', 'small_file', 'hotspot', 'stream'))
        self.assertIn('static RSE_DYNAMIC_ALLOCATOR', observed['root.rs'])
        self.assertNotIn('RSE_DYNAMIC_ALLOCATOR', before['root.rs'])
        for name in audit.PROFILES:
            self.assertIn('Epoch::begin()', observed[name + '.rs'])
        for name in ('smoke', 'hotspot'):
            self.assertIn('"source"', observed[name + '.rs'])

    def test_unknown_source_layout_is_refused_before_mutation(self):
        matrix = (ROOT/'src/matrix.rs').read_text()
        with self.assertRaises(ValueError):
            audit.patch_matrix(matrix.replace('let mut work = self.augment', 'let mut work = self.other'))
        self.assertEqual(matrix, (ROOT/'src/matrix.rs').read_text())

    def test_source_scopes_do_not_replace_original_initialization_or_publication(self):
        original = (ROOT/'src/core/reconstruct.rs').read_text()
        changed = audit.patch_decode(original)
        self.assertEqual(changed.count('destination.copy_from_slice(&bytes);'), original.count('destination.copy_from_slice(&bytes);'))
        self.assertEqual(changed.count('get_or_initialize('), original.count('get_or_initialize('))
        self.assertIn('Region::new("decode")', changed)
        self.assertNotIn(audit.MODULE, original)




def generated_source_fixture():
    """Independent six-owner encode lifecycle, with no native timing evidence."""
    events=[];next_id=0
    empty={'status':'empty','pages':0,'resident_pages':0,'errno':None}
    unavailable={'status':'unsupported','pages':None,'resident_pages':None,'errno':None}
    for observation in range(12):
        for iteration in (1,2):
            call=observation*2+iteration
            def event(kind,**fields):
                value={'kind':kind,'observation':observation,'call':call,'thread':0,'region':'caller',
                    'allocation':None,'allocation_offset':None,'previous':None,'role':'','stage':'','index':0,
                    'len':0,'capacity':None,'element_size':1,'shape':None,'storage':'','occupied':False,'present':None,
                    'bytes':0,'align':0,'offsets':None,'elapsed_ns':len(events),'faults':None,'snapshot':dict(empty)}
                value.update(fields);events.append(value)
            event('begin');ids=[]
            for index in range(6):
                next_id+=1;ids.append(next_id);position=(next_id*16)%4096
                event('alloc',allocation=next_id,allocation_offset=0,bytes=65536,align=1,offsets=[position%64,position,position],snapshot=dict(unavailable))
            for stage in ('created','encoded'):
                for index,ident in enumerate(ids):
                    position=(ident*16)%4096
                    event('view',allocation=ident,allocation_offset=0,role='source',stage=stage,index=index,
                        len=65536,capacity=65536,storage='heap',occupied=True,present=True,bytes=65536,
                        offsets=[position%64,position,position],snapshot=dict(unavailable))
            for ident in ids:
                position=(ident*16)%4096
                event('free',allocation=ident,allocation_offset=0,bytes=65536,align=1,
                    offsets=[position%64,position,position],snapshot=dict(unavailable))
            event('end')
    final=dict(events[-1]);final.update(kind='finish',call=0,region='between',elapsed_ns=len(events));events.append(final)
    return {'schema':'dynamic-buffer-lifecycle-v1','page_size':4096,'errors':0,'calls':[2]*12,'release_approval':False,'events':events}


class AllocationLifecycleTests(unittest.TestCase):
    def test_every_generated_owner_is_observed_and_freed_without_mutating_input(self):
        import copy
        data=generated_source_fixture();original=copy.deepcopy(data)
        result=audit.validate_ledger(data,'smoke','encode:4:2:65536',4096)
        self.assertEqual(result['iterations'],24)
        self.assertEqual(result['allocation_generations'],144)
        self.assertEqual(result['retained_requests'],0)
        self.assertEqual(data,original)

    def test_dead_foreign_or_reused_generation_cannot_be_relabelled_as_retained(self):
        import copy
        data=generated_source_fixture()
        view=next(v for v in data['events'] if v['kind']=='view' and v['stage']=='encoded')
        other=next(v for v in data['events'] if v['kind']=='alloc' and v['allocation']==2)
        view['allocation']=2;view['offsets']=other['offsets'][:]
        with self.assertRaisesRegex(ValueError,'replaced generated source'):
            audit.validate_ledger(data,'smoke','encode:4:2:65536',4096)
        data=generated_source_fixture()
        first=next(i for i,v in enumerate(data['events']) if v['kind']=='free')
        owned_view=next(i for i,v in enumerate(data['events']) if v['kind']=='view' and v['stage']=='encoded' and v['allocation']==1)
        data['events'][first],data['events'][owned_view]=data['events'][owned_view],data['events'][first]
        for index,event in enumerate(data['events']):event['elapsed_ns']=index
        with self.assertRaisesRegex(ValueError,'live allocation'):
            audit.validate_ledger(data,'smoke','encode:4:2:65536',4096)

    def test_complete_order_geometry_sizes_privacy_and_presence_are_required(self):
        import copy
        changes=[lambda d:d['events'].pop(),lambda d:d['calls'].__setitem__(0,True),
            lambda d:d['events'][1].update(address=1234),lambda d:d['events'][1].update(align=3),
            lambda d:d['events'][1].update(allocation=2),lambda d:d['events'][7].update(len=True),
            lambda d:d['events'][7].update(capacity=65535),lambda d:d['events'][7].update(present=False),
            lambda d:d['events'][7].update(allocation_offset=1),lambda d:d['events'][7].update(offsets=[0,16,16]),
            lambda d:d['events'][0].update(role='source'),lambda d:d['events'][0].update(region=[]),
            lambda d:d.update(release_approval=True),lambda d:d.update(errors=1)]
        for change in changes:
            data=generated_source_fixture();change(data);original=copy.deepcopy(data)
            with self.assertRaises(ValueError):audit.validate_ledger(data,'smoke','encode:4:2:65536',4096)
            self.assertEqual(data,original)

    def test_native_residency_and_faults_are_not_inferred_from_portable_rows(self):
        with self.assertRaises(ValueError):audit.validate_ledger(generated_source_fixture(),'smoke','encode:4:2:65536',4096,native=True)


class DiagnosticOwnershipTests(unittest.TestCase):
    def test_existing_output_remains_untouched_on_refusal(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            root=pathlib.Path(folder);(root/'keep').write_text('original')
            with self.assertRaises(FileExistsError):audit.check(ROOT,root)
            self.assertEqual([p.name for p in root.iterdir()],['keep'])

    def test_source_overlay_restores_failures_and_preserves_foreign_edits(self):
        import tempfile
        for tamper in (False,True):
            with tempfile.TemporaryDirectory() as folder:
                root=pathlib.Path(folder)
                for name in audit.NAMES:
                    p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text((ROOT/name).read_text())
                original={name:(root/name).read_text() for name in audit.NAMES}
                with self.assertRaises(ValueError if tamper else RuntimeError):
                    with audit.source_overlay(root):
                        if tamper:(root/'src/matrix.rs').write_text('foreign edit')
                        else:raise RuntimeError('interrupted worker')
                if tamper:self.assertEqual((root/'src/matrix.rs').read_text(),'foreign edit')
                else:
                    self.assertEqual({name:(root/name).read_text() for name in audit.NAMES},original)
                    self.assertFalse((root/('src/'+audit.MODULE+'.rs')).exists())



class CompleteInventoryContractTests(unittest.TestCase):
    def test_fixed_original_modes_and_decoder_supplements_have_no_case_holes(self):
        steps=audit.inventory_steps()
        self.assertEqual(len(steps),516)
        for role in ('baseline','candidate'):
            for mode in ('calibrate','sample'):
                actual={(v['profile'],v['case']) for v in steps if v['role']==role and v['mode']==mode}
                expected={(p,c) for p in audit.PROFILES for c in audit.e.expected_cases(p)}
                self.assertEqual(actual,expected)
                self.assertEqual(len(actual),126)
            self.assertEqual({v['case'] for v in steps if v['role']==role and v['profile']=='decoder'},set(audit.SUPPLEMENTS))
        self.assertEqual(len({tuple(sorted(v.items())) for v in steps}),len(steps))

    def test_raw_dynamic_workload_is_refused_by_formal_validation(self):
        import copy
        for profile in audit.PROFILES:
            case=sorted(audit.e.expected_cases(profile))[0]
            contract=audit.s.workload_contract(profile,case)
            contract['rng']=None
            raw={'case':case,'workload':{'observer':'dynamic-source-copy-v1','contract':contract}}
            before=copy.deepcopy(raw)
            with self.assertRaises(ValueError):audit.s.validate_workload(raw,profile)
            decoded=audit.underlying_records([raw])
            self.assertEqual(decoded[0]['workload'],contract)
            self.assertEqual(raw,before)
            for change in (lambda d:d['workload'].update(observer='formal'),lambda d:d['workload'].update(address=1),lambda d:d.update(workload=contract)):
                bad=copy.deepcopy(raw);change(bad)
                with self.assertRaises(ValueError):audit.underlying_records([bad])

    def test_leopard_lanes_and_initialization_are_actual_typed_extents(self):
        import copy
        data={'role':'leopard_work','shape':[16,32,32768],'capacity':524288,'len':524288,
              'element_size':2,'stage':'chunk_coded','index':0,'present':None,'occupied':True,'storage':'heap'}
        audit.validate_internal_view(data,'decoder','leopard_gf16:10:4:65600',False)
        for key,value in [('shape',[16,32769,32768]),('shape',[True,32,32768]),('capacity',524287),('len',0),('element_size',1),('occupied',False),('stage','unknown')]:
            bad=copy.deepcopy(data);bad[key]=value
            with self.assertRaises(ValueError):audit.validate_internal_view(bad,'decoder','leopard_gf16:10:4:65600',False)

    def test_legacy_all_shard_temporary_owners_are_not_relabelled_missing_only(self):
        data={'role':'leopard_decoded','shape':None,'index':1,'len':32768,'capacity':32768,
              'element_size':1,'stage':'decoded','present':None,'occupied':True,'storage':'heap'}
        audit.validate_internal_view(data,'decoder','leopard_gf8:10:4:32768',True)
        with self.assertRaises(ValueError):audit.validate_internal_view(data,'decoder','leopard_gf8:10:4:32768',False)

    def envelope(self,folder):
        import json,copy
        candidate={'commit':'c'*40,'tree':'d'*40,'lock_sha256':'f'*64}
        baseline={'commit':audit.e.BASELINE_COMMIT,'tree':'e'*40,'lock_sha256':'f'*64}
        context={'compiler':'rustc fixture','machine':'fixture','cpu_identity':'fixture','cpu_affinity':None,'page_size':4096,
                 'kernel_memory_policy':None,'libc':['',''],'allocator_environment':{},'settings':copy.deepcopy(audit.e.SAMPLING_ENV)}
        report={'schema':'dynamic-buffer-check-v1','release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,
                'tool':candidate,'sources':{'candidate':candidate,'baseline':baseline},'cases':{'candidate':{},'baseline':{}},
                'source_overlays':{'candidate':{},'baseline':{}},'harness_sha256':{'candidate':'a'*64,'baseline':'a'*64},
                'overlay_sha256':{'candidate':'b'*64,'baseline':'b'*64},'context':context,'page_size':4096,'cpu_affinity':None,
                'native_sha256':'1'*64,'sampler_implementation_sha256':'2'*64,'controller_sha256':'3'*64}
        steps=audit.inventory_steps();plan=copy.deepcopy(report);plan['steps']=steps
        (folder/'plan.json').write_text(json.dumps(plan))
        entries=[]
        for step in steps:
            key=step['mode']+'/'+step['profile']+'/'+step['case'];report['cases'][step['role']][key]={}
            entries.extend([{'stage':'begin','step':step,'context':context},{'stage':'end','step':step,'context':context,'entry':{}}])
        (folder/'execution.jsonl').write_text(''.join(json.dumps(v)+'\n' for v in entries))
        return report,{candidate['commit']:candidate,baseline['commit']:baseline}

    def test_complete_plan_context_and_every_execution_boundary_are_bound(self):
        import copy,json,tempfile
        from unittest import mock
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);report,sources=self.envelope(root)
            with mock.patch.object(audit.e,'source_at',side_effect=lambda root,revision:copy.deepcopy(sources[revision])):
                audit.validate_envelope(report,root,ROOT)
                raw=(root/'execution.jsonl').read_text();entries=[json.loads(v) for v in raw.splitlines()]
                for mutation in ('missing','order','context','summary'):
                    bad=copy.deepcopy(entries)
                    if mutation=='missing':bad.pop()
                    elif mutation=='order':bad[0],bad[2]=bad[2],bad[0]
                    elif mutation=='context':bad[33]['context']['settings']['RAYON_NUM_THREADS']='1'
                    else:bad[33]['entry']={'forged':True}
                    (root/'execution.jsonl').write_text(''.join(json.dumps(v)+'\n' for v in bad))
                    with self.assertRaises(ValueError):audit.validate_envelope(report,root,ROOT)
                (root/'execution.jsonl').write_text(raw)
                for change in (lambda d:d['context']['settings'].update(RAYON_NUM_THREADS='1'),lambda d:d['tool'].update(commit='--help'),lambda d:d.update(address=123)):
                    bad=copy.deepcopy(report);change(bad)
                    with self.assertRaises(ValueError):audit.validate_envelope(bad,root,ROOT)


if __name__ == '__main__':
    unittest.main()
