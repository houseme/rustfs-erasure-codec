"""Synthetic timing envelopes verify bounds/refusals, never performance acceptance."""
import copy
import io
import json
import pathlib
import subprocess
import sys
import unittest
from contextlib import redirect_stderr
from unittest import mock

sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import analyze_baseline_variability as analysis
import diagnose_adjacent_baseline as adjacent
import test_adjacent_baseline as fixtures


class EpochTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.AdjacentTests();self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
        self.root=self.fixture.root;self.prior=self.fixture.prior
        self.tool=self.root/'frozen';(self.tool/'scripts').mkdir(parents=True)
        self.script=self.tool/'scripts/diagnose_adjacent_baseline.py'
        self.script_bytes=pathlib.Path(adjacent.__file__).read_bytes();self.script.write_bytes(self.script_bytes)
        folder=self.root/'reservation';folder.mkdir()
        (folder/'status.json').write_text(json.dumps({'status':'collected-diagnostic-only','error':None,'release_approval':False,'isolation_checks':100}))
        (folder/'cleanup.json').write_text(json.dumps(dict.fromkeys(('child_removed','controller_restored','parent_affinities_restored'),True)))
        patch=mock.patch.object(analysis.subprocess,'check_output',return_value=self.script_bytes)
        self.blob=patch.start();self.addCleanup(patch.stop)
        patch=mock.patch.object(analysis.subprocess,'run')
        self.run=patch.start();self.addCleanup(patch.stop)
        self.summary={'cases':126,'observations':3024,'release_approval':False,'formal_qualification':False,'drift_failures':0,'spread_failures':0}
        self.run.return_value=subprocess.CompletedProcess([],0,stdout=json.dumps(self.summary),stderr='')

    def capture(self):return self.fixture.capture()
    def analyze(self):return analysis.analyze_adjacent(self.root,self.prior,self.tool)

    def test_complete_catalog_reuses_frozen_validator_and_preserves_all_traces(self):
        self.capture();result=self.analyze()
        self.assertEqual(result['case_count'],126)
        self.assertEqual(sum(len(c['phases'][p]['trace']) for c in result['cases'] for p in ('A1','A2')),3024)
        self.assertTrue(result['resources_restored']);self.assertFalse(result['release_approval']);self.assertFalse(result['formal_qualification']);self.assertFalse(result['new_acceptance_gate']);self.assertFalse(result['inner_api_gap_measured'])
        self.assertIsNone(result['candidate_attribution']);self.assertFalse(result['prior_kernel_memory_policy_recorded'])
        self.assertEqual(len(result['input_sha256']),515)
        command=self.run.call_args.args[0]
        self.assertEqual(command[1:4],['-I','-S','-B']);self.assertIn('replay',command);self.assertNotIn('capture',command)
        self.assertEqual(self.run.call_count,1)

    def test_incomplete_or_unrestored_capture_is_refused_before_any_replay(self):
        self.capture();p=self.root/'reservation/cleanup.json';original=p.read_bytes();data=json.loads(original);data['parent_affinities_restored']=False;p.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError,'restored resources'):self.analyze()
        p.write_bytes(original);(self.root/'adjacent/workers/251.json').unlink()
        with self.assertRaisesRegex(ValueError,'regular original'):self.analyze()
        self.run.assert_not_called()

    def test_changed_controller_bytes_are_not_executed(self):
        self.capture();self.script.write_bytes(self.script_bytes+b'\n')
        with self.assertRaisesRegex(ValueError,'controller bytes'):self.analyze()
        self.run.assert_not_called()

    def test_members_changed_during_frozen_replay_are_retained_and_refused(self):
        self.capture();path=self.root/'adjacent/workers/000.log';original=path.read_bytes()
        def run(*args,**kwargs):
            path.write_bytes(original+b'changed\n')
            return subprocess.CompletedProcess([],0,stdout=json.dumps(self.summary),stderr='')
        self.run.side_effect=run
        with self.assertRaisesRegex(ValueError,'members changed'):self.analyze()
        self.assertEqual(path.read_bytes(),original+b'changed\n');self.assertEqual(self.run.call_count,1)

    def test_source_changed_during_frozen_replay_is_refused(self):
        self.capture()
        def run(*args,**kwargs):
            self.script.write_bytes(self.script_bytes+b'\n')
            return subprocess.CompletedProcess([],0,stdout=json.dumps(self.summary),stderr='')
        self.run.side_effect=run
        with self.assertRaisesRegex(ValueError,'controller bytes'):self.analyze()

    def test_v2_missing_policy_cannot_be_downgraded_by_an_older_frozen_tool(self):
        self.capture();prov=self.fixture.prov;prov['schema_version']=analysis.evidence.QUALIFICATION_SCHEMA
        (self.prior/'provenance.json').write_text(json.dumps(prov))
        with self.assertRaisesRegex(ValueError,'kernel memory'):self.analyze()
        self.run.assert_not_called()

    def test_statistical_failures_remain_in_the_complete_cohort(self):
        def change(profile,case,path,rows):
            if profile=='small_file' and case=='verify:4:2:1024' and int(path.stem)%2:
                for r in rows:r['rows'][0]['ns_per_iter']*=1.1;r['timed_ns']*=1.1
        self.fixture.changed=change;self.capture();self.summary['drift_failures']=1
        self.run.return_value.stdout=json.dumps(self.summary)
        result=self.analyze()
        self.assertEqual(result['case_count'],126);self.assertEqual(sum(c['repeat']['exceeds_repeat_limit'] for c in result['cases']),1)
        self.assertFalse(result['release_approval']);self.assertFalse(result['formal_qualification'])

    def test_failed_or_malformed_frozen_outcome_is_never_accepted(self):
        self.capture()
        self.run.side_effect=subprocess.CalledProcessError(1,['replay'])
        with self.assertRaises(subprocess.CalledProcessError):self.analyze()
        self.run.side_effect=None
        for field,value in (('release_approval',True),('formal_qualification',True),('cases',126.0)):
            changed=dict(self.summary);changed[field]=value;self.run.return_value.stdout=json.dumps(changed)
            with self.assertRaisesRegex(ValueError,'invalid frozen replay'):self.analyze()

    def test_cli_requires_explicit_frozen_tool_and_preserves_existing_output(self):
        output=self.root/'keep.json';output.write_text('keep')
        with mock.patch.object(sys,'argv',['analysis','--adjacent-capture',str(self.root),'--output',str(output)]),redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):analysis.main()
        self.assertEqual(output.read_text(),'keep');self.run.assert_not_called()
        output.unlink()
        with mock.patch.object(sys,'argv',['analysis','--adjacent-capture',str(self.root),'--output',str(output)]),redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):analysis.main()
        self.run.assert_not_called()

    def test_failure_marker_cannot_be_hidden_behind_successful_parent_status(self):
        self.capture();(self.root/'adjacent/failure.json').write_text('{}')
        with self.assertRaisesRegex(ValueError,'failed adjacent'):self.analyze()
        self.run.assert_not_called()

    def test_resource_state_is_checked_from_the_same_validated_snapshot(self):
        self.capture();original=analysis.snapshot_members;path=self.root/'reservation/cleanup.json'
        def replaced(paths):
            value=json.loads(path.read_text());value['child_removed']=False;path.write_text(json.dumps(value))
            yield from original(paths)
        with mock.patch.object(analysis,'snapshot_members',side_effect=replaced):
            with self.assertRaisesRegex(ValueError,'restored resources'):self.analyze()
        self.run.assert_not_called()
