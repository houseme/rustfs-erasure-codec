"""Synthetic scheduling/refusal fixtures never certify measured performance."""
import copy
import json
import pathlib
import shutil
import sys
import unittest
from unittest import mock
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_adjacent_baseline as a
import test_release_evidence as fixtures


class AdjacentTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.EvidenceTests();self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
        self.root=self.fixture.root;self.prior=self.root/'prior';self.prior.mkdir();self.baseline=self.root/'baseline';self.baseline.mkdir();self.output=self.root/'adjacent'
        shutil.copytree(self.root/'calibration-baseline',self.prior/'calibration')
        self.source=self.fixture.sources['baseline'];self.tool=self.fixture.sources['candidate'];self.context=copy.deepcopy(self.fixture.context)
        old=copy.deepcopy(self.context);old['runtime'].pop('kernel_memory_policy')
        self.prov={'schema_version':'baseline-qualification-v1','release_approval':False,'candidate_attribution':None,'source':self.source,'tool':self.tool,'context':old,'sampling':dict(a.s.PROTOCOL,implementation_sha256='e'*64,overlay_sha256='f'*64),'harness_sha256':'d'*64}
        self.doc={'source':self.source,'tool':self.tool,'context':old,'calibration':{},'sampling':dict(common_iterations={p:{c:1 for c in a.e.expected_cases(p)} for p in a.e.FILES})}
        for p in a.e.FILES:
            name='calibration/'+p+'-samples.json';self.doc['calibration'][p]={'path':name,'sha256':a.e.digest((self.prior/name).read_bytes())}
        a.dump(self.prior/'provenance.json',self.prov);a.dump(self.prior/'qualification.partial.json',self.doc)
        self.stack=mock.patch.object(a.e,'source_at',side_effect=lambda root,ref:self.source if ref==a.e.BASELINE_COMMIT else self.tool);self.stack.start();self.addCleanup(self.stack.stop)
        self.patches=[]
        for obj,name,options in ((a.e,'source_info',{'side_effect':lambda root:self.source if root==self.baseline else self.tool}),(a.e,'environment_context',{'side_effect':lambda *args:copy.deepcopy(self.context)}),(a.e,'prepare_capture_source',{}),(a.s,'run_profile',{'side_effect':self.emit}),(a.time,'monotonic_ns',{'side_effect':iter(v for i in range(252) for v in (i*50_000_000_000,i*50_000_000_000+49_000_000_000))})):
            patch=mock.patch.object(obj,name,**options);self.patches.append(patch.start());self.addCleanup(patch.stop)
        self.changed=lambda *args:None

    def emit(self,root,env,profile,mode,path,counts_file,*,cases):
        case=next(iter(cases));row=next(r for r in self.fixture.fixture_rows('A1',profile) if ':'.join(str(r[k]) for k in ('operation','data_shards','parity_shards','shard_size','stream_block_size') if k in r)==case) if profile!='hotspot' else next(r for r in self.fixture.fixture_rows('A1',profile) if r['scenario']==case)
        rows=[self.fixture.record(row,profile,'baseline',kind,index) for index,kind in enumerate(['warmup']*3+['sample']*9)]
        self.changed(profile,case,path,rows)
        path.write_text(''.join('RSE_TIMING '+json.dumps({k:r[k] for k in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')})+'\nRSE_SAMPLE '+json.dumps(r)+'\n' for r in rows))
        return rows

    def capture(self):return a.capture(self.root,self.baseline,self.prior,self.output,self.root/'unused')

    def test_all_cases_are_adjacent_without_special_counts_or_release_approval(self):
        result=self.capture();plan=a.e.json_data(self.output/'plan.json')
        self.assertEqual(len(plan['steps']),252);self.assertEqual(result['cases'],126);self.assertEqual(result['observations'],3024)
        for first,second in zip(plan['steps'][::2],plan['steps'][1::2]):
            self.assertEqual((first['profile'],first['case'],first['iterations']),(second['profile'],second['case'],second['iterations']))
            self.assertEqual((first['phase'],second['phase']),('A1','A2'))
        self.assertFalse(result['release_approval']);self.assertFalse(result['formal_qualification']);self.assertIsNone(result['candidate_attribution'])
        self.assertEqual(a.replay(self.output,self.prior,self.root),result)
        for name,value in (('cases',True),('formal_qualification',True),('release_approval',True)):
            original=copy.deepcopy(result);original[name]=value;(self.output/'result.json').write_text(json.dumps(original))
            with self.assertRaises(ValueError):a.replay(self.output,self.prior,self.root)
        (self.output/'result.json').write_text(json.dumps(result))

    def test_bad_worker_contract_retains_raw_records_and_stops(self):
        def change(profile,case,path,rows):rows[1]['rows'][0]['backend_id']='changed'
        self.changed=change
        with self.assertRaisesRegex(ValueError,'incompatible'):self.capture()
        self.assertTrue((self.output/'failure.json').exists());self.assertTrue((self.output/'workers/000.log').exists());self.assertTrue((self.output/'workers/000.json').exists());self.assertFalse((self.output/'result.json').exists())
        self.assertEqual(self.patches[3].call_count,1)

    def test_existing_destination_and_changed_prior_are_not_accepted(self):
        self.output.mkdir();sentinel=self.output/'sentinel';sentinel.write_text('keep')
        with self.assertRaises(FileExistsError):self.capture()
        self.assertEqual(list(self.output.iterdir()),[sentinel])
        member=self.prior/'calibration/smoke-samples.json';member.write_text('[]')
        with self.assertRaisesRegex(ValueError,'prior calibration'):a.inputs(self.prior,self.root)

    def test_schedule_requires_the_complete_positive_integer_catalog(self):
        counts=self.doc['sampling']['common_iterations']
        for value in (0,True,1.0,-1,1_000_000_001):
            changed=copy.deepcopy(counts);changed['smoke'][next(iter(changed['smoke']))]=value
            with self.assertRaises(ValueError):a.schedule(changed)
        changed=copy.deepcopy(counts);changed['stream'].pop(next(iter(changed['stream'])))
        with self.assertRaises(ValueError):a.schedule(changed)

    def test_rehashed_reordered_or_nonadjacent_execution_is_refused(self):
        result=self.capture();path=self.output/'execution.jsonl';original=[json.loads(r) for r in path.read_text().splitlines()]
        for change in (lambda r:r.reverse(),lambda r:r[1].update(start_ns=r[0]['end_ns']+a.MAX_PAIR_GAP_NS+1),lambda r:r[0]['after']['runtime']['kernel_memory_policy'].update(process_thp_disable=1)):
            rows=copy.deepcopy(original);change(rows);path.write_text(''.join(json.dumps(r)+'\n' for r in rows));summary=copy.deepcopy(result);summary['execution_sha256']=a.e.digest(path.read_bytes());(self.output/'result.json').write_text(json.dumps(summary))
            with self.assertRaises(ValueError):a.replay(self.output,self.prior,self.root)

    def test_quality_failures_are_preserved_across_the_whole_predeclared_cohort(self):
        def change(profile,case,path,rows):
            if profile=='small_file' and case=='verify:4:2:1024' and int(path.stem)%2:
                for r in rows:r['rows'][0]['ns_per_iter']*=1.1;r['timed_ns']*=1.1
        self.changed=change
        result=self.capture()
        self.assertEqual(len(result['checks']),126);self.assertEqual(sum(r['drift']>.05 for r in result['checks']),1);self.assertFalse(result['release_approval'])

    def test_capture_rejects_nonadjacent_workers_before_starting_another(self):
        self.patches[4].side_effect=iter((0,49_000_000_000,60_000_000_001))
        with self.assertRaisesRegex(ValueError,'gap exceeded'):self.capture()
        self.assertEqual(self.patches[3].call_count,1)
        self.assertTrue((self.output/'workers/000.json').exists())
        self.assertTrue((self.output/'failure.json').exists())
        self.assertFalse((self.output/'result.json').exists())

    def test_runtime_policy_change_retains_worker_and_refuses_replay(self):
        changed=copy.deepcopy(self.context);changed['runtime']['kernel_memory_policy']['process_thp_disable']=1
        self.patches[1].side_effect=[copy.deepcopy(self.context),copy.deepcopy(self.context),changed]
        with self.assertRaisesRegex(ValueError,'runtime context changed'):self.capture()
        self.assertEqual(self.patches[3].call_count,1)
        self.assertTrue((self.output/'workers/000.log').exists())
        self.assertTrue((self.output/'workers/000.json').exists())
        with self.assertRaisesRegex(ValueError,'failed diagnostic'):a.replay(self.output,self.prior,self.root)

    def record_current_prior(self):
        self.prov['schema_version']=a.e.QUALIFICATION_SCHEMA
        self.prov['context']=copy.deepcopy(self.context)
        self.doc['context']=copy.deepcopy(self.context)
        (self.prior/'provenance.json').write_text(json.dumps(self.prov))
        (self.prior/'qualification.partial.json').write_text(json.dumps(self.doc))

    def test_known_prior_policy_is_preserved_in_capture_and_replay(self):
        self.record_current_prior()
        result=self.capture()
        self.assertEqual(a.replay(self.output,self.prior,self.root),result)
        plan=self.output/'plan.json';changed=a.e.json_data(plan)
        changed['context']['runtime']['kernel_memory_policy']['process_thp_disable']=1
        plan.write_text(json.dumps(changed))
        with self.assertRaisesRegex(ValueError,'kernel policy differs'):a.replay(self.output,self.prior,self.root)

    def test_known_prior_policy_change_stops_before_first_worker(self):
        self.record_current_prior()
        self.context['runtime']['kernel_memory_policy']['process_thp_disable']=1
        with self.assertRaisesRegex(ValueError,'kernel policy differs'):self.capture()
        self.patches[3].assert_not_called()
        self.assertTrue((self.output/'failure.json').exists())
        self.assertFalse((self.output/'plan.json').exists())

    def test_v2_prior_requires_its_policy_before_starting_any_worker(self):
        for index,missing in enumerate((True,False)):
            self.record_current_prior()
            if missing:self.prov['context']['runtime'].pop('kernel_memory_policy')
            else:self.prov['context']['runtime']['kernel_memory_policy']=None
            self.doc['context']=copy.deepcopy(self.prov['context'])
            (self.prior/'provenance.json').write_text(json.dumps(self.prov))
            (self.prior/'qualification.partial.json').write_text(json.dumps(self.doc))
            self.output=self.root/f'invalid-v2-{index}'
            with self.assertRaises(ValueError):self.capture()
            self.patches[3].assert_not_called()
            self.assertTrue((self.output/'failure.json').exists())
