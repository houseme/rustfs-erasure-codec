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

    def test_native_fault_adapter_reuses_the_actual_bounded_procfs_parser(self):
        expected=sampling.block((ROOT/'scripts/diagnostics/stream_core.rs').read_text(),'fn faults()')
        native=audit.native_template()
        self.assertIn(expected,native)
        self.assertNotIn('process_faults()',native)
        self.assertIn('process_faults(read_bounded(',native)

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
