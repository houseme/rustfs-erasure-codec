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
        self.doc['schema_version']=2
        self.doc['sampling']=dict(sampling.PROTOCOL,implementation_sha256='e'*64,overlay_sha256='f'*64,
                                  common_iterations={p:{k:1 for k in evidence.expected_cases(p)} for p in evidence.FILES})
        for name,value in (('implementation_digest','e'*64),('overlay_digest','f'*64)):
            patch=mock.patch.object(sampling,name,return_value=value);patch.start();self.addCleanup(patch.stop)
        for context in self.doc['contexts'].values():
            context['settings']=evidence.SAMPLING_ENV
            context['isolation']['cpu_affinity']=list(range(8))
        self.doc['calibration']={}
        for role in ('baseline','candidate'):
            folder='calibration-'+role;(self.root/folder).mkdir()
            self.doc['calibration'][role]={}
            for profile in evidence.FILES:
                rows=self.cases('A1' if role=='baseline' else 'B1',profile)
                records=[self.record(row,profile,role,'calibration',0) for row in rows]
                self.doc['calibration'][role][profile]=self.write(folder,profile,records)
        for phase in evidence.PHASES:
            for profile in evidence.FILES:
                rows=self.cases(phase,profile)
                self.fixture.write_rows(phase,profile,rows)
                records=[]
                for row in rows:
                    records.extend(self.record(row,profile,'baseline' if phase.startswith('A') else 'candidate',kind,i)
                                   for i,kind in enumerate(['warmup']+['sample']*sampling.PROTOCOL['samples']))
                self.doc['files'][phase][profile]['samples']=self.write(phase,profile,records)

    def cases(self,phase,profile):
        rows=self.fixture.fixture_rows(phase,profile)
        for row in rows:
            if profile!='hotspot':
                row['iterations']=1;row['ns_per_iter']=10_000_000.0
                if profile=='stream':row['ns_per_block']=10_000_000.0
        return rows

    def record(self,row,profile,role,kind,index):
        key=row.get('scenario') or ':'.join(str(row[f]) for f in ('operation','data_shards','parity_shards','shard_size','stream_block_size') if f in row)
        return {'kind':kind,'case':key,'revision':self.doc['sources'][role]['commit'],'index':index,'iterations':1,
                'wall_ns':120_000_000,'timed_ns':10_000_000.0,'complete':True,'rows':[copy.deepcopy(row)],
                'before':{'cpu':0,'run_ns':index*100,'wait_ns':index,'switches':index},
                'after':{'cpu':0,'run_ns':index*100+99,'wait_ns':index+1,'switches':index+1}}

    def write(self,folder,profile,records):
        name=folder+'/'+profile+'-samples.json';p=self.root/name;p.write_text(json.dumps(records))
        return {'path':name,'sha256':evidence.digest(p.read_bytes())}

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
