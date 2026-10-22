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

    def qualification_provenance(self):
        self.provenance.update(schema_version='baseline-qualification-v1',
            sampling=dict(sampling.PROTOCOL, implementation_sha256='e'*64, overlay_sha256='f'*64),
            release_approval=False, candidate_attribution=None)
        self.provenance.pop('protocol'); self.provenance.pop('renderer_sha256')
        self.save('provenance.json', self.provenance)

    def test_shared_qualification_replays_all_original_rows_without_approval(self):
        self.qualification_provenance()
        report = analysis.analyze(self.root)
        self.assertEqual(report['capture_schema'], 'baseline-qualification-v1')
        self.assertEqual(report['case_count'], 126)
        self.assertFalse(report['release_approval'])
        self.assertTrue(all(len(case['phases']['A2']['trace']) == 12 for case in report['cases']))

    def test_new_qualification_validates_kernel_context_and_stays_diagnostic(self):
        self.qualification_provenance();self.provenance['schema_version']=analysis.evidence.QUALIFICATION_SCHEMA
        self.save('provenance.json',self.provenance);report=analysis.analyze(self.root)
        self.assertTrue(report['kernel_memory_policy_recorded']);self.assertEqual(report['case_count'],126)
        self.assertFalse(report['release_approval']);self.assertIsNone(report['candidate_attribution'])

    def test_v2_requires_kernel_policy_but_v1_history_remains_descriptive(self):
        self.qualification_provenance();self.provenance['context']['runtime'].pop('kernel_memory_policy')
        self.save('provenance.json',self.provenance)
        old=analysis.analyze(self.root)
        self.assertFalse(old['kernel_memory_policy_recorded']);self.assertFalse(old['release_approval'])
        self.provenance['schema_version']=analysis.evidence.QUALIFICATION_SCHEMA
        self.save('provenance.json',self.provenance)
        with self.assertRaisesRegex(ValueError,'kernel memory'):analysis.analyze(self.root)

    def test_qualification_missing_contract_or_approval_claim_is_rejected(self):
        self.qualification_provenance()
        original = copy.deepcopy(self.provenance)
        for change in (lambda d:d['sampling'].pop('overlay_sha256'),
                       lambda d:d['sampling'].update(version=0),
                       lambda d:d.update(release_approval=True)):
            self.provenance = copy.deepcopy(original); change(self.provenance)
            self.save('provenance.json', self.provenance)
            with self.assertRaises(ValueError): analysis.analyze(self.root)

    def test_every_raw_backend_io_and_placement_must_match_calibration(self):
        probes = (
            ('calibration/smoke-samples.json', 1, 'backend_id'),
            ('A1/smoke-samples.json', 1, 'backend_id'),
            ('A2/small_file-samples.json', 3, 'backend_id'),
            ('A2/stream-samples.json', 11, 'stream_io_mode'),
            ('A1/hotspot-samples.json', 1, 'constructor_budget'),
        )
        for name, index, field in probes:
            with self.subTest(name=name, index=index, field=field):
                path = self.root/name
                original = path.read_bytes()
                try:
                    def change(records):
                        record = records[index]
                        if field == 'constructor_budget':
                            record['placement'][field] = 7
                        else:
                            record['rows'][0][field] = 'incompatible'
                    self.mutate(name, change)
                    retained = path.read_bytes()
                    with self.assertRaisesRegex(ValueError, 'incompatible|observation placement'):
                        analysis.analyze(self.root)
                    # Offline refusal must leave every original observation intact.
                    self.assertEqual(path.read_bytes(), retained)
                finally:
                    path.write_bytes(original)

    def test_consistent_changed_random_stream_cannot_be_compared_with_calibration(self):
        name = 'A1/hotspot-samples.json'
        def change(records):
            for record in records:
                for key in ('next_before', 'next_after'):
                    record['workload']['rng'][key] = [v+1 for v in record['workload']['rng'][key]]
        self.mutate(name, change)
        # This sequence is internally valid; only the calibration comparison
        # exposes its different input. Do not turn it into a service-time claim.
        with self.assertRaisesRegex(ValueError, 'input replay differs from calibration'):
            analysis.analyze(self.root)

    def test_exact_five_percent_drift_remains_within_limit(self):
        def change(records):
            for row in records:
                if row['case'] == 'encode:4:2:1024':
                    row['timed_ns'] *= 1.05; row['rows'][0]['ns_per_iter'] *= 1.05
        self.mutate('A2/small_file-samples.json', change)
        report = analysis.analyze(self.root)
        case = next(c for c in report['cases'] if c['case'] == 'encode:4:2:1024')
        self.assertEqual(case['repeat']['absolute_metric_drift'], .05)
        self.assertFalse(case['repeat']['exceeds_repeat_limit'])

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

    def service_case(self, run_values):
        path = self.root/'A1/small_file-samples.json'
        records = json.loads(path.read_text())
        case = 'encode:4:2:1024'
        for record in records:
            if record['case'] != case:
                continue
            index = record['index']
            timed = {3: 1_400_000_000, 4: 1_200_000_000}.get(index, 1_000_000_000)
            record['timed_ns'] = timed
            record['wall_ns'] = timed + 1_000
            record['rows'][0]['ns_per_iter'] = timed / record['iterations']
            run = run_values[index-3] if index >= 3 else timed
            record['after']['run_ns'] = record['before']['run_ns'] + run
            record['after']['wait_ns'] = record['before']['wait_ns'] + 100
            for key in ('process_minor_faults', 'process_major_faults'):
                record['after'][key] = record['before'][key]
        self.save('A1/small_file-samples.json', records)
        original = path.read_bytes()
        report = analysis.analyze(self.root)
        self.assertEqual(path.read_bytes(), original)
        result = next(c for c in report['cases'] if c['profile'] == 'small_file' and c['case'] == case)
        self.assertFalse(report['release_approval'])
        return result['phases']['A1']

    def test_service_transition_keeps_all_samples_and_the_original_failure(self):
        detail = self.service_case([1_400_000_000, 1_200_000_000] + [1_000_000_000]*7)
        self.assertEqual([r['index'] for r in detail['trace']], list(range(12)))
        self.assertAlmostEqual(detail['central_spread'], .2)
        service = detail['measured_service']
        self.assertAlmostEqual(service['main_run_central_spread'], .2)
        self.assertEqual(service['total_process_minor_faults'], 0)
        self.assertEqual(service['total_process_major_faults'], 0)
        self.assertLess(service['wall_minus_timed_fraction_range'][1], .000002)
        self.assertLess(detail['maximum_measured_main_wait_to_wall'], .000001)

    def test_service_distribution_uses_its_own_ranks_and_does_not_clamp(self):
        detail = self.service_case([1_000_000_000]*5 + [1_300_000_000, 900_000_000] + [1_000_000_000]*2)
        self.assertAlmostEqual(detail['central_spread'], .2)
        service = detail['measured_service']
        self.assertEqual(service['main_run_central_spread'], 0)
        self.assertGreater(service['main_run_to_wall_range'][1], 1)

    def test_zero_thread_runtime_keeps_unknown_spread_instead_of_dividing_by_zero(self):
        detail = self.service_case([0]*9)
        self.assertIsNone(detail['measured_service']['main_run_central_spread'])
        self.assertEqual(detail['measured_service']['main_run_to_wall_range'], [0, 0])

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
