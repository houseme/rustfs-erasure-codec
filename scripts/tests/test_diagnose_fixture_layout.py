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
            rows=[row for row in self.fixture.fixture_rows(phase,'small_file') if row['operation']=='verify_with_buffer']
            records=[]
            for row in rows:
                for index in range(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']):
                    kind='warmup' if index<sampling.PROTOCOL['warmups'] else 'sample'
                    records.append(self.fixture.record(row,'small_file',role,kind,index))
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
        report=inventory.analyze(self.phases,self.sources,{0})
        self.assertEqual(report['case_count'],11)
        self.assertEqual(len(report['layout_checks']),66)
        self.assertEqual(report['layout_mismatches'],[])
        self.assertFalse(report['release_approval'])
        self.assertIsNone(report['candidate_attribution'])

    def test_cross_version_layout_differences_are_retained_without_timing_attribution(self):
        for phase in ('B1','B2'):
            for record in self.phases[phase]:
                record['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
        report=inventory.analyze(self.phases,self.sources,{0})
        self.assertEqual(len(report['layout_mismatches']),44)
        self.assertTrue(all(check['pair'] not in ('A1/A2','B1/B2') for check in report['layout_mismatches']))
        self.assertTrue(all(check['different_buffers'] for check in report['layout_mismatches']))
        self.assertIsNone(report['candidate_attribution'])

    def test_incomplete_or_mixed_worker_layouts_are_rejected(self):
        original=copy.deepcopy(self.phases)
        for change in (lambda phases:phases.pop('A2'),lambda phases:phases['B2'].pop(),
                       lambda phases:phases['A1'][0]['fixture']['source'][0].update(capacity=2048)):
            phases=copy.deepcopy(original);change(phases)
            with self.assertRaises(ValueError):inventory.analyze(phases,self.sources,{0})

    def test_invalid_cpu_or_more_than_one_iteration_cannot_enter_inventory(self):
        for change in (lambda r:r.update(iterations=2),lambda r:r['after'].update(cpu=7)):
            phases=copy.deepcopy(self.phases);change(phases['B1'][0])
            with self.assertRaises(ValueError):inventory.analyze(phases,self.sources,{0})


if __name__=='__main__':unittest.main()
