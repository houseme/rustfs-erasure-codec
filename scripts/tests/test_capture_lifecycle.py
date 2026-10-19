"""Exercise capture ownership and failure retention, never performance acceptance."""
from contextlib import ExitStack
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as evidence


class CaptureLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.baseline, self.candidate = (self.root/name for name in ('baseline', 'candidate'))
        for root in (self.baseline, self.candidate): (root/'tests').mkdir(parents=True)
        self.output = self.root/'capture'
        self.payload = {'case': 'synthetic-lifecycle-only', 'index': 0}
        self.stack = ExitStack(); self.addCleanup(self.stack.close)
        def patch(target, **kwargs): return self.stack.enter_context(mock.patch(target, **kwargs))
        patch('release_evidence.source_info', side_effect=lambda root: {'commit': evidence.BASELINE_COMMIT if root == self.baseline else 'b'*40})
        self.context = patch('release_evidence.environment_context', return_value={})
        patch('release_evidence.harness_digest', return_value='c'*64)
        patch('release_evidence.sampling.implementation_digest', return_value='d'*64)
        patch('release_evidence.sampling.overlay_digest', return_value='e'*64)
        patch('release_evidence.sampling.render', return_value={'root.rs': '// generated entry', 'stub.rs': '// generated module'})
        self.build = patch('release_evidence.subprocess.run')
        self.run = patch('release_evidence.sampling.run_profile', side_effect=self.emit)
        patch('release_evidence.calibration_counts', side_effect=lambda records, profile, revision: {case: 1 for case in evidence.expected_cases(profile)})
        patch('release_evidence.sampling.selected_row', return_value=([self.payload], []))
        patch('release_evidence.time.sleep')
        self.verify = patch('release_evidence.verify', return_value=({}, {}, {'status': 'passed', 'test_only': True}))

    def emit(self, root, env, profile, mode, path, count_file=None, **kwargs):
        path.write_text('RSE_TIMING '+json.dumps(self.payload)+'\nRSE_SAMPLE '+json.dumps(self.payload)+'\n')
        return [self.payload]

    def capture(self): return evidence.capture(self.baseline, self.candidate, self.output, self.root/'unused-group')

    def assert_removed(self, root):
        self.assertFalse((root/'tests/rse_release_sampling.rs').exists())
        self.assertFalse((root/'tests/rse_release_sampling').exists())

    def test_success_removes_both_byte_identical_overlays(self):
        self.capture()
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertTrue((self.output/'manifest.json').exists())
        self.assertEqual(self.run.call_count, 24)  # 8 calibration + 16 phase profiles.

    def test_build_failure_removes_already_acquired_overlay(self):
        self.build.side_effect = subprocess.CalledProcessError(1, ['cargo'])
        with self.assertRaises(subprocess.CalledProcessError): self.capture()
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertFalse((self.output/'manifest.json').exists())

    def test_second_acquisition_failure_preserves_existing_candidate_input(self):
        foreign = self.candidate/'tests/rse_release_sampling.rs'; foreign.write_text('user source')
        with self.assertRaisesRegex(ValueError, 'overwrite'): self.capture()
        self.assert_removed(self.baseline)
        self.assertEqual(foreign.read_text(), 'user source')

    def test_calibration_and_measurement_failures_retain_completed_numeric_journals(self):
        for mode, folder in (('calibrate', 'calibration-baseline'), ('sample', 'A1')):
            if self.output.exists():
                import shutil
                shutil.rmtree(self.output)
            def fail(root, env, profile, current_mode, path, count_file=None, **kwargs):
                rows = self.emit(root, env, profile, current_mode, path, count_file, **kwargs)
                if current_mode == mode:
                    with path.open('a') as log: log.write('RSE_SAMPLE {"interrupted":')
                    raise subprocess.CalledProcessError(1, ['cargo'])
                return rows
            self.run.side_effect = fail
            with self.assertRaises(subprocess.CalledProcessError): self.capture()
            for kind in ('samples', 'timing'):
                self.assertEqual(json.loads((self.output/folder/('smoke-'+kind+'.partial.json')).read_text()), [self.payload])
            for root in (self.baseline, self.candidate): self.assert_removed(root)
            self.assertFalse((self.output/'manifest.json').exists())

    def test_keyboard_interrupt_retains_journal_and_releases_overlays(self):
        def interrupt(*args, **kwargs):
            self.emit(*args, **kwargs)
            raise KeyboardInterrupt()
        self.run.side_effect = interrupt
        with self.assertRaises(KeyboardInterrupt): self.capture()
        self.assertEqual(json.loads((self.output/'calibration-baseline/smoke-samples.partial.json').read_text()), [self.payload])
        for root in (self.baseline, self.candidate): self.assert_removed(root)

    def test_malformed_journal_preserves_raw_log_and_original_worker_failure(self):
        def fail(*args, **kwargs):
            self.emit(*args, **kwargs)
            with args[4].open('a') as log: log.write('RSE_SAMPLE {broken}\n')
            raise subprocess.CalledProcessError(7, ['cargo'])
        self.run.side_effect = fail
        with self.assertRaises(subprocess.CalledProcessError) as raised: self.capture()
        self.assertEqual(raised.exception.returncode, 7)
        folder = self.output/'calibration-baseline'
        self.assertIn('{broken}', (folder/'smoke.log').read_text())
        self.assertEqual(json.loads((folder/'smoke-samples.error.json').read_text())['status'], 'failed')
        self.assertEqual(json.loads((folder/'smoke-timing.partial.json').read_text()), [self.payload])
        for root in (self.baseline, self.candidate): self.assert_removed(root)

    def test_context_change_retains_completed_rows_before_rejecting(self):
        self.context.side_effect = [{}, {}, {}, {'changed': True}]
        with self.assertRaisesRegex(ValueError, 'environment changed'): self.capture()
        path = self.output/'calibration-baseline/smoke-samples.json'
        self.assertEqual(json.loads(path.read_text()), [self.payload])
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertFalse((self.output/'manifest.json').exists())

    def test_final_verification_failure_preserves_complete_data_and_releases_overlays(self):
        self.verify.side_effect = evidence.EvidenceFailure('intentional invalid evidence', {})
        with self.assertRaises(evidence.EvidenceFailure): self.capture()
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertTrue((self.output/'A2/stream-samples.json').exists())
        self.assertFalse((self.output/'report.json').exists())

    def test_changed_generated_source_is_preserved(self):
        def fail(*args, **kwargs):
            self.emit(*args, **kwargs)
            (self.baseline/'tests/rse_release_sampling/stub.rs').write_text('changed source')
            raise subprocess.CalledProcessError(1, ['cargo'])
        self.run.side_effect = fail
        with self.assertRaisesRegex(ValueError, 'preserve'): self.capture()
        self.assertEqual((self.baseline/'tests/rse_release_sampling/stub.rs').read_text(), 'changed source')
        self.assert_removed(self.candidate)


if __name__ == '__main__': unittest.main()
