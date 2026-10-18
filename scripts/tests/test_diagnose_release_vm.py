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
    return {'cpu':0,'run_ns':tick,'wait_ns':tick,'switches':tick,'process':{
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

    def test_partial_duplicate_or_invalid_threads_reject(self):
        for mutation in ('partial','duplicate','negative','flag'):
            records=self.records();p=records[0]['before']['process']
            if mutation=='partial':p['complete']=False
            elif mutation=='duplicate':p['threads'].append(copy.deepcopy(p['threads'][0]))
            elif mutation=='negative':p['threads'][0]['minor_faults']=-1
            else:p['threads'][0]['measuring_thread']='true'
            with self.assertRaises(ValueError):diagnostic.basic_records(records)

    def test_complete_diagnostic_keeps_faults_and_threads_separate(self):
        report=diagnostic.analyze(self.records(),'fixture')
        self.assertEqual(len(report[0]['samples']),9)
        self.assertEqual(report[0]['central_ns_spread'],0)
        row=report[0]['samples'][0]
        self.assertEqual(row['process_faults'],{'minor_faults':1,'major_faults':0})
        self.assertTrue(row['stable_thread_set'])

    def test_short_samples_are_diagnostic_flags_not_release_approval(self):
        rows=self.records()
        report=diagnostic.analyze(rows,'fixture')
        self.assertFalse(report[0]['samples'][0]['interval_sufficient'])
        self.assertNotIn('status',report[0])

    def test_missing_samples_and_reset_counters_reject(self):
        rows=self.records();rows.pop()
        with self.assertRaises(ValueError):diagnostic.analyze(rows,'fixture')
        rows=self.records();rows[0]['after']['process']['minor_faults']=0
        with self.assertRaises(ValueError):diagnostic.analyze(rows,'fixture')

if __name__=='__main__':unittest.main()
