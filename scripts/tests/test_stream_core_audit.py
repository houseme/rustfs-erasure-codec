"""Refusal/ownership contracts; synthetic ledgers are never performance proof."""
import copy
import pathlib
import sys
import tempfile
import unittest
from unittest import mock
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import stream_core_audit as audit
import release_sampling as sampling

ROOT=pathlib.Path(__file__).resolve().parents[2]
CASE='verify_stream:4:2:65536:65536'


def current_verify_ledger(native=True):
    # Independent single-block stream: reserve, read, initialize/verify, EOF,
    # release. Repeated calls keep their original observation/call coordinates.
    events=[]
    for observation in range(12):
        for call in (1,2):
            scope=observation*2+call
            def event(phase,block,slot=None,length=0,capacity=0,visible=0,complete=False):
                events.append({'observation':observation,'call':call,'scope':scope,'parent':0,
                    'api':'verify_stream','variant':'retained-scratch','block':block,'phase':phase,'slot':slot,
                    'len':length,'capacity':capacity,'visible_len':visible,'occupied':slot is not None,
                    'offsets':[16]*3 if capacity else None,'elapsed_ns':len(events),
                    'faults':[len(events),0] if native else None,'complete':complete,'valid':True if complete else None,
                    'snapshot':({'status':'mapped','pages':17,'resident_pages':17,'errno':None} if native else {'status':'unsupported','pages':None,'resident_pages':None,'errno':None}) if capacity else {'status':'empty','pages':0,'resident_pages':0,'errno':None}})
            event('enter',0)
            for stage,block in [('reserved',0),('read',1),('ready',1),('coded',1),('read',2)]:
                for index in range(6):event(stage,block,index,0 if stage=='reserved' else 65536,65536,0 if stage=='reserved' or block==2 else 65536)
                for index in (48,49):
                    occupied=stage in ('ready','coded') or block==2
                    event(stage,block,index,65536 if occupied else 0,65536 if occupied else 0,65536 if occupied else 0)
            for index in [0,1,2,3,4,5,48,49]:event('released',2,index,65536,65536,0 if index<6 else 65536,True)
            event('exit',2,complete=True)
    return {'schema':'stream-core-lifecycle-v1','page_size':4096,'errors':0,'calls':[2]*12,'release_approval':False,'events':events}


class SourceContractTests(unittest.TestCase):
    def test_source_hooks_are_reversible_and_formal_sampler_is_untouched(self):
        original=(ROOT/'src/core/stream.rs').read_text();changed=audit.source_patch(original)
        self.assertNotIn('rse_probe',original);self.assertIn('rse_probe',changed)
        self.assertIn('StreamReconstructPlan::new',changed)
        before=sampling.render(ROOT);traced=audit.render(ROOT)
        self.assertEqual(before,sampling.render(ROOT))
        self.assertNotIn(audit.MODULE,before['engine.rs']);self.assertIn(audit.MODULE,traced['engine.rs'])
        self.assertEqual(traced['stream_views.rs'],audit.views.render(ROOT)['stream_views.rs'])
        with self.assertRaises(ValueError):audit.source_patch(original.replace('pub fn encode_stream(','pub fn renamed('))
        with self.assertRaises(ValueError):audit.source_patch(changed)

    def fixture(self,path):
        for name in audit.NAMES:
            target=path/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text((ROOT/name).read_text())
        return {name:(path/name).read_text() for name in audit.NAMES}

    def test_source_ownership_cleanup_and_unexpected_mutation_preservation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);original=self.fixture(root)
            with self.assertRaises(RuntimeError):
                with audit.source_overlay(root) as bindings:
                    self.assertEqual(len(bindings),4);raise RuntimeError('failed worker')
            self.assertEqual(original,{n:(root/n).read_text() for n in audit.NAMES})
            self.assertFalse((root/('src/'+audit.MODULE+'.rs')).exists())
            with self.assertRaisesRegex(ValueError,'preserved'):
                with audit.source_overlay(root):(root/'src/core/stream.rs').write_text('unexpected edit')
            self.assertEqual((root/'src/core/stream.rs').read_text(),'unexpected edit')

    def test_existing_capture_is_never_modified(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);(root/'sentinel').write_text('keep')
            with mock.patch.object(audit,'_check') as check:
                with self.assertRaises(FileExistsError):audit.check(ROOT,root,audit.e.BASELINE_COMMIT)
                check.assert_not_called()
            self.assertEqual([p.name for p in root.iterdir()],['sentinel'])


class WorkerContractTests(unittest.TestCase):
    def setUp(self):
        import json
        import test_release_evidence as fixtures
        from layout_fixture import view_fixture
        self.json=json;self.temporary=tempfile.TemporaryDirectory();self.addCleanup(self.temporary.cleanup)
        self.root=pathlib.Path(self.temporary.name);self.source={'commit':'c'*40}
        factory=fixtures.EvidenceTests(methodName='runTest');factory.sources={'candidate':self.source}
        row=next(r for r in factory.fixture_rows('B1','stream') if r['operation']=='verify_stream' and r['data_shards']==4 and r['shard_size']==65536)
        row['iterations']=2;self.rows=[];self.views=[]
        for index,kind in enumerate(['warmup']*3+['sample']*9):
            record=factory.record(row,'stream','candidate',kind,index);record['iterations']=2;record['timed_ns']*=2
            for n in ('run_ns','wait_ns'):record['before'][n]=index*100;record['after'][n]=index*100+99
            record['after']['process_minor_faults']=10000
            for target in ('fixture','workload'):record[target]['call_entry']=audit.views.ENTRIES['verify_stream']
            self.rows.append(record)
            view=view_fixture(CASE,2,index)
            for window in view['windows']:
                for target in ('before','after'):
                    window[target][1]=0;window[target][2]+=index*100;window[target][3]+=index*100
            self.views.append(view)
        self.data={'samples':self.rows,'views':self.views,'core':current_verify_ledger()}
        self.name='candidate-stream-'+CASE.replace(':','_');self.write()

    def write(self):
        member=self.root/(self.name+'.json');member.write_text(self.json.dumps(self.data))
        raw=''.join('RSE_TIMING '+self.json.dumps({k:r[k] for k in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')})+'\nRSE_SAMPLE '+self.json.dumps(r)+'\n' for r in self.data['samples'])
        raw+=''.join(audit.views.PREFIX+self.json.dumps(v)+'\n' for v in self.data['views'])+audit.PREFIX+self.json.dumps(self.data['core'])+'\n'
        journal=self.root/(self.name+'-journal.jsonl');journal.write_text(raw)
        self.entry={'path':member.name,'sha256':audit.e.digest(member.read_bytes()),'numeric_journal_path':journal.name,'numeric_journal_sha256':audit.e.digest(journal.read_bytes()),'raw_log_sha256':'a'*64}

    def validate(self):
        data,rows,views=audit.worker_data(self.root,self.name,self.entry)
        return audit.worker_report(rows,views,data['core'],CASE,self.source,4096,list(range(8)),True)

    def test_complete_numerical_worker_replays_without_private_logs(self):
        self.assertEqual(self.validate()['calls'],24)

    def test_recomputed_member_digests_cannot_hide_wrong_runtime_or_counters(self):
        original=copy.deepcopy(self.data)
        for change in (lambda d:d['samples'][1]['rows'][0].update(backend_id='changed'),
                       lambda d:d['samples'][2]['rows'][0].update(stream_io_backend='changed'),
                       lambda d:d['samples'][0]['after'].update(process_minor_faults=0),
                       lambda d:d['views'][0]['windows'][0]['after'].__setitem__(2,1000000),
                       lambda d:d['core']['events'][1].update(parent=5)):
            self.data=copy.deepcopy(original);change(self.data);self.write()
            with self.assertRaises(ValueError):self.validate()

    def test_digest_path_and_original_journal_bindings_are_required(self):
        for change in (lambda e:e.update(path='../escape.json'),lambda e:e.update(numeric_journal_path='unexpected'),lambda e:e.update(sha256='f'*64),lambda e:e.update(numeric_journal_sha256='f'*64)):
            self.write();change(self.entry)
            with self.assertRaises(ValueError):self.validate()
        self.write();p=self.root/self.entry['numeric_journal_path'];p.write_text(p.read_text().replace('RSE_TIMING','IGNORED_TIMING',1));self.entry['numeric_journal_sha256']=audit.e.digest(p.read_bytes())
        with self.assertRaises(ValueError):self.validate()
