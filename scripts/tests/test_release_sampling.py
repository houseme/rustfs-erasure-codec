"""Duration protocol fixtures test the contract, not actual performance."""
import copy
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as evidence
import release_sampling as sampling
import test_release_evidence as fixtures


class SamplingContractTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.EvidenceTests(); self.fixture.setUp(); self.addCleanup(self.fixture.doCleanups)
        self.root=self.fixture.root; self.doc=self.fixture.doc

    def cases(self,phase,profile):
        return self.fixture.fixture_rows(phase,profile)

    def record(self,row,profile,role,kind,index):
        return self.fixture.record(row,profile,role,kind,index)

    def write(self,folder,profile,records):
        return self.fixture.write_samples(folder,profile,records)

    def verify(self):
        self.fixture.manifest.write_text(json.dumps(self.doc))
        return evidence.verify(self.fixture.manifest,self.root)

    def mutate(self,change,phase='B2',profile='small_file'):
        path=self.root/self.doc['files'][phase][profile]['samples']['path']
        records=json.loads(path.read_text());change(records)
        self.doc['files'][phase][profile]['samples']=self.write(phase,profile,records)

    def test_process_fault_counters_are_required_nonnegative_and_monotonic(self):
        row = self.record(self.cases('A1', 'smoke')[0], 'smoke', 'baseline', 'sample', 3)
        sampling.validate_telemetry(row)
        for field in ('process_minor_faults', 'process_major_faults'):
            for value in (None, True, -1):
                changed = copy.deepcopy(row)
                if value is None: changed['after'].pop(field)
                else: changed['after'][field] = value
                with self.assertRaises(ValueError): sampling.validate_telemetry(changed)
            changed = copy.deepcopy(row)
            changed['before'][field] = 2; changed['after'][field] = 1
            with self.assertRaisesRegex(ValueError, 'backwards'): sampling.validate_telemetry(changed)

    def test_every_retained_caller_case_requires_its_declared_api_contract(self):
        covered = 0
        for profile in ('small_file', 'stream'):
            for row in self.cases('A1', profile):
                record = self.record(row, profile, 'baseline', 'sample', 3)
                sampling.validate_fixture(record, profile, 4096)
                covered += 1
                for field, value in (('call_entry', 'slice'), ('output_policy', 'reuse-everything'),
                                     ('missing_mask', -1), ('required_mask', True)):
                    changed = copy.deepcopy(record); changed['fixture'][field] = value
                    with self.assertRaises(ValueError): sampling.validate_fixture(changed, profile, 4096)
                changed = dict(record, fixture=None)
                with self.assertRaises(ValueError): sampling.validate_fixture(changed, profile, 4096)
        self.assertEqual(covered, 103)

    def test_new_source_layouts_cannot_change_between_calibration_and_measurement(self):
        original = copy.deepcopy(self.doc)
        paths = [self.root/self.doc['files']['B1'][p]['samples']['path'] for p in ('small_file', 'stream')]
        originals = {path: path.read_bytes() for path in paths}
        for profile, key in (('small_file', 'reconstruct_opt:4:2:524288'),
                             ('stream', 'reconstruct_stream:4:2:65536:65536')):
            self.doc = copy.deepcopy(original)
            for path, data in originals.items(): path.write_bytes(data)
            path = self.root/self.doc['files']['B1'][profile]['samples']['path']
            records = json.loads(path.read_text())
            for record in records:
                if record['case'] == key:
                    record['fixture']['source'][0].update(offset_64=32, offset_4096=32, offset_page=32)
            self.doc['files']['B1'][profile]['samples'] = self.write('B1', profile, records)
            with self.assertRaisesRegex(ValueError, 'final calibration'): self.verify()

    def test_shared_kernels_keep_preparation_outside_the_timed_loops(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        rendered = sampling.render(root)
        for profile in ('small_file', 'stream'):
            kernel = sampling.block(rendered[profile+'.rs'], 'fn execute_release_kernel')
            for preparation in ('ReedSolomon::new', 'make_full_shards', 'make_data_shards', 'make_encoded_shards'):
                self.assertNotIn(preparation, kernel)
            self.assertIn('.cloned()', kernel)
        self.assertIn('Vec<Option<Vec<u8>>>', rendered['small_file.rs'])
        self.assertIn('reserved_stream_outputs(case.parity_shards, case.shard_size)', rendered['stream.rs'])

    def test_hotspot_deferred_result_metadata_uses_no_owned_strings(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        rendered = sampling.render(root)
        result = sampling.block(rendered['hotspot.rs'], 'struct ReconstructionHotspotBenchResult')
        self.assertNotIn(': String,', result)
        self.assertEqual(result.count('super::pattern::ShardPattern'), 2)
        pattern = sampling.block(rendered['hotspot.rs'], 'fn shard_index_pattern')
        self.assertNotIn('format!', pattern)
        self.assertNotIn('Vec::', pattern)
        self.assertIn('mod pattern;', rendered['root.rs'])
        self.assertEqual(sampling.PROTOCOL['result_metadata_policy'], 'bounded-inline-hotspot-patterns-deferred-formatting')

    def test_smoke_and_hotspot_cannot_omit_the_execution_contract(self):
        for profile in ('smoke','hotspot'):
            original=copy.deepcopy(self.doc)
            path=self.root/self.doc['files']['B2'][profile]['samples']['path']
            previous=path.read_bytes()
            self.mutate(lambda rows:rows[0].pop('workload'), profile=profile)
            with self.assertRaisesRegex(ValueError,'workload contract'):self.verify()
            path.write_bytes(previous);self.doc=original

    def test_consistent_candidate_random_stream_change_is_not_same_input(self):
        case=sampling.HOTSPOT_CASES[0][2]
        for folder in ('B1','B2','calibration-candidate'):
            entry=self.doc['calibration']['candidate']['hotspot'] if folder.startswith('calibration') else self.doc['files'][folder]['hotspot']['samples']
            rows=json.loads((self.root/entry['path']).read_text())
            for row in rows:
                if row['case']==case:
                    for key in ('next_before','next_after'):row['workload']['rng'][key][0]^=1
            changed=self.write(folder,'hotspot',rows)
            if folder.startswith('calibration'):self.doc['calibration']['candidate']['hotspot']=changed
            else:self.doc['files'][folder]['hotspot']['samples']=changed
        with self.assertRaisesRegex(ValueError,'input differs between ABBA'):self.verify()

    def test_same_initial_preview_cannot_hide_changed_later_input(self):
        def change(rows):
            for row in rows:
                if row['case']==sampling.HOTSPOT_CASES[0][2]:
                    if row['index']>0:row['workload']['rng']['next_before'][0]^=1
                    row['workload']['rng']['next_after'][0]^=1
        self.mutate(change,profile='hotspot')
        with self.assertRaisesRegex(ValueError,'random input differs from final calibration'):self.verify()

    def test_placement_budget_cannot_change_between_calibration_and_measurement(self):
        def change(records):
            for row in records:row['placement']['constructor_budget']=4
        self.mutate(change,profile='smoke')
        with self.assertRaisesRegex(ValueError,'preparation or initial input state'):self.verify()

    def test_base_page_size_is_required_and_bound_to_runtime(self):
        for value in (None, True, 0, 3000, 2**30):
            self.assertFalse(sampling.valid_page_size(value))
        self.assertTrue(sampling.valid_page_size(4096))
        self.assertTrue(sampling.valid_page_size(16384))
        original=copy.deepcopy(self.doc)
        for value in (None, True, 3000, 16384):
            self.doc=copy.deepcopy(original)
            def change(records):
                for r in records:
                    if r['case'].startswith('verify_with_buffer:'):r['fixture']['page_size']=value
            self.mutate(change)
            with self.assertRaises(ValueError):self.verify()

    def test_page_offsets_and_runtime_page_changes_are_rejected(self):
        self.mutate(lambda records:next(r for r in records if r['case'].startswith('verify_with_buffer:'))['fixture']['source'][0].update(offset_page=4096))
        with self.assertRaises(ValueError):self.verify()
        self.fixture.doc=self.doc
        self.doc['contexts']['B1']['runtime']['page_size']=16384
        with self.assertRaises(ValueError):self.verify()

    def test_consistent_runtime_page_change_cannot_hide_mismatched_fixture_pages(self):
        for context in self.doc['contexts'].values():context['runtime']['page_size']=16384
        with self.assertRaisesRegex(ValueError,'page size differs'):self.verify()

    def test_interrupted_sample_journal_preserves_completed_records(self):
        row={'case':'fixture'}
        text='log\nRSE_SAMPLE '+json.dumps(row)+'\nRSE_SAMPLE {"case":'
        self.assertEqual(sampling.parse_output(text),[row])
        with self.assertRaises(json.JSONDecodeError):sampling.parse_output(text+'\n')

    def test_missing_invalid_and_replaced_vec_layouts_are_rejected(self):
        original = copy.deepcopy(self.doc)
        path = self.root/self.doc['files']['B2']['small_file']['samples']['path']
        records = json.loads(path.read_text())
        changes = [lambda r:r.pop('fixture'), lambda r:r.update(fixture=None),
                   lambda r:r['fixture'].update(entry='slice'),
                   lambda r:r['fixture'].update(lifetime='observation'),
                   lambda r:r['fixture']['source'].pop(),
                   lambda r:r['fixture']['scratch'][0].update(offset_64=True),
                   lambda r:r['fixture']['scratch'][0].update(offset_4096=4096),
                   lambda r:r['fixture']['scratch'][0].update(offset_4096=32),
                   lambda r:r['fixture']['scratch'][0].update(capacity=0)]
        for change in changes:
            self.doc = copy.deepcopy(original)
            updated = copy.deepcopy(records)
            record = next(r for r in updated if r['case'].startswith('verify_with_buffer:'))
            change(record)
            self.doc['files']['B2']['small_file']['samples'] = self.write('B2','small_file',updated)
            with self.assertRaises(ValueError): self.verify()

    def test_fixture_changes_within_warmup_or_samples_reject(self):
        def change(records):
            record = next(r for r in records if r['case'].startswith('verify_with_buffer:'))
            record['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
        self.mutate(change)
        with self.assertRaisesRegex(ValueError,'inside a measurement worker'): self.verify()

    def test_fixed_layout_must_match_final_calibration(self):
        def change(records):
            for r in records:
                if r['case'].startswith('verify_with_buffer:'):
                    r['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
        self.mutate(change)
        with self.assertRaisesRegex(ValueError,'final calibration'): self.verify()

    def test_both_candidate_phases_cannot_use_a_different_layout(self):
        for folder in ('B1','B2','calibration-candidate'):
            profile='small_file'
            entry = self.doc['calibration']['candidate'][profile] if folder.startswith('calibration') else self.doc['files'][folder][profile]['samples']
            records=json.loads((self.root/entry['path']).read_text())
            for r in records:
                if r['case'].startswith('verify_with_buffer:'):
                    r['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
            entry=self.write(folder,profile,records)
            if folder.startswith('calibration'): self.doc['calibration']['candidate'][profile]=entry
            else: self.doc['files'][folder][profile]['samples']=entry
        with self.assertRaisesRegex(ValueError,'between ABBA phases'): self.verify()

    def test_calibration_layout_cannot_change_during_rehearsal(self):
        path=self.root/self.doc['calibration']['baseline']['small_file']['path']
        records=json.loads(path.read_text())
        record=next(r for r in records if r['case'].startswith('verify_with_buffer:'))
        record['fixture']['scratch'][0].update(offset_64=32,offset_4096=32,offset_page=32)
        with self.assertRaisesRegex(ValueError,'inside a calibration worker'):
            evidence.calibration_counts(records,'small_file',self.sources_commit('baseline'))

    def test_stream_fixture_correction_is_explicit_and_shared(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        source=(root/'tests/benchmark_stream_paths.rs').read_text()
        updated=sampling.reserve_stream_outputs(source)
        self.assertNotEqual(updated,source)
        old='let mut writers = vec![Vec::with_capacity(case.shard_size); case.parity_shards];'
        new='let mut writers = reserved_stream_outputs(case.parity_shards, case.shard_size);'
        self.assertEqual(updated.replace(new,old),source)
        with self.assertRaises(ValueError):sampling.reserve_stream_outputs(updated)

    def test_timing_adapter_preserves_every_operation_statement(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        for filename,name,count in (("benchmark_small_files.rs","fn run_operation",11),("benchmark_stream_paths.rs","fn run_stream_operation",3)):
            original=(root/"tests"/filename).read_text()
            adapted=sampling.exclude_once_only_setup(original,name,count)
            self.assertEqual(adapted.count("measured += start.elapsed();"),count)
            with self.assertRaises(ValueError): sampling.exclude_once_only_setup(original,name,count+1)

    def test_complete_new_protocol_replays_every_sample_and_existing_gates(self):
        with mock.patch.object(evidence.subprocess,'run') as gates:
            report=self.verify()[2]
        self.assertEqual(gates.call_count,16)
        self.assertEqual(report['sample_distribution_checks'],504)
        self.assertEqual(report['drift_checks'],252)

    def test_missing_duplicate_reordered_samples_and_short_intervals_reject(self):
        original=copy.deepcopy(self.doc)
        profile='small_file';phase='B2'
        path=self.root/self.doc['files'][phase][profile]['samples']['path'];records=json.loads(path.read_text())
        first = sampling.PROTOCOL['warmups']
        mutations=[lambda r:r.pop(1),lambda r:r.insert(1,copy.deepcopy(r[1])),
                   lambda r:r[first].update(index=2),lambda r:r[first].update(wall_ns=1),
                   lambda r:r[first].update(timed_ns=1),lambda r:r[first].update(iterations=2),
                   lambda r:r[first]['after'].update(cpu=20),lambda r:r[first]['after'].update(run_ns=-1),
                   lambda r:r[first].update(isolated_case=False),lambda r:r[first].update(calibration_batch=False),
                   lambda r:r[1].update(index=True)]
        for change in mutations:
            self.doc=copy.deepcopy(original);updated=copy.deepcopy(records);change(updated)
            self.doc['files'][phase][profile]['samples']=self.write(phase,profile,updated)
            with self.assertRaises((ValueError,KeyError)):self.verify()

    def test_summary_must_be_actual_median_and_not_selected_best(self):
        rows=self.cases('B2','small_file');rows[0]['ns_per_iter']*=.9
        entry=self.doc['files']['B2']['small_file']['samples']
        self.fixture.doc=self.doc;self.fixture.write_rows('B2','small_file',rows)
        self.doc['files']['B2']['small_file']['samples']=entry
        with self.assertRaisesRegex(ValueError,'median'):self.verify()

    def test_broad_distribution_cannot_be_hidden_by_unchanged_median(self):
        def change(records):
            for index in (sampling.PROTOCOL['warmups'],sampling.PROTOCOL['warmups']+1):
                records[index]['rows'][0]['ns_per_iter']*=.8;records[index]['timed_ns']*=.8
            for index in (sampling.PROTOCOL['warmups']+7,sampling.PROTOCOL['warmups']+8):
                records[index]['rows'][0]['ns_per_iter']*=1.2;records[index]['timed_ns']*=1.2
        self.mutate(change)
        with self.assertRaisesRegex(ValueError,'distributions'):self.verify()

    def test_incomplete_rehearsal_does_not_freeze_calibration(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text())
        records.pop(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']-1)
        with self.assertRaisesRegex(ValueError,'incomplete'):
            evidence.calibration_counts(records,'smoke',self.sources_commit('baseline'))

    def test_late_short_probe_grows_a_fresh_complete_rehearsal(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text());first=records[0];case=first['case']
        replacement=[]
        width=sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']
        for batch,count in ((1,1),(2,3)):
            for index in range(width):
                row=copy.deepcopy(first)
                warming=index<sampling.PROTOCOL['warmups']
                ns=102000000 if batch==1 else 42000000
                wall=106000000 if batch==1 else 126000000
                if batch==1 and index==width-1:ns=42000000;wall=42000000
                row.update(index=index,kind='calibration_warmup' if warming else 'calibration',iterations=count,calibration_batch=batch,timed_ns=ns*count,wall_ns=wall,target_met=wall>=sampling.PROTOCOL['target_wall_ns'] and ns*count>=sampling.PROTOCOL['target_timed_ns'],complete=batch==2 and index==width-1)
                row['rows'][0].update(iterations=count,ns_per_iter=ns)
                replacement.append(row)
        records=replacement+[r for r in records if r['case']!=case]
        counts=evidence.calibration_counts(records,'smoke',self.sources_commit('baseline'))
        self.assertEqual(counts[case],3)

    def test_calibration_must_warm_before_rehearsal_and_finish_every_probe(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text())
        early=copy.deepcopy(records);early[0]['kind']='calibration'
        with self.assertRaisesRegex(ValueError,'warmup'):
            evidence.calibration_counts(early,'smoke',self.sources_commit('baseline'))
        late=copy.deepcopy(records)
        last=sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']-1
        late[last].update(wall_ns=40000000,target_met=False)
        with self.assertRaisesRegex(ValueError,'rehearsal'):
            evidence.calibration_counts(late,'smoke',self.sources_commit('baseline'))

    def sources_commit(self,role):
        return self.doc['sources'][role]['commit']

    def test_process_runner_isolates_each_case_and_rejects_wrong_workload(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory)/'runner.log'
            counts=pathlib.Path(directory)/'counts.tsv';counts.write_text('a\t1\nb\t1\n')
            observed = []
            def emit(command, **kwargs):
                case = kwargs['env']['RSE_SAMPLER_CASE']
                observed.append(case)
                kwargs['stdout'].write('RSE_SAMPLE '+json.dumps({'case':case,'isolated_case':True})+'\n')
                kwargs['stdout'].flush()
            with mock.patch('subprocess.run',side_effect=emit):
                rows=sampling.run_profile(self.root,{},'smoke','sample',path,counts,cases={'b','a'})
            self.assertEqual(observed,['a','b'])
            self.assertEqual([r['case'] for r in rows],observed)
            def wrong_case(command, **kwargs):
                kwargs['stdout'].write('RSE_SAMPLE {"case":"wrong","isolated_case":true}\n')
                kwargs['stdout'].flush()
            with mock.patch('subprocess.run',side_effect=wrong_case):
                with self.assertRaisesRegex(ValueError,'selected workload'):
                    sampling.run_profile(self.root,{},'smoke','sample',path,counts,cases={'a','b'})

    def test_controller_rejects_bad_frozen_counts_before_launching_workers(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);counts=root/'counts.tsv'
            for content in ('a\t1\na\t2\n','b\t1\n','a\t0\n'):
                counts.write_text(content)
                with mock.patch('subprocess.run') as workers:
                    with self.assertRaises(ValueError):
                        sampling.run_profile(self.root,{},'smoke','sample',root/'output.log',counts,cases={'a'})
                    workers.assert_not_called()

    def test_calibration_controller_uses_a_new_worker_for_each_proposed_count(self):
        with tempfile.TemporaryDirectory() as directory:
            path=pathlib.Path(directory)/'calibration.log';observed=[]
            def emit(command, **kwargs):
                count=int(kwargs['env']['RSE_SAMPLER_ITERATIONS']);batch=int(kwargs['env']['RSE_SAMPLER_BATCH'])
                observed.append((count,batch))
                width=sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']
                for index in range(width):
                    warming=index<sampling.PROTOCOL['warmups'];met=not(batch==1 and index==width-1)
                    row={'case':'a','isolated_case':True,'kind':'calibration_warmup' if warming else 'calibration','iterations':count,'calibration_batch':batch,'index':index,'wall_ns':120000000 if met else 42000000,'timed_ns':10000000,'target_met':met,'complete':batch==2 and index==width-1}
                    kwargs['stdout'].write('RSE_SAMPLE '+json.dumps(row)+'\n')
                kwargs['stdout'].flush()
            with mock.patch('subprocess.run',side_effect=emit):
                rows=sampling.run_profile(self.root,{},'smoke','calibrate',path,cases={'a'})
            self.assertEqual(observed,[(1,1),(3,2)])
            self.assertEqual(len(rows),2*(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']))

    def test_calibration_budget_failure_preserves_every_observation(self):
        with tempfile.TemporaryDirectory() as directory:
            path=pathlib.Path(directory)/'failed.log'
            def emit(command, **kwargs):
                count=int(kwargs['env']['RSE_SAMPLER_ITERATIONS']);batch=int(kwargs['env']['RSE_SAMPLER_BATCH'])
                for index in range(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']):
                    row={'case':'a','isolated_case':True,'kind':'calibration_warmup' if index<sampling.PROTOCOL['warmups'] else 'calibration','iterations':count,'calibration_batch':batch,'index':index,'wall_ns':40000000,'timed_ns':10000000,'target_met':False,'complete':False}
                    kwargs['stdout'].write('RSE_SAMPLE '+json.dumps(row)+'\n')
                kwargs['stdout'].flush()
            with mock.patch('subprocess.run',side_effect=emit) as workers:
                with self.assertRaisesRegex(ValueError,'batch budget'):
                    sampling.run_profile(self.root,{},'smoke','calibrate',path,cases={'a'})
            self.assertEqual(workers.call_count,sampling.PROTOCOL['max_calibration_batches'])
            self.assertEqual(len(sampling.parse_log(path)),sampling.PROTOCOL['max_calibration_records'])

    def test_missing_fixed_warmup_cannot_be_replaced_by_a_sample(self):
        self.mutate(lambda records: records[0].update(kind='sample'))
        with self.assertRaisesRegex(ValueError,'warmup'):
            self.verify()

    def test_same_process_calibration_cannot_enter_release_validation(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text());records[0]['isolated_case']=False
        with self.assertRaisesRegex(ValueError,'calibration'):
            evidence.calibration_counts(records,'smoke',self.sources_commit('baseline'))

    def test_calibration_and_protocol_cannot_be_changed(self):
        original=copy.deepcopy(self.doc)
        for field in ('samples','warmups','case_isolation','case_order','max_calibration_batches','target_timed_ns','central_spread_limit','implementation_sha256','overlay_sha256','fixture_layout','stream_output_policy','result_metadata_policy'):
            self.doc=copy.deepcopy(original);self.doc['sampling'][field]='invalid'
            with self.assertRaises(ValueError):self.verify()
        self.doc=copy.deepcopy(original)
        key=next(iter(self.doc['sampling']['common_iterations']['small_file']))
        self.doc['sampling']['common_iterations']['small_file'][key]=2
        with self.assertRaises(ValueError):self.verify()


if __name__=='__main__':unittest.main()
