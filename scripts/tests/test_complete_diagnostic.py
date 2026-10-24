"""Complete synthetic failures exercise collection, never performance acceptance."""
import copy
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as e
import release_sampling as s
import diagnose_verify_layout as layout
import test_release_evidence as fixtures
import test_capture_lifecycle as lifecycle
from layout_fixture import BASELINE, CANDIDATE, paired_records, write_phase


class CompleteReplayTests(unittest.TestCase):
    def setUp(self):
        self.f = fixtures.EvidenceTests(methodName='runTest'); self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.root = self.f.root; self.doc = self.f.doc
        self.manifest = self.root/'diagnostic-manifest.json'
        self.doc.update(schema_version=e.DIAGNOSTIC_SCHEMA, diagnostic_only=True, release_approval=False)
        folder = self.root/e.LAYOUT_DIRECTORY
        old = folder/'manifest.json'; document = json.loads(old.read_text())
        document['diagnostic_only'] = True
        self.layout_manifest = folder/'diagnostic-manifest.json'
        self.layout_manifest.write_text(json.dumps(document)); old.unlink()
        self.doc['layout_supplement'] = {'path': e.LAYOUT_DIRECTORY+'/diagnostic-manifest.json',
                                         'sha256': e.digest(self.layout_manifest.read_bytes())}

    def replay(self):
        events = []
        for phase in ('calibration-baseline','calibration-candidate',*e.PHASES):
            calibration=phase.startswith('calibration-')
            for profile in e.FILES:
                entry=(self.doc['calibration'][phase.removeprefix('calibration-')][profile] if calibration
                       else self.doc['files'][phase][profile]['samples'])
                for r in json.loads((self.root/entry['path']).read_text()):
                    if r['index'] != 0:continue
                    base={'phase':phase,'profile':profile,'mode':'calibrate' if calibration else 'sample',
                          'case':r['case'],'iterations':r['iterations'],'calibration_batch':r['calibration_batch']}
                    for event in ('start','end'):
                        row=dict(base,event=event,monotonic_ns=len(events)+1,utc_ns=len(events)+1)
                        if event=='end':row['exit_code']=0
                        events.append(row)
        journal=self.root/'worker-journal.jsonl';journal.write_text(''.join(json.dumps(r)+'\n' for r in events))
        self.doc['worker_journal']={'path':journal.name,'sha256':e.digest(journal.read_bytes())}
        self.manifest.write_text(json.dumps(self.doc))
        return e.verify_diagnostic(self.manifest, self.root)[2]

    def mutate(self, phase, profile, change):
        records = json.loads((self.root/self.doc['files'][phase][profile]['samples']['path']).read_text())
        change(records)
        self.doc['files'][phase][profile]['samples'] = self.f.write_samples(phase, profile, records)
        counts = self.doc['sampling']['common_iterations'][profile]
        revision = self.doc['sources']['baseline' if phase.startswith('A') else 'candidate']['commit']
        selection = e.diagnostic_rows(records, profile, e.METRICS[profile], revision,
                                         e.expected_cases(profile), counts)
        self.f.write_rows(phase, profile, selection['selected'])

    def test_worker_journal_reuses_each_bound_profile_read(self):
        reader=e.member_bytes;native=e.native_tools
        def read(path,*args,**kwargs):
            if path.name.endswith('-samples.json') or path.name in e.FILES.values():raise AssertionError('Python campaign member read')
            return reader(path,*args,**kwargs)
        with mock.patch.object(e,'member_bytes',side_effect=read), mock.patch.object(e,'validate_runtime',side_effect=AssertionError('Python runtime prequery')), mock.patch.object(native,'_request_payload',wraps=native._request_payload) as frames, mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')):
            report=self.replay()
        self.assertEqual(report['worker_invocations'],756)
        requests=[c.args[0] for c in frames.call_args_list if json.loads(c.args[0]).get('kind')=='sampling' and json.loads(c.args[0]).get('request',{}).get('operation')=='campaign']
        self.assertEqual(len(requests),1)
        original_bytes=sum(p.stat().st_size for p in self.root.rglob('*-samples.json'))
        self.assertLess(len(requests[0].encode()),original_bytes//100)
        self.assertNotIn('summarized',json.loads(requests[0])['request']['request'])
        self.assertIsInstance(json.loads(requests[0])['request']['request']['bindings'],dict)
        self.assertEqual(json.loads(requests[0])['request']['request']['bindings']['compiler'],'rustc fixture')

    def test_execution_errors_do_not_become_complete_statistical_diagnostics(self):
        with mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],2,'','input integrity failed')) as checks:
            with self.assertRaisesRegex(ValueError,'regression validation failed'):self.replay()
        self.assertEqual(checks.call_count,1)

    def test_wide_short_repeat_and_all_regression_failures_are_reported_together(self):
        def wide(records):
            for r in records:
                if r['case']=='encode:4:2:1024' and r['index']>=9:
                    r['rows'][0]['ns_per_iter']*=2; r['timed_ns']*=2
        self.mutate('A1','small_file',wide)
        self.mutate('B1','stream',lambda rows: rows[-1].update(wall_ns=500_000_000,complete=False,target_met=False))
        def drift(records):
            for r in records:
                if r['case']=='encode:4:2:1024':
                    r['rows'][0]['ns_per_iter']*=1.1; r['timed_ns']*=1.1
        self.mutate('B2','small_file',drift)
        with mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],1,'gate details','refused')) as checks:
            report=self.replay()
        self.assertEqual(checks.call_count,16)
        self.assertEqual(len(report['sample_spreads']),504)
        self.assertEqual(len(report['repeat_checks']),252)
        self.assertEqual(len(report['unqualified_metric_comparisons']),504)
        self.assertEqual(len(report['regression_checks']),16)
        self.assertEqual(report['worker_invocations'],756)
        self.assertTrue(report['distribution_failures']); self.assertTrue(report['short_samples'])
        self.assertTrue(report['repeat_failures']); self.assertTrue(all(r['exit_code']==1 for r in report['regression_checks']))
        self.assertEqual(len(report['layout']['unqualified_layout_comparisons']),256)
        self.assertTrue(report['all_checks_executed']); self.assertFalse(report['all_original_limits_passed'])
        self.assertIsNone(report['candidate_attribution']); self.assertFalse(report['release_approval'])
        self.assertFalse((self.root/'manifest.json').exists())

    def test_passing_diagnostic_cannot_be_used_by_formal_verify_or_materialize(self):
        original=e.verify_sampling
        def analyze(document,root,repository,*,diagnostic=False,bindings=None):
            before=copy.deepcopy(document)
            with mock.patch.object(e.native_tools,'_request_payload',wraps=e.native_tools._request_payload) as frames:
                result=original(document,root,repository,diagnostic=diagnostic,bindings=bindings)
            self.assertEqual(frames.call_count,1)
            self.assertEqual(document,before)
            return result
        with mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')),mock.patch.object(e,'verify_sampling',side_effect=analyze) as analysis:
            report=self.replay()
        analysis.assert_called_once()
        self.assertTrue(report['all_original_limits_passed']); self.assertFalse(report['release_approval'])
        self.assertIsNone(report['layout']['candidate_attribution'])
        with self.assertRaisesRegex(ValueError,'schema 4'): e.verify(self.manifest,self.root)
        with self.assertRaisesRegex(ValueError,'schema 4'): e.materialize(self.manifest,self.root,self.root/'release.env')
        self.assertFalse((self.root/'release.env').exists())
        formal=self.layout_manifest.with_name('manifest.json');formal.write_bytes(self.layout_manifest.read_bytes())
        with self.assertRaisesRegex(ValueError,'modes must match'): layout.verify_paired(formal.parent,self.root)

    def test_finite_metrics_cannot_produce_nonfinite_campaign_ratios(self):
        for repeat_overflow in (False,True):
            for phase in e.PHASES:
                value=1e-308 if phase.startswith('A') and not (repeat_overflow and phase=='A2') else 1e308
                def change(records, value=value):
                    for row in records:
                        if row['case']=='encode:4:2:1024':
                            row['rows'][0]['ns_per_iter']=value
                            row['timed_ns']=value*row['iterations']
                self.mutate(phase,'small_file',change)
            with mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')):
                with self.assertRaisesRegex(ValueError,'nonfinite campaign'):
                    self.replay()

    def test_missing_or_corrupt_raw_records_are_not_silently_accepted(self):
        entry=self.doc['files']['B2']['small_file']['samples']
        (self.root/entry['path']).write_text('[]')
        with self.assertRaisesRegex(ValueError,'digest mismatch'): self.replay()

    def test_rehashed_worker_time_journal_cannot_reorder_cases(self):
        with mock.patch.object(e.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'','')):self.replay()
        path=self.root/'worker-journal.jsonl';rows=path.read_text().splitlines();rows[0],rows[2]=rows[2],rows[0]
        path.write_text('\n'.join(rows)+'\n');self.doc['worker_journal']['sha256']=e.digest(path.read_bytes())
        self.manifest.write_text(json.dumps(self.doc))
        with mock.patch.object(e.subprocess,'run') as gate, self.assertRaisesRegex(ValueError,'journal order'):
            e.verify_diagnostic(self.manifest,self.root)
        gate.assert_not_called()


class CompleteCaptureTests(unittest.TestCase):
    def test_wide_first_phase_continues_all_profiles_then_layouts(self):
        f=lifecycle.CaptureLifecycleTests(methodName='runTest');f.setUp();self.addCleanup(f.doCleanups)
        def layouts(*args,**kwargs):
            self.assertEqual(kwargs,{'diagnostic':True})
            f.emit_layouts(*args)
            folder=args[2];(folder/'manifest.json').rename(folder/'diagnostic-manifest.json')
        f.layouts.side_effect=layouts
        original=f.emit
        def emit(*args,**kwargs):
            rows=original(*args,**kwargs)
            if 'journal' in kwargs:kwargs['journal']({'synthetic-lifecycle-only':True})
            return rows
        f.run.side_effect=emit
        widths=[{'case':'fixture','central_spread':.25,'short_samples':[{'index':3,'wall_ns':1,'timed_ns':1}]}]
        with mock.patch.object(e,'diagnostic_rows',return_value={'selected':[f.payload],'spreads':widths}), mock.patch.object(e,'verify_diagnostic',return_value=({}, {}, {'status':'diagnostic_complete','release_approval':False})) as replay, redirect_stdout(io.StringIO()):
            e.capture(f.baseline,f.candidate,f.output,f.root/'group',diagnostic=True)
        self.assertEqual(f.run.call_count,24)
        self.assertTrue(all((f.output/p/'stream-samples.json').exists() for p in e.PHASES))
        f.verify.assert_not_called();f.profiles.assert_not_called();replay.assert_called_once()
        self.assertTrue((f.output/'diagnostic-manifest.json').exists())
        self.assertFalse((f.output/'manifest.json').exists())
        self.assertEqual(json.loads((f.output/'diagnostic-manifest.json').read_text())['schema_version'],e.DIAGNOSTIC_SCHEMA)

    def test_layout_failure_collects_all_four_phases_and_every_comparison(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);baseline=root/'baseline';candidate=root/'candidate';output=root/'layout'
            def source(path):return BASELINE if path==baseline else CANDIDATE
            def capture(path,destination,group,count):
                rows=paired_records()
                for row in rows:
                    if row['index'] in (10,11):row['wall_ns']*=2;row['ns_per_iter']*=2
                    if path==candidate:row['wall_ns']*=2;row['ns_per_iter']*=2
                return write_phase(destination,source(path),rows)
            with mock.patch.object(e,'source_info',side_effect=source),mock.patch.object(e,'source_at',return_value=BASELINE),mock.patch.object(e,'command',return_value='rustc controlled'),mock.patch.object(layout,'capture',side_effect=capture) as run,redirect_stdout(io.StringIO()):
                report=layout.capture_paired(baseline,candidate,output,root/'group',1_000_000,diagnostic=True)
            self.assertEqual(run.call_count,4)
            self.assertFalse(report['layout_quality_passed']);self.assertFalse(report['layout_gate_passed'])
            self.assertEqual(len(report['unqualified_layout_comparisons']),256)
            self.assertTrue(report['unqualified_regression_failures'])
            self.assertIsNone(report['candidate_attribution']);self.assertFalse(report['release_approval'])
            self.assertTrue((output/'diagnostic-manifest.json').exists());self.assertFalse((output/'manifest.json').exists())

    def test_parent_journal_records_success_and_failed_command_without_changing_exception(self):
        for fail in (False,True):
            with self.subTest(fail=fail),tempfile.TemporaryDirectory() as directory:
                root=pathlib.Path(directory);counts=root/'counts.tsv';counts.write_text('a\t3\n')
                events=[]
                def run(*args,**kwargs):
                    kwargs['stdout'].write('RSE_SAMPLE {"case":"a","isolated_case":true}\n');kwargs['stdout'].flush()
                    if fail:raise subprocess.CalledProcessError(7,['worker'])
                with mock.patch('subprocess.run',side_effect=run):
                    if fail:
                        with self.assertRaises(subprocess.CalledProcessError) as raised:
                            s.run_profile(root,{},'smoke','sample',root/'out.log',counts,cases={'a'},journal=events.append)
                        self.assertEqual(raised.exception.returncode,7)
                    else:s.run_profile(root,{},'smoke','sample',root/'out.log',counts,cases={'a'},journal=events.append)
                self.assertEqual([v['event'] for v in events],['start','end'])
                self.assertEqual(events[0]['iterations'],3)
                self.assertGreaterEqual(events[1]['monotonic_ns'],events[0]['monotonic_ns'])
                self.assertEqual(events[1]['exit_code'],7 if fail else 0)


if __name__=='__main__':unittest.main()
