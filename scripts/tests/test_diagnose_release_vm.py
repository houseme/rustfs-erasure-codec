import copy
import pathlib
import sys
import json
import tempfile
from unittest import mock
import unittest

sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_release_vm as diagnostic
import release_sampling as sampling
import release_evidence as evidence


def point(tick):
    return {'cpu':0,'run_ns':tick,'wait_ns':tick,'switches':tick, 'process_minor_faults':0, 'process_major_faults':0,'process':{
        'available':True,'complete':True,'minor_faults':tick,'major_faults':0,'snapshot_ns':10,
        'threads':[{'slot':0,'measuring_thread':True,'cpu':0,'minor_faults':tick,'major_faults':0,
                    'run_ns':tick,'wait_ns':tick,'timeslices':tick}]}}


class DiagnosticTests(unittest.TestCase):
    def records(self):
        return [{'kind':'sample','index':i,'case':'fixture','iterations':1,'timed_ns':100.,'wall_ns':1000,
                 'before':point(i),'after':point(i+1)} for i in range(1,10)]

    def test_diagnostic_observations_are_not_release_telemetry(self):
        record=self.records()[0]
        with self.assertRaises(ValueError):sampling.validate_telemetry(record)
        stripped=diagnostic.basic_records([record])[0]
        sampling.validate_telemetry(stripped)
        self.assertIn('process',record['before'])
        self.assertNotIn('process',stripped['before'])

    def test_diagnostic_manifest_cannot_enter_release_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);manifest=root/'diagnosis.json'
            manifest.write_text(json.dumps({'schema_version':'baseline-vm-diagnostic-v1','sources':{},'contexts':{},'files':{},'phase_order':list(evidence.PHASES)}))
            with mock.patch.object(evidence,'source_info') as source:
                with self.assertRaises(ValueError):evidence.verify(manifest,root)
                source.assert_not_called()

    def test_bounded_basic_copy_and_one_request_preparation_keep_raw_records_independent(self):
        rows = self.records()
        for row in rows: row['retained'] = {'bytes': [1, 2, 3]}
        original = copy.deepcopy(rows)
        with mock.patch.object(diagnostic.native_tools, '_request_payload', wraps=diagnostic.native_tools._request_payload) as frames:
            prepared = diagnostic.native_tools.request('vm-analysis', request=dict(
                operation='prepare', records=rows, profile='fixture'))
            self.assertEqual(frames.call_count, 1)
        self.assertEqual(prepared['analysis'], diagnostic.analyze(rows, 'fixture'))
        prepared['basic'][0]['retained']['bytes'][0] = 9
        self.assertEqual(rows, original)
        large = rows*15
        with mock.patch.object(diagnostic.native_tools, '_request_payload', wraps=diagnostic.native_tools._request_payload) as frames:
            copied = diagnostic.basic_records(large)
            self.assertEqual(frames.call_count, 3)
            self.assertTrue(all(len(json.loads(call.args[0])['request']['records']) <= 64
                                for call in frames.call_args_list))
        copied[0]['retained']['bytes'][0] = 9
        self.assertEqual(rows, original)

    def test_one_thread_reset_cannot_be_hidden_by_another_threads_progress(self):
        for field in ('run_ns', 'wait_ns', 'timeslices'):
            rows = self.records()
            for side, value in (('before', 1), ('after', 201)):
                thread = copy.deepcopy(rows[0][side]['process']['threads'][0])
                thread.update(slot=1, measuring_thread=False)
                thread[field] = value
                rows[0][side]['process']['threads'].append(thread)
            rows[0]['before']['process']['threads'][0][field] = 100
            diagnostic.basic_records(rows)
            with self.assertRaisesRegex(ValueError, 'thread counters reset'):
                diagnostic.analyze(rows, 'fixture')

class CaptureLifecycleTests(unittest.TestCase):
    def test_build_and_sampling_failures_restore_owned_source(self):
        import subprocess
        for stage in ('build', 'sample', 'interrupt'):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as directory:
                root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
                source={'commit':evidence.BASELINE_COMMIT}
                failure=KeyboardInterrupt() if stage=='interrupt' else ValueError('sampling failed')
                build_error=subprocess.CalledProcessError(1,['cargo']) if stage=='build' else None
                with mock.patch.object(evidence,'source_info',return_value=source), \
                     mock.patch.object(evidence,'environment_context',return_value={'isolation':{'cpu_affinity':[0]}}), \
                     mock.patch.object(sampling,'render',return_value={'root.rs':'// owned test\n','process.rs':'// owned module\n'}), \
                     mock.patch.object(diagnostic.subprocess,'run',side_effect=build_error), \
                     mock.patch.object(sampling,'run_profile',side_effect=failure):
                    with self.assertRaises((ValueError,subprocess.CalledProcessError,KeyboardInterrupt)):
                        diagnostic.capture(baseline,root/'out',root/'group')
                self.assertFalse((baseline/'tests/rse_release_sampling.rs').exists())
                self.assertFalse((baseline/'tests/rse_release_sampling').exists())
                self.assertTrue((root/'out/failure.json').is_file())
                self.assertFalse((root/'out/diagnosis.json').exists())


    def test_success_metadata_is_written_only_after_source_restoration(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
            output=root/'out';source={'commit':evidence.BASELINE_COMMIT}
            def source_info(path):
                if path==baseline:self.assertFalse((baseline/'tests/rse_release_sampling.rs').exists())
                return source
            with mock.patch.object(evidence,'source_info',side_effect=source_info), \
                 mock.patch.object(evidence,'environment_context',return_value={'isolation':{'cpu_affinity':[0]}}), \
                 mock.patch.object(sampling,'render',return_value={'root.rs':'// owned test\n'}), \
                 mock.patch.object(diagnostic.subprocess,'run'), \
                 mock.patch.object(evidence,'FILES',{}):
                diagnostic.capture(baseline,output,root/'group')
            self.assertTrue((output/'diagnosis.json').is_file())
            self.assertFalse((output/'failure.json').exists())

    def test_every_profile_updates_its_owned_partial_checkpoint(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
            records=DiagnosticTests().records()
            with mock.patch.object(evidence,'source_info',return_value={'commit':evidence.BASELINE_COMMIT}), \
                 mock.patch.object(evidence,'environment_context',return_value={'isolation':{'cpu_affinity':[0]}}), \
                 mock.patch.object(sampling,'render',return_value={'root.rs':'// owned test\n'}), \
                 mock.patch.object(diagnostic.subprocess,'run'), \
                 mock.patch.object(evidence,'FILES',{'first':'first.json','second':'second.json'}), \
                 mock.patch.object(evidence,'expected_cases',return_value={'fixture'}), \
                 mock.patch.object(evidence,'calibration_counts',return_value={'fixture':1}), \
                 mock.patch.object(sampling,'sample_groups'), \
                 mock.patch.object(sampling,'run_profile',return_value=records) as worker:
                diagnostic.capture(baseline,root/'out',root/'group')
            report=json.loads((root/'out/diagnosis.json').read_text())
            self.assertEqual(len(report['cases']),2);self.assertEqual(worker.call_count,4)
            self.assertFalse(report['release_approval']);self.assertFalse(report['formal_qualification'])
            self.assertTrue((root/'out/first-samples.json').is_file())
            self.assertTrue((root/'out/second-samples.json').is_file())
            self.assertFalse((root/'out/failure.json').exists())

    def test_native_capability_refusal_runs_no_codec_and_restores_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
            with mock.patch.object(evidence,'source_info',return_value={'commit':evidence.BASELINE_COMMIT}), \
                 mock.patch.object(evidence,'environment_context',return_value={}), \
                 mock.patch.object(diagnostic,'native_clock_render',return_value={'root.rs':'// owned test\n','native_service_clock.rs':'// native stub\n'}), \
                 mock.patch.object(diagnostic.subprocess,'run'), \
                 mock.patch.object(diagnostic,'probe_native_clock',side_effect=ValueError('capability refused')), \
                 mock.patch.object(sampling,'run_profile') as codec:
                with self.assertRaisesRegex(ValueError,'capability refused'):
                    diagnostic.capture(baseline,root/'out',root/'group',native_service_clock=True)
                codec.assert_not_called()
            self.assertFalse((baseline/'tests/rse_release_sampling.rs').exists())
            self.assertTrue((root/'out/failure.json').exists())
            self.assertFalse((root/'out/diagnosis.json').exists())

    def test_cleanup_refusal_never_publishes_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
            output=root/'out'
            def build(*args,**kwargs):
                (baseline/'tests/rse_release_sampling.rs').write_text('// foreign mutation\n')
            with mock.patch.object(evidence,'source_info',return_value={'commit':evidence.BASELINE_COMMIT}), \
                 mock.patch.object(evidence,'environment_context',return_value={}), \
                 mock.patch.object(sampling,'render',return_value={'root.rs':'// owned test\n'}), \
                 mock.patch.object(diagnostic.subprocess,'run',side_effect=build), \
                 mock.patch.object(evidence,'FILES',{}):
                with self.assertRaisesRegex(ValueError,'unexpected generated input'):
                    diagnostic.capture(baseline,output,root/'group')
            self.assertEqual((baseline/'tests/rse_release_sampling.rs').read_text(),'// foreign mutation\n')
            self.assertFalse((output/'diagnosis.json').exists())
            self.assertTrue((output/'failure.json').exists())


def native_point(start,end,offset=0,cpu=0):
    return {'schema':diagnostic.NATIVE_CLOCK_SCHEMA,'start_ns':start,'end_ns':end,
            'available':True,'cpu':cpu,'counters':{'tsc_before':100+offset,'tsc_after':101+offset,
                                               'aperf':200+offset,'mperf':300+offset}}


class NativeClockTests(unittest.TestCase):
    def record(self):
        return {'wall_ns':1000,'placement':{'cpu':0},
                'before':{'process':{'service_clock':native_point(0,10)}},
                'after':{'process':{'service_clock':native_point(1010,1020,2000)}}}

    def test_clock_overlay_does_not_change_any_formal_kernel_or_timer(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        ordinary=sampling.render(root);native=diagnostic.native_clock_render(root)
        for name in ordinary:
            if name not in ('root.rs','process.rs'):self.assertEqual(native[name],ordinary[name],name)
        self.assertTrue(native['root.rs'].startswith(ordinary['root.rs']))
        self.assertEqual(sampling.PROTOCOL['version'],15)
        self.assertIn('native_service_clock.rs',native)

    def test_probe_only_never_starts_calibration(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';(baseline/'tests').mkdir(parents=True)
            with mock.patch.object(evidence,'source_info',return_value={'commit':evidence.BASELINE_COMMIT}), \
                 mock.patch.object(evidence,'environment_context',return_value={}), \
                 mock.patch.object(diagnostic,'native_clock_render',return_value={'root.rs':'// owned test\n','native_service_clock.rs':'// native stub\n'}), \
                 mock.patch.object(diagnostic.subprocess,'run'), \
                 mock.patch.object(diagnostic,'probe_native_clock',return_value={'codec_calls':0}), \
                 mock.patch.object(sampling,'run_profile') as codec:
                diagnostic.capture(baseline,root/'out',root/'group',native_service_clock=True,probe_only=True)
                codec.assert_not_called()
            report=json.loads((root/'out/diagnosis.json').read_text())
            self.assertEqual(report['status'],'zero-codec-probe-complete; diagnostic only')
            self.assertEqual(report['cases'],[])

    def test_native_mode_requires_counters_on_every_original_observation(self):
        rows=DiagnosticTests().records()
        with self.assertRaises(ValueError):diagnostic.basic_records(rows,native_service_clock=True)
        with self.assertRaises(ValueError):diagnostic.analyze(rows,'fixture',native_service_clock=True)
        for index,row in enumerate(rows):
            start=index*3000;offset=index*8000;row['placement']={'cpu':0}
            row['before']['process']['service_clock']=native_point(start,start+10,offset)
            row['after']['process']['service_clock']=native_point(start+1010,start+1020,offset+2000)
        original=copy.deepcopy(rows)
        diagnostic.basic_records(rows,native_service_clock=True)
        result=diagnostic.analyze(rows,'fixture',native_service_clock=True)
        self.assertEqual(rows,original)
        self.assertTrue(all(s['native_service_clock']['interpretation_qualified'] for s in result[0]['samples']))

    def test_probe_requires_one_complete_line_including_final_newline(self):
        row=self.record();probe={'wall_ns':row['wall_ns'],'before':row['before']['process'],'after':row['after']['process']}
        line='RSE_NATIVE_CLOCK_PROBE '+json.dumps(probe)+'\n'
        self.assertTrue(diagnostic.parse_native_probe('test noise\n'+line+'ok\n',0)['interpretation_qualified'])
        for text in (line.rstrip(),line+line,line+'RSE_NATIVE_CLOCK_PROBE {', 'test name ... '+line):
            with self.subTest(text=text[-40:]),self.assertRaises(ValueError):
                diagnostic.parse_native_probe(text,0)

    def test_standalone_native_reader_regressions(self):
        import os,subprocess
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);wrapper=root/'native-tests.rs';binary=root/'native-tests'
            repo=pathlib.Path(__file__).resolve().parents[2]
            ordinary=sampling.render(repo);native=diagnostic.native_clock_render(repo)
            for name in ('process.rs','native_service_clock.rs'):(root/name).write_text(native[name])
            wrapper.write_text('#![allow(dead_code)]\n#[path="process.rs"] mod process;\n'+native['root.rs'][len(ordinary['root.rs']):])
            subprocess.run(['rustc','--edition=2024','--test','--crate-name','native_clock_tests',str(wrapper),'-o',str(binary)],
                           env=dict(os.environ,RUSTUP_TOOLCHAIN='stable'),check=True,capture_output=True,text=True)
            result=subprocess.run([str(binary),'--nocapture'],check=True,capture_output=True,text=True)
            self.assertIn('6 passed',result.stdout)
            self.assertIn('1 ignored',result.stdout)
            values=[json.loads(line[len('RSE_NATIVE_FIXTURE '):]) for line in result.stdout.splitlines() if line.startswith('RSE_NATIVE_FIXTURE ')]
            self.assertEqual(len(values),2)
            for value in values:
                self.assertEqual(value['marker'],19)
                diagnostic.native_tools.request('vm-analysis', request=dict(operation='point', point=value['service_clock']))
            self.assertEqual({v['service_clock']['available'] for v in values},{True,False})
            # Serial libtest writes its test name without a terminating newline.
            # Exercise the same reporter as the real probe, without MSR access.
            framed=subprocess.run([str(binary),'--exact','process::native_service_clock::tests::probe_record_under_libtest',
                                   '--nocapture','--test-threads=1'],check=True,capture_output=True,text=True)
            frames=[line for line in framed.stdout.splitlines(keepends=True) if line.startswith('RSE_NATIVE_CLOCK_PROBE ')]
            self.assertEqual(len(frames),1,framed.stdout)
            self.assertTrue(frames[0].endswith('\n'))
            self.assertEqual(json.loads(frames[0][len('RSE_NATIVE_CLOCK_PROBE '):]),
                             {'wall_ns':1000,'before':{'fixture':'before'},'after':{'fixture':'after'}})


if __name__=='__main__':unittest.main()
