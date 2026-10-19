"""Synthetic replay tests never establish performance acceptance."""
import copy
import json
import pathlib
import shutil
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import analyze_baseline_variability as analysis
import release_sampling as sampling
import test_release_evidence as fixtures


class BaselineVariabilityTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.EvidenceTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        shutil.copytree(self.root/'calibration-baseline', self.root/'calibration')
        self.provenance = {
            'schema_version': 'baseline-basic-controls-v1',
            'source': self.fixture.sources['baseline'], 'tool': self.fixture.sources['candidate'],
            'protocol': copy.deepcopy(sampling.PROTOCOL), 'renderer_sha256': 'e'*64,
            'context': self.fixture.doc['contexts']['A1'],
        }
        self.save('provenance.json', self.provenance)

    def save(self, name, data):
        (self.root/name).write_text(json.dumps(data))

    def mutate(self, name, work):
        records = json.loads((self.root/name).read_text())
        work(records)
        self.save(name, records)

    def test_complete_catalog_retains_warmups_and_never_approves(self):
        report = analysis.analyze(self.root)
        self.assertEqual(report['case_count'], 126)
        self.assertEqual(report['repeat_endpoint_counts'], {'single_endpoint_cpu_same': {'cases': 126, 'repeat_failures': 0}})
        self.assertEqual(len(report['input_sha256']), 13)
        self.assertFalse(report['release_approval'])
        self.assertIsNone(report['candidate_attribution'])
        self.assertTrue(all(len(case['phases']['A1']['trace']) == 12 for case in report['cases']))

    def test_same_cpu_can_have_repeat_drift_without_being_dropped(self):
        def change(records):
            for record in records:
                if record['case'] == 'encode:10:4:524288':
                    record['timed_ns'] *= 1.2
                    record['rows'][0]['ns_per_iter'] *= 1.2
        self.mutate('A2/small_file-samples.json', change)
        report = analysis.analyze(self.root)
        case = next(c for c in report['cases'] if c['profile'] == 'small_file' and c['case'] == 'encode:10:4:524288')
        self.assertAlmostEqual(case['repeat']['absolute_metric_drift'], .2)
        self.assertEqual(case['repeat']['endpoint_category'], 'single_endpoint_cpu_same')
        self.assertEqual(report['repeat_endpoint_counts']['single_endpoint_cpu_same']['repeat_failures'], 1)

    def test_fixed_thread_contract_rejects_changed_and_multiple_endpoints(self):
        def change(records):
            for record in records:
                if record['case'] == 'encode:4:2:1024':
                    record['before']['cpu'] = record['after']['cpu'] = 1
                if record['case'] == 'encode:4:2:4096' and record['index'] == 3:
                    record['after']['cpu'] = 1
        self.mutate('A2/small_file-samples.json', change)
        with self.assertRaisesRegex(ValueError, 'fixed measuring thread'):
            analysis.analyze(self.root)

    def test_complete_first_phase_without_repeat_has_no_repeat_claim(self):
        shutil.rmtree(self.root/'A2')
        report = analysis.analyze(self.root)
        self.assertEqual(report['phase_order'], ['A1'])
        self.assertFalse(report['repeat_endpoint_counts'])
        self.assertTrue(all(c['repeat'] is None for c in report['cases']))
        self.assertFalse(report['release_approval'])

    def test_incomplete_phase_and_mismatched_protocol_reject(self):
        name = 'A2/small_file-samples.json'
        original = (self.root/name).read_bytes()
        self.mutate(name, lambda records: records.pop())
        with self.assertRaises(ValueError):
            analysis.analyze(self.root)
        (self.root/name).write_bytes(original)
        self.provenance['protocol']['version'] -= 1
        self.save('provenance.json', self.provenance)
        with self.assertRaisesRegex(ValueError, 'frozen protocol'):
            analysis.analyze(self.root)

    def test_counter_escape_and_backwards_counters_reject(self):
        name = 'A2/small_file-samples.json'
        original = (self.root/name).read_bytes()
        for point, field, value in [('after', 'cpu', 99), ('after', 'run_ns', -1)]:
            (self.root/name).write_bytes(original)
            self.mutate(name, lambda records: records[0][point].update({field: value}))
            with self.assertRaises(ValueError):
                analysis.analyze(self.root)

    def test_short_intervals_and_layout_mismatches_remain_visible(self):
        def change(records):
            for record in records:
                if record['case'] == 'encode:4:2:1024':
                    record['wall_ns'] = 1
                    for position in record['fixture']['source']:
                        position.update(offset_64=32, offset_4096=32, offset_page=32)
        self.mutate('A2/small_file-samples.json', change)
        report = analysis.analyze(self.root)
        self.assertEqual(report['phase_quality']['A2']['short_samples'], 9)
        self.assertEqual(report['phase_quality']['A2']['layout_mismatches'], [{'profile': 'small_file', 'case': 'encode:4:2:1024'}])
        # Telemetry surrounds the wall timer. Do not clamp the resulting ratio.
        case = next(c for c in report['cases'] if c['profile'] == 'small_file' and c['case'] == 'encode:4:2:1024')
        self.assertGreater(case['phases']['A2']['trace'][3]['main_run_to_wall'], 1)

    def test_published_baseline_and_matching_renderer_are_required(self):
        self.provenance['source'] = self.fixture.sources['candidate']
        self.save('provenance.json', self.provenance)
        with self.assertRaisesRegex(ValueError, 'published-9.0.0'):
            analysis.analyze(self.root)
        self.provenance['source'] = self.fixture.sources['baseline']
        self.provenance['renderer_sha256'] = 'f'*64
        self.save('provenance.json', self.provenance)
        with self.assertRaisesRegex(ValueError, 'renderer'):
            analysis.analyze(self.root)


if __name__ == '__main__':
    unittest.main()
