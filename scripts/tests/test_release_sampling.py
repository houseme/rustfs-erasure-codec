"""Duration protocol fixtures test the contract, not actual performance."""
import copy
import json
import pathlib
import sys
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
        mutations=[lambda r:r.pop(1),lambda r:r.insert(1,copy.deepcopy(r[1])),
                   lambda r:r[1].update(index=2),lambda r:r[1].update(wall_ns=1),
                   lambda r:r[1].update(timed_ns=1),lambda r:r[1].update(iterations=2),
                   lambda r:r[1]['after'].update(cpu=20),lambda r:r[1]['after'].update(run_ns=-1)]
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
            for index in (1,2):
                records[index]['rows'][0]['ns_per_iter']*=.8;records[index]['timed_ns']*=.8
            for index in (8,9):
                records[index]['rows'][0]['ns_per_iter']*=1.2;records[index]['timed_ns']*=1.2
        self.mutate(change)
        with self.assertRaisesRegex(ValueError,'distributions'):self.verify()

    def test_a_single_cold_completion_does_not_freeze_calibration(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text())
        records.pop(0)
        with self.assertRaisesRegex(ValueError,'confirmations'):
            evidence.calibration_counts(records,'smoke',self.sources_commit('baseline'))

    def test_cold_success_followed_by_warm_shortfall_grows_then_confirms(self):
        path=self.root/self.doc['calibration']['baseline']['smoke']['path']
        records=json.loads(path.read_text());first=records[0];case=first['case']
        replacement=[]
        for count,ns,wall,met,complete in ((1,102000000,106000000,True,False),(1,42000000,42000000,False,False),(3,42000000,126000000,True,False),(3,42000000,126000000,True,True)):
            row=copy.deepcopy(first);row.update(iterations=count,timed_ns=ns*count,wall_ns=wall,target_met=met,complete=complete)
            row['rows'][0].update(iterations=count,ns_per_iter=ns)
            replacement.append(row)
        records=replacement+[r for r in records if r['case']!=case]
        counts=evidence.calibration_counts(records,'smoke',self.sources_commit('baseline'))
        self.assertEqual(counts[case],3)

    def sources_commit(self,role):
        return self.doc['sources'][role]['commit']

    def test_calibration_and_protocol_cannot_be_changed(self):
        original=copy.deepcopy(self.doc)
        for field in ('samples','target_timed_ns','central_spread_limit','implementation_sha256','overlay_sha256'):
            self.doc=copy.deepcopy(original);self.doc['sampling'][field]='invalid'
            with self.assertRaises(ValueError):self.verify()
        self.doc=copy.deepcopy(original)
        key=next(iter(self.doc['sampling']['common_iterations']['small_file']))
        self.doc['sampling']['common_iterations']['small_file'][key]=2
        with self.assertRaises(ValueError):self.verify()


if __name__=='__main__':unittest.main()
