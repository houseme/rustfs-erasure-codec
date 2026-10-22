"""Counter capability/refusal and original-worker replay; no performance approval."""
import copy
import json
import pathlib
import sys
import unittest
from contextlib import ExitStack
from unittest import mock

sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_release_vm as d
import analyze_baseline_variability as analysis
import test_adjacent_baseline as fixtures


def point(tick,start,end,cpus=(0,)):
    return {'start_ns':start,'end_ns':end,'cpus':{str(c):{'tsc_before':tick*1000,'tsc_after':tick*1000+2,'aperf':tick*400,'mperf':tick*300} for c in cpus}}


class CounterTests(unittest.TestCase):
    def test_read_only_short_reads_and_close_on_partial_open(self):
        with mock.patch.object(d.os,'open',side_effect=[41,OSError('unavailable')]) as opened,mock.patch.object(d.os,'close') as close:
            with self.assertRaises(OSError),ExitStack() as stack:d.MsrReader(stack,[0,1])
            close.assert_called_once_with(41)
            self.assertEqual(opened.call_args_list[0].args,('/dev/cpu/0/msr',d.os.O_RDONLY))
        with ExitStack() as stack,mock.patch.object(d.os,'open',return_value=41),mock.patch.object(d.os,'close'),mock.patch.object(d.os,'pread',return_value=b'\x00'*7):
            reader=d.MsrReader(stack,[0])
            with self.assertRaisesRegex(ValueError,'short'):reader.snapshot()

    def test_real_snapshot_order_is_bounded_and_uses_only_pread(self):
        with mock.patch.object(d.os,'open',return_value=41),mock.patch.object(d.os,'close'),mock.patch.object(d.os,'pread',side_effect=[(i).to_bytes(8,'little') for i in (100,20,10,105)]) as read,ExitStack() as stack:
            snapshot=d.MsrReader(stack,[0]).snapshot()
        self.assertLessEqual(snapshot['start_ns'],snapshot['end_ns'])
        self.assertEqual(snapshot['cpus']['0']['tsc_after'],105)
        self.assertEqual([v.args[2] for v in read.call_args_list],[0x10,0xe8,0xe7,0x10])

class FrequencyTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.AdjacentTests();self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
        f=self.fixture;self.root=f.root;self.output=self.root/'frequency'
        self.cases=[{'profile':'small_file','case':'encode:4:2:1024','repeat':{'exceeds_repeat_limit':True}},
                    {'profile':'small_file','case':'verify:4:2:1024','repeat':{'exceeds_repeat_limit':True}},
                    {'profile':'small_file','case':'reconstruct:4:2:1024','repeat':{'exceeds_repeat_limit':False}}]
        patch=mock.patch.object(analysis,'analyze_adjacent',return_value={'cases':self.cases,'input_sha256':{'original':'a'*64}});self.analysis=patch.start();self.addCleanup(patch.stop)
        (self.root/'original/adjacent').mkdir(parents=True);(self.root/'original/adjacent/plan.json').write_text(json.dumps({'context':f.context}))
        self.capture_root=self.root/'original'
        _,_,_,hashes=d.adjacent.inputs(f.prior,self.root)
        self.analysis.return_value['input_sha256']={'prior/'+n:h for n,h in hashes.items()}
        self.analysis.return_value['input_sha256']['adjacent/plan.json']=d.evidence.digest((self.capture_root/'adjacent/plan.json').read_bytes())
        f.patches[4].side_effect=iter(v for i in range(8) for v in (100+i*50_000_000_000,49_000_000_100+i*50_000_000_000))
        cpus=f.context['isolation']['cpu_affinity'];snapshots=[]
        for i in range(8):
            snapshots.extend([point(2*i+1,i*50_000_000_000+1,i*50_000_000_000+2,cpus),point(2*i+2,i*50_000_000_000+49_000_000_150,i*50_000_000_000+49_000_000_160,cpus)])
        patch=mock.patch.object(d,'MsrReader');self.reader=patch.start();self.addCleanup(patch.stop);self.reader.return_value.snapshot.side_effect=snapshots

    def capture(self):
        f=self.fixture
        return d.capture_frequency(self.root,f.baseline,self.capture_root,f.prior,self.root/'trusted-frozen',self.output,self.root/'unused')

    def replay(self):return d.replay_frequency(self.root,self.capture_root,self.fixture.prior,self.root/'trusted-frozen',self.output)

    def test_every_original_failure_fixed_repetitions_and_counts_no_process_observer(self):
        result=self.capture();plan=json.loads((self.output/'plan.json').read_text())
        self.assertEqual(plan['cohort']['failed_cases'],['small_file/encode:4:2:1024','small_file/verify:4:2:1024'])
        self.assertEqual(len(plan['cohort']['steps']),8);self.assertEqual(result['observations'],96)
        self.assertEqual({s['repetition'] for s in plan['cohort']['steps']},{0,1,2,3})
        self.assertEqual({s['iterations'] for s in plan['cohort']['steps']},{1})
        self.assertEqual(self.replay(),result);self.assertFalse(result['release_approval']);self.assertFalse(result['formal_qualification']);self.assertIsNone(result['candidate_attribution']);self.assertFalse(result['time_normalization']);self.assertFalse(result['exclusive_api_cycles'])
        for call in self.fixture.patches[3].call_args_list:self.assertNotIn('RSE_SAMPLER_DIAGNOSTIC_PROCESS',call.args[1])
        self.assertEqual(self.fixture.patches[3].call_count,8)

    def test_incomplete_or_changed_plan_original_journal_and_result_refuse(self):
        self.capture()
        for name,change in [('plan.json',lambda p:p['cohort']['steps'].pop()),('result.json',lambda p:p.update(release_approval=True)),('workers/000.start.json',lambda p:p.update(start_ns=101)),('workers/000.json',lambda p:p[0]['before'].update(cpu=999))]:
            path=self.output/name;old=path.read_bytes();data=json.loads(old);change(data);path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):self.replay()
            path.write_bytes(old)
        path=self.output/'execution.jsonl';lines=path.read_text().splitlines();path.write_text('\n'.join(lines[:-1])+'\n')
        with self.assertRaisesRegex(ValueError,'incomplete'):self.replay()

    def test_counter_failure_retains_original_worker_and_stops_without_retry(self):
        self.reader.return_value.snapshot.side_effect=[point(1,1,2,self.fixture.context['isolation']['cpu_affinity']),OSError('unavailable')]
        with self.assertRaises(OSError):self.capture()
        self.assertTrue((self.output/'failure.json').exists());self.assertTrue((self.output/'workers/000.json').exists());self.assertTrue((self.output/'workers/000.log').exists());self.assertTrue((self.output/'workers/000.start.json').exists());self.assertFalse((self.output/'result.json').exists())
        self.assertEqual(self.fixture.patches[3].call_count,1)
        with self.assertRaisesRegex(ValueError,'failed frequency'):self.replay()

    def test_statistical_failure_is_preserved_in_all_eight_workers(self):
        def changed(profile,case,path,rows):
            if int(path.stem)==7:
                for r in rows:r['rows'][0]['ns_per_iter']*=1.2;r['timed_ns']*=1.2
        self.fixture.changed=changed
        result=self.capture()
        self.assertEqual(len(result['workers']),8)
        self.assertGreater(result['workers'][-1]['original_metric']['value'],result['workers'][1]['original_metric']['value'])
        self.assertFalse(result['release_approval'])

    def test_full_input_replay_rejection_and_no_failures_prevent_workers(self):
        self.analysis.side_effect=ValueError('incomplete full catalog')
        with self.assertRaisesRegex(ValueError,'incomplete'):self.capture()
        self.fixture.patches[3].assert_not_called();self.assertTrue((self.output/'failure.json').exists())
        self.analysis.side_effect=None
        for row in self.cases:row['repeat']['exceeds_repeat_limit']=False
        self.output=self.root/'no-failures'
        with self.assertRaisesRegex(ValueError,'no full-catalog failures'):self.capture()
        self.fixture.patches[3].assert_not_called()

    def test_changed_full_input_after_replay_stops_before_worker(self):
        self.analysis.return_value['input_sha256']['adjacent/plan.json']='0'*64
        with self.assertRaisesRegex(ValueError,'input changed'):self.capture()
        self.fixture.patches[3].assert_not_called()

    def test_prior_identity_uses_explicit_frozen_object_store_after_rebase(self):
        original=d.adjacent.inputs
        with mock.patch.object(d.adjacent,'inputs',wraps=original) as read:
            self.capture()
        self.assertTrue(read.call_count>=2)
        for call in read.call_args_list:self.assertEqual(call.args[1],self.root/'trusted-frozen')
