"""Structural inventory fixtures never represent measured performance."""
import copy
import json
import subprocess
import tempfile
from unittest import mock
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_fixture_layout as inventory
import stream_view_audit as audit
from layout_fixture import view_fixture
import release_evidence as evidence
import release_sampling as sampling
import test_release_evidence as fixtures


class FixtureInventoryTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.EvidenceTests();self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
        self.sources=self.fixture.sources
        self.phases={}
        for phase in evidence.PHASES:
            role='baseline' if phase.startswith('A') else 'candidate'
            rows=[(profile,row) for profile in inventory.PROFILES for row in self.fixture.fixture_rows(phase,profile)]
            records=[]
            for profile,row in rows:
                for index in range(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']):
                    kind='warmup' if index<sampling.PROTOCOL['warmups'] else 'sample'
                    records.append(self.fixture.record(row,profile,role,kind,index))
            self.phases[phase]=records

    def test_failed_inventory_keeps_completed_rows_and_cleans_only_owned_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';candidate=root/'candidate'
            for source in (baseline,candidate):(source/'tests').mkdir(parents=True)
            output=root/'output';tool=pathlib.Path(sampling.__file__).resolve().parents[1]
            def info(source):return {'commit':evidence.BASELINE_COMMIT if source==baseline else 'b'*40}
            def generate(source):
                (source/'tests/rse_release_sampling').mkdir()
                (source/'tests/rse_release_sampling/engine.rs').write_text('')
                (source/'tests/rse_release_sampling.rs').write_text('')
            row={'case':'incomplete-fixture'}
            def failed_profile(*args,**kwargs):
                args[4].write_text('RSE_TIMING {"index":0}\nRSE_SAMPLE '+json.dumps(row)+'\nRSE_SAMPLE {"case":')
                raise subprocess.CalledProcessError(1,['cargo'])
            with mock.patch.object(evidence,'source_info',side_effect=info), mock.patch.object(evidence,'environment_context',return_value={'isolation':{'cpu_affinity':[0,1]},'runtime':{'page_size':4096}}), mock.patch.object(evidence,'harness_digest',return_value='a'*64), mock.patch.object(sampling,'render',return_value={'root.rs':'','engine.rs':''}), mock.patch.object(sampling,'generate',side_effect=generate), mock.patch.object(sampling,'run_profile',side_effect=failed_profile), mock.patch.object(inventory.subprocess,'run'), mock.patch.object(inventory.os,'sched_getaffinity',return_value={0,1},create=True), mock.patch.object(inventory.os,'sched_setaffinity',create=True):
                with self.assertRaises(subprocess.CalledProcessError):
                    inventory.capture(tool,baseline,candidate,output,root/'group')
            self.assertEqual(json.loads((output/'A1-samples.partial.json').read_text()),[row])
            self.assertEqual(json.loads((output/'A1-timing.partial.json').read_text()),[{'index':0}])
            self.assertFalse((output/'inventory.json').exists())
            for source in (baseline,candidate):
                self.assertFalse((source/'tests/rse_release_sampling').exists())
                self.assertFalse((source/'tests/rse_release_sampling.rs').exists())

    def test_complete_inventory_is_never_release_approval(self):
        original=copy.deepcopy(self.phases)
        with mock.patch.object(inventory.native_tools,'request',wraps=inventory.native_tools.request) as calls:
            report=inventory.analyze(self.phases,self.sources,range(8))
        self.assertEqual(calls.call_count,1)
        self.assertEqual(self.phases,original)
        self.assertEqual(report['case_count'],103)
        self.assertEqual(len(report['layout_checks']),618)
        self.assertEqual(report['layout_mismatches'],[])
        self.assertFalse(report['release_approval'])
        self.assertIsNone(report['candidate_attribution'])

    def test_cross_version_layout_differences_are_retained_without_timing_attribution(self):
        for phase in ('B1','B2'):
            for record in self.phases[phase]:
                if not record['case'].startswith('verify_with_buffer:'): continue
                record['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
        report=inventory.analyze(self.phases,self.sources,range(8))
        self.assertEqual(len(report['layout_mismatches']),44)
        self.assertTrue(all(check['pair'] not in ('A1/A2','B1/B2') for check in report['layout_mismatches']))
        self.assertTrue(all(check['different_buffers'] for check in report['layout_mismatches']))
        self.assertIsNone(report['candidate_attribution'])

    def test_incomplete_or_mixed_worker_layouts_are_rejected(self):
        original=copy.deepcopy(self.phases)
        for change in (lambda phases:phases.pop('A2'),lambda phases:phases['B2'].pop(),
                       lambda phases:phases['A1'][0]['fixture']['source'][0].update(capacity=2048)):
            phases=copy.deepcopy(original);change(phases)
            with self.assertRaises(ValueError):inventory.analyze(phases,self.sources,range(8))

    def test_invalid_cpu_or_more_than_one_iteration_cannot_enter_inventory(self):
        for change in (lambda r:r.update(iterations=2),lambda r:r['after'].update(cpu=7)):
            phases=copy.deepcopy(self.phases);change(phases['B1'][0])
            with self.assertRaises(ValueError):inventory.analyze(phases,self.sources,range(8))


class StreamViewIntegrationTests(unittest.TestCase):
    def test_diagnostic_entries_are_explicit_and_normalization_keeps_originals(self):
        helper=fixtures.EvidenceTests(methodName='runTest');helper.setUp();self.addCleanup(helper.doCleanups)
        row=helper.fixture_rows('A1','stream')[0];sample=helper.record(row,'stream','baseline','sample',3)
        operation=sample['case'].split(':')[0]
        for field in ('fixture','workload'):sample[field]['call_entry']=audit.ENTRIES[operation]
        normalized=audit.underlying_records([sample])[0]
        self.assertNotEqual(normalized['fixture']['call_entry'],sample['fixture']['call_entry'])
        sampling.validate_workload(normalized,'stream')
        with self.assertRaises(ValueError):sampling.validate_workload(sample,'stream')


    def test_decorator_does_not_change_the_formal_renderer_or_library(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        before=sampling.render(root);traced=audit.render(root)
        self.assertEqual(before,sampling.render(root))
        self.assertNotIn('stream_views.rs',before)
        self.assertIn('stream_views.rs',traced)
        self.assertLess(traced['stream_views.rs'].index('fn read_bounded'),traced['stream_views.rs'].rindex('mod tests {'))
        engine=(sampling.TEMPLATES/'engine.rs').read_text()
        for declaration in ('fn read_bounded','fn process_faults'):
            self.assertIn(sampling.block(engine,declaration),traced['stream_views.rs'])
        for name in before:
            if name not in ('root.rs','stream.rs'):self.assertEqual(before[name],traced[name])
        self.assertIn('super::stream_views::cursors(&cursors, 8)',traced['stream.rs'])
        self.assertIn('super::stream_views::drop_outputs(writers); drop(readers);',traced['stream.rs'])
        import diagnose_cpu_placement
        placement=diagnose_cpu_placement.render(root,allocation_check=True)
        self.assertNotIn('stream_views::',placement['root.rs'])
        self.assertNotIn('stream_views.rs',placement)


    def inventory_fixture(self):
        helper=fixtures.EvidenceTests(methodName='runTest');helper.setUp();self.addCleanup(helper.doCleanups)
        phases={};views={}
        for phase in evidence.PHASES:
            role='baseline' if phase.startswith('A') else 'candidate';phases[phase]=[];views[phase]=[]
            for row in helper.fixture_rows(phase,'stream'):
                for index in range(12):
                    record=helper.record(row,'stream',role,'warmup' if index<3 else 'sample',index)
                    record['iterations']=2;record['rows'][0]['iterations']=2;record['timed_ns']*=2
                    operation=record['case'].split(':')[0]
                    for field in ('fixture','workload'):record[field]['call_entry']=audit.ENTRIES[operation]
                    phases[phase].append(record);views[phase].append(view_fixture(record['case'],observation=index))
        return helper,phases,views


    def test_cross_phase_backend_io_and_constructor_budget_mismatch_are_rejected(self):
        helper,original,views=self.inventory_fixture()
        for field in ('features','backend_id','stream_io_mode','constructor_budget'):
            phases=copy.deepcopy(original)
            for row in phases['B1']:
                if field=='constructor_budget':row['placement'][field]=7
                else:row['rows'][0][field]='serial' if field=='stream_io_mode' else 'std' if field=='features' else 'changed'
            with self.subTest(field=field):
                with self.assertRaises(ValueError):inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)


    def test_linux_inventory_rejects_missing_lifecycle_counters_in_raw_data(self):
        helper,phases,views=self.inventory_fixture()
        for view in views['B1']:
            for window in view['windows']:window.update(before=None,after=None)
        with self.assertRaisesRegex(ValueError,'Linux lifecycle counters'):
            inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)


    def test_four_phase_inventory_reports_differences_without_performance_approval(self):
        helper,phases,views=self.inventory_fixture()
        original=copy.deepcopy(views)
        with mock.patch.object(inventory.native_tools,'request',wraps=inventory.native_tools.request) as calls:
            report=inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)
        batches=[call.kwargs['request'] for call in calls.call_args_list if call.args[0]=='stream-views']
        self.assertEqual(len(batches),180)
        self.assertTrue(all(batch['operation']=='inventory' and len(batch['observations'])<=4 for batch in batches))
        self.assertEqual(len(report['lifecycle_windows']['A1']),180)
        self.assertEqual(views,original)
        self.assertEqual(report['observations'],720)
        self.assertEqual(report['case_count'],15)
        self.assertFalse(report['release_approval']);self.assertFalse(report['coverage']['internal_verify_scratch'])
        self.assertFalse(report['layout_mismatches'])
        event=next(e for e in views['B1'][0]['events'] if e['role']=='read_scratch_view');event['offsets']=[32,32,32]
        for other in views['B1'][0]['events']:
            if other['call']==event['call'] and other['role']==event['role'] and other['index']==event['index']:other['offsets']=[32,32,32]
        for value in views['B1'][0]['residency']:
            if value['call']==event['call'] and value['role']==event['role'] and value['index']==event['index']:value['offset_page']=32
        report=inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)
        self.assertTrue(report['layout_mismatches'])
        with self.assertRaises(ValueError):inventory.analyze_stream_views(phases,dict(views,A2=[]),helper.sources,list(range(8)),4096,2)


if __name__=='__main__':unittest.main()
