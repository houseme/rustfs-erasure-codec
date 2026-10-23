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
        self.payload = {'case': 'synthetic-lifecycle-only', 'index': 0, 'complete': True}
        self.stack = ExitStack(); self.addCleanup(self.stack.close)
        def patch(target, **kwargs): return self.stack.enter_context(mock.patch(target, **kwargs))
        self.sources = {'baseline': {'commit':evidence.BASELINE_COMMIT, 'tree':'a'*40, 'lock_sha256':'a'*64},
                        'candidate': {'commit':'b'*40, 'tree':'b'*40, 'lock_sha256':'b'*64}}
        patch('release_evidence.source_info', side_effect=lambda root: self.sources['baseline' if root == self.baseline else 'candidate'])
        self.context = patch('release_evidence.environment_context', return_value={})
        patch('release_evidence.harness_digest', return_value='c'*64)
        patch('release_evidence.sampling.implementation_digest', return_value='d'*64)
        patch('release_evidence.sampling.overlay_digest', return_value='e'*64)
        patch('release_evidence.sampling.render', return_value={'root.rs': '// generated entry', 'stub.rs': '// generated module'})
        self.build = patch('release_evidence.subprocess.run')
        self.run = patch('release_evidence.sampling.run_profile', side_effect=self.emit)
        patch('release_evidence.calibration_counts', side_effect=lambda records, profile, revision: {case: 1 for case in evidence.expected_cases(profile)})
        self.real_observation_contracts = evidence.observation_contracts
        self.contracts = patch('release_evidence.observation_contracts', return_value={})
        self.select = patch('release_evidence.sampling.select_profile', return_value={'selected':[self.payload],'spreads':[]})
        patch('release_evidence.time.sleep')
        self.profiles = patch('release_evidence._verify_profiles', return_value=({}, {}, {'status': 'profiles_verified'}))
        self.layouts = patch('diagnose_verify_layout.capture_paired', side_effect=self.emit_layouts)
        self.verify = patch('release_evidence.verify', return_value=({}, {}, {'status': 'passed', 'test_only': True}))

    def emit_layouts(self, baseline, candidate, output, group, iterations):
        for root in (baseline, candidate): self.assert_removed(root)
        output.mkdir()
        (output/'manifest.json').write_text('{"synthetic-lifecycle-only": true}')

    def emit(self, root, env, profile, mode, path, count_file=None, **kwargs):
        path.write_text('RSE_TIMING '+json.dumps(self.payload)+'\nRSE_SAMPLE '+json.dumps(self.payload)+'\n')
        return [self.payload]

    def capture(self): return evidence.capture(self.baseline, self.candidate, self.output, self.root/'unused-group')

    def assert_removed(self, root):
        self.assertFalse((root/'tests/rse_release_sampling.rs').exists())
        self.assertFalse((root/'tests/rse_release_sampling').exists())

    def test_raw_contract_mismatch_stops_final_capture_before_later_phases(self):
        import test_release_evidence as fixtures
        fixture=fixtures.EvidenceTests(methodName='runTest');fixture.sources=self.sources
        self.contracts.side_effect=self.real_observation_contracts
        for stop in ('calibration-candidate','A1','B1'):
            self.output=self.root/('provenance-'+stop);self.run.reset_mock()
            def emit(root,env,profile,mode,path,count_file=None,**kwargs):
                role='baseline' if root==self.baseline else 'candidate'
                kinds=['calibration_warmup']*3+['calibration']*9 if mode=='calibrate' else ['warmup']*3+['sample']*9
                rows=[fixture.record(row,profile,role,kind,index) for row in fixture.fixture_rows('A1' if role=='baseline' else 'B1',profile) for index,kind in enumerate(kinds)]
                if mode=='calibrate':
                    for row in rows:row.update(complete=row['index']==11)
                if path.parent.name==stop and profile=='smoke':
                    # The full candidate calibration is internally consistent
                    # but incompatible with baseline; phase failures affect only
                    # one warmup, leaving the selected median unchanged.
                    changed=rows if mode=='calibrate' else rows[1:2]
                    for row in changed:row['rows'][0]['backend_id']='changed'
                path.write_text(''.join('RSE_SAMPLE '+json.dumps(row)+'\n' for row in rows))
                return rows
            self.run.side_effect=emit
            with self.assertRaisesRegex(ValueError,'incompatible backend_id'):self.capture()
            self.assertEqual(self.run.call_count,{'calibration-candidate':5,'A1':9,'B1':13}[stop])
            self.assertTrue((self.output/stop/'smoke-samples.json').exists())
            self.assertFalse((self.output/'report.json').exists())
            self.assertFalse((self.output/'manifest.json').exists())
            for root in (self.baseline,self.candidate):self.assert_removed(root)
            self.layouts.assert_not_called()

    def test_success_removes_both_byte_identical_overlays(self):
        self.capture()
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertTrue((self.output/'manifest.json').exists())
        self.assertEqual(self.run.call_count, 24)  # 8 calibration + 16 phase profiles.

    def test_layout_count_is_frozen_before_paired_execution_and_both_overlays_are_closed(self):
        def counts(records, profile, revision):
            return {case: (3 if case.endswith(':1024') else 7) if case in evidence.LAYOUT_CASES else 1 for case in evidence.expected_cases(profile)}
        with mock.patch.object(evidence, 'calibration_counts', side_effect=counts): self.capture()
        self.assertEqual(self.layouts.call_args.args[-1], 7)
        document = json.loads((self.output/'manifest.json').read_text())
        self.assertEqual(document['schema_version'], evidence.SCHEMA_VERSION)
        self.assertEqual(document['layout_supplement']['path'], 'layout-pair/manifest.json')

    def test_full_profile_failure_stops_before_any_layout_capture(self):
        self.profiles.side_effect = evidence.EvidenceFailure('ABBA drift', {})
        with self.assertRaisesRegex(ValueError, 'ABBA drift'): self.capture()
        self.layouts.assert_not_called()
        self.verify.assert_not_called()
        self.assertTrue((self.output/'A2/stream-samples.json').exists())
        self.assertFalse((self.output/'manifest.json').exists())
        self.assertFalse((self.output/'report.json').exists())

    def test_layout_failure_keeps_both_sets_of_rows_without_complete_manifest_or_success(self):
        def fail(*args):
            self.emit_layouts(*args)
            folder = args[2]/'A1'; folder.mkdir()
            (folder/'samples.partial.json').write_text(json.dumps([self.payload]))
            raise evidence.EvidenceFailure('layout quality', {})
        self.layouts.side_effect = fail
        with self.assertRaisesRegex(ValueError, 'layout quality'): self.capture()
        self.verify.assert_not_called()
        self.assertTrue((self.output/'A2/stream-samples.json').exists())
        self.assertEqual(json.loads((self.output/'layout-pair/A1/samples.partial.json').read_text()), [self.payload])
        self.assertFalse((self.output/'manifest.json').exists())
        self.assertFalse((self.output/'report.json').exists())

    def test_changed_context_after_layout_preserves_partial_data_without_success(self):
        def change(*args):
            self.emit_layouts(*args)
            self.context.return_value = {'changed': True}
        self.layouts.side_effect = change
        with self.assertRaisesRegex(ValueError, 'context changed'): self.capture()
        self.verify.assert_not_called()
        self.assertEqual(json.loads((self.output/'manifest.partial.json').read_text())['contexts']['after'], {'changed': True})
        self.assertFalse((self.output/'manifest.json').exists())
        self.assertFalse((self.output/'report.json').exists())

    def test_mismatched_tool_fails_before_builds_or_any_measurement(self):
        def source(path):
            if path == self.baseline: return self.sources['baseline']
            if path == self.candidate: return self.sources['candidate']
            return dict(self.sources['candidate'], tree='f'*40)
        with mock.patch.object(evidence, 'source_info', side_effect=source):
            with self.assertRaisesRegex(ValueError, 'tool must match'): self.capture()
        self.build.assert_not_called()
        self.run.assert_not_called()
        self.layouts.assert_not_called()
        self.assertFalse((self.output/'manifest.json').exists())

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

    def test_wide_first_phase_stops_before_formal_candidate_measurement(self):
        self.select.return_value = {'selected':[self.payload],'spreads':[{'case': 'fixture', 'central_spread': .2}]}
        with self.assertRaises(evidence.EvidenceFailure) as raised: self.capture()
        self.assertEqual(raised.exception.details['failed_phase'], 'A1')
        self.assertIsNone(raised.exception.details['candidate_attribution'])
        self.assertEqual(len(raised.exception.details['sample_spreads']), 4)
        self.assertEqual(self.run.call_count, 12)  # Both calibrations plus complete A1.
        for profile in evidence.FILES:
            self.assertTrue((self.output/'A1'/(profile+'-samples.json')).exists())
        self.assertFalse((self.output/'B1').exists())
        self.assertFalse((self.output/'manifest.json').exists())
        self.verify.assert_not_called()
        self.profiles.assert_not_called()
        self.layouts.assert_not_called()
        for root in (self.baseline, self.candidate): self.assert_removed(root)

    def test_final_verification_failure_preserves_complete_data_and_releases_overlays(self):
        self.verify.side_effect = evidence.EvidenceFailure('intentional invalid evidence', {})
        with self.assertRaises(evidence.EvidenceFailure): self.capture()
        for root in (self.baseline, self.candidate): self.assert_removed(root)
        self.assertTrue((self.output/'A2/stream-samples.json').exists())
        self.assertFalse((self.output/'report.json').exists())

    def test_cleanup_failure_cannot_leave_a_passing_report(self):
        def validated_but_changed(*_):
            (self.baseline/'tests/rse_release_sampling/stub.rs').write_text('changed source')
            return {}, {}, {'status': 'passed', 'test_only': True}
        self.profiles.side_effect = validated_but_changed
        with self.assertRaisesRegex(ValueError, 'preserve'): self.capture()
        self.assertFalse((self.output/'report.json').exists())
        self.assertEqual((self.baseline/'tests/rse_release_sampling/stub.rs').read_text(), 'changed source')
        self.assert_removed(self.candidate)

    def test_changed_generated_source_is_preserved(self):
        def fail(*args, **kwargs):
            self.emit(*args, **kwargs)
            (self.baseline/'tests/rse_release_sampling/stub.rs').write_text('changed source')
            raise subprocess.CalledProcessError(1, ['cargo'])
        self.run.side_effect = fail
        with self.assertRaisesRegex(ValueError, 'preserve'): self.capture()
        self.assertEqual((self.baseline/'tests/rse_release_sampling/stub.rs').read_text(), 'changed source')
        self.assert_removed(self.candidate)


class NativeControllerBindingsTests(unittest.TestCase):
    def test_source_overlays_prepare_once_and_restore_each_owned_input(self):
        import verify_scratch_audit as verify, stream_core_audit as stream, dynamic_buffer_audit as dynamic
        tools=verify.native_tools;tools._executable()
        for audit,names in ((verify,('src/lib.rs','src/core/verify.rs')),(stream,stream.NAMES),(dynamic,dynamic.NAMES)):
            with self.subTest(observer=audit.MODULE), tempfile.TemporaryDirectory() as folder:
                root=pathlib.Path(folder)
                original={n:(tools.ROOT/n).read_text() for n in names}
                for name,text in original.items():
                    path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
                with mock.patch.object(tools,'_run',wraps=tools._run) as calls:
                    with audit.source_overlay(root) as bindings:
                        self.assertEqual(calls.call_count,1)
                        for name,entry in bindings.items():
                            self.assertEqual(entry['diagnostic_sha256'],evidence.digest((root/name).read_text().encode()))
                self.assertEqual({n:(root/n).read_text() for n in names},original)
                self.assertFalse((root/('src/'+audit.MODULE+'.rs')).exists())

    def setUp(self):
        import native_tools
        self.native = native_tools
        self.native.close_worker()
        self.addCleanup(self.native.close_worker)

    def field(self, value):
        return self.native.request('provenance-field', row={'cpu_model':value}, field='cpu_model')

    def test_runtime_cache_revalidates_mutations_and_leaves_original_values_intact(self):
        from layout_fixture import kernel_policy_fixture
        value={'libc':{'name':'cache-fixture','version':'1'},'allocator_environment':{},
               'page_size':4096,'kernel_memory_policy':kernel_policy_fixture()}
        original=json.loads(json.dumps(value))
        self.native._validated_metadata.cache_clear()
        with mock.patch.object(self.native,'_request_payload',wraps=self.native._request_payload) as frames:
            self.native.validate_runtime(value);self.native.validate_runtime(value)
            self.assertEqual(frames.call_count,1)
            value['allocator_environment']['PATH']='not an allocator setting'
            with self.assertRaises(ValueError):self.native.validate_runtime(value)
            self.assertEqual(frames.call_count,2)
        value['allocator_environment'].clear()
        self.assertEqual(value,original)
        self.native._validated_metadata.cache_clear()
        value['allocator_environment']['LD_FIXTURE']='x'*65536
        with mock.patch.object(self.native,'_request_payload',wraps=self.native._request_payload) as frames:
            self.native.validate_runtime(value);self.native.validate_runtime(value)
        self.assertEqual(frames.call_count,2)
        self.assertEqual(self.native._validated_metadata.cache_info().currsize,0)

    def test_artifact_sized_requests_preserve_the_next_worker_frame(self):
        self.native.close_worker()
        value=self.native.request('provenance-field',row={'backend':'scalar','extra':'x'*(17*1024*1024)},field='backend')
        self.assertEqual(value,'scalar')
        worker=self.native._worker
        self.assertEqual(self.field('ordinary cpu'),'ordinary cpu')
        self.assertIs(self.native._worker,worker)

    def test_native_controller_builds_the_optimized_host_tool(self):
        host='aarch64-apple-darwin'
        artifact=dict(reason='compiler-artifact',target=dict(name='xtask'),
                      executable='/tool-target/'+host+'/release/xtask')
        with mock.patch.object(self.native,'_run',side_effect=[
                'rustc test\nhost: '+host+'\n',json.dumps(artifact)+'\n']) as run:
            executable=self.native._executable.__wrapped__()
        command=run.call_args_list[1].args[0]
        self.assertIn('--release',command)
        self.assertEqual(command[command.index('--target')+1],host)
        self.assertEqual(command[command.index('--target-dir')+1],str(self.native.ROOT/'target'))
        self.assertIn('--locked',command)
        self.assertIn('--offline',command)
        self.assertEqual(executable,artifact['executable'])

    def test_view_batches_reuse_validation_summaries_and_recheck_mutated_records(self):
        import stream_view_audit as views
        from layout_fixture import view_fixture
        case='encode_stream:4:2:65536:65536'
        data=[view_fixture(case,iterations=1,observation=i) for i in range(9)]
        source=[dict(len=e['len'],capacity=e['capacity'],offset_64=e['offsets'][0],
                     offset_4096=e['offsets'][1],offset_page=e['offsets'][2])
                for e in data[0]['events'] if e['call']==0]
        samples=[dict(case=case,index=i,fixture=dict(source=source)) for i in range(9)]
        with mock.patch.object(self.native,'request',wraps=self.native.request) as calls:
            reports=views.validate_samples(data,samples,1,4096,require_counters=True,summary=True)
        self.assertEqual(calls.call_count,3)
        self.assertEqual(len(reports),9)
        self.assertEqual(reports[0],views.summarize_windows(data[0]))
        child=self.native._worker
        data[-1]['events'][0]['capacity']=True
        with self.assertRaises(ValueError):views.validate_samples(data,samples,1,4096)
        self.assertIs(self.native._worker,child)

    def test_sampling_contract_cache_returns_private_copies_and_reuses_the_native_worker(self):
        self.field('sampling cache test')
        case='reconstruct_shard_slot:4:2:1024'
        first=self.native.sampling_contract('workload-contract','small_file',case)
        child=self.native._worker
        first['seed']=0
        self.assertEqual(self.native.sampling_contract('workload-contract','small_file',case)['seed'],65406575726428439)
        catalog=self.native.sampling_catalog();catalog['cases']['smoke'].clear()
        self.assertEqual(len(self.native.sampling_catalog()['cases']['smoke']),15)
        self.assertIs(self.native._worker,child)

    def test_controller_reuses_one_owned_worker_and_validation_errors_do_not_reset_it(self):
        self.assertEqual(self.field('CPU one'),'CPU one')
        child=self.native._worker
        with self.assertRaises(ValueError):
            self.native.request('benchmark-rows',rows=[],source='fixture')
        self.assertIs(self.native._worker,child)
        self.assertEqual(self.field('CPU two'),'CPU two')
        self.native.close_worker()
        self.assertEqual(child.returncode,0)

    def test_concurrent_calls_do_not_exchange_each_others_responses(self):
        from concurrent.futures import ThreadPoolExecutor
        expected=['CPU '+str(i) for i in range(40)]
        with ThreadPoolExecutor(max_workers=4) as pool:
            self.assertEqual(list(pool.map(self.field,expected)),expected)
        self.assertIsNone(self.native._worker.poll())

    def test_dead_worker_fails_the_current_call_without_replay_and_next_call_can_restart(self):
        self.field('before')
        child=self.native._worker
        child.terminate();child.wait(timeout=5)
        with self.assertRaises((OSError,ValueError)):
            self.field('interrupted')
        self.assertIsNone(self.native._worker)
        self.assertEqual(self.field('after'),'after')
        self.assertIsNot(self.native._worker,child)

    def test_normalization_cache_is_immutable_and_provenance_alias_mutations_are_rechecked(self):
        row={'operation':'encode','data_shards':4,'parity_shards':2,'shard_size':1024,
             'throughput_mb_s':2866592468.8449306,'ns_per_iter':8.344125545873929e-19,'backend':'scalar'}
        result=next(iter(self.native.index_records([row]).values()))
        self.assertEqual(result['throughput_mb_s'],row['throughput_mb_s'])
        self.assertEqual(result['ns_per_iter'],row['ns_per_iter'])
        result['backend']='changed'
        self.assertEqual(next(iter(self.native.index_records([row]).values()))['backend'],'scalar')
        self.assertEqual(self.native.provenance_value(row,'backend'),'scalar')
        row['backend_name']='rust-neon'
        with self.assertRaisesRegex(ValueError,'conflicting aliases'):
            self.native.provenance_value(row,'backend')

    def test_observation_batches_keep_references_and_validate_across_frame_boundaries(self):
        rows=[dict(case='a',kind='sample',index=i,rows=[{'backend':'scalar'}],placement={'cpu':2}) for i in range(130)]
        with mock.patch.object(self.native,'request',wraps=self.native.request) as calls:
            reference=self.native.observation_contracts('smoke',rows)
        self.assertEqual(calls.call_count,3)
        self.assertIs(reference['a'][0],rows[0]['rows'][0])
        self.assertIs(reference['a'][1],rows[0]['placement'])
        self.assertEqual(self.native.observation_contracts('smoke',rows,reference),reference)
        rows[-1]['rows'][0]['backend_id']='changed'
        with self.assertRaisesRegex(ValueError,'a sample\\[129\\].*backend_id'):
            self.native.observation_contracts('smoke',rows,reference)

    def test_live_capture_and_file_adapters_use_fresh_caller_context(self):
        self.field('pure worker')
        child=self.native._worker
        self.native._executable()
        with mock.patch.object(self.native,'_run',side_effect=['{"process_thp_disable":0}','{"process_thp_disable":3}','"source"']) as fresh:
            self.assertEqual(self.native.capture_memory_policy()['process_thp_disable'],0)
            self.assertEqual(self.native.capture_memory_policy()['process_thp_disable'],3)
            self.assertEqual(self.native.native_template('views'),'source')
        self.assertEqual(fresh.call_count,3)
        self.assertIs(self.native._worker,child)
        self.assertIsNone(child.poll())


if __name__ == '__main__': unittest.main()
