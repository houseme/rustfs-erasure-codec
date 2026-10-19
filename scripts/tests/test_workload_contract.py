"""Replay source-bound input contracts; fixtures never qualify performance."""
import copy
import pathlib
import sys
import unittest

sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parent))
import release_evidence as evidence
import release_sampling as sampling
import test_release_evidence as fixtures
from layout_fixture import BASELINE,CANDIDATE

class WorkloadContractTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.EvidenceTests()
        self.fixture.sources={'baseline':BASELINE,'candidate':CANDIDATE}

    def records(self, profile, case):
        row=next(row for row in self.fixture.fixture_rows('A1',profile) if row.get('scenario') == case or ':'.join(str(row[f]) for f in ('operation','data_shards','parity_shards','shard_size','stream_block_size') if f in row) == case)
        return [self.fixture.record(row,profile,'baseline','warmup' if i<3 else 'sample',i) for i in range(12)]

    def test_all_126_cases_require_real_api_masks_seed_and_worker_lifetime(self):
        covered=0
        for profile in evidence.FILES:
            for case in evidence.expected_cases(profile):
                row=self.records(profile,case)[0]
                sampling.validate_workload(row,profile)
                covered+=1
                for field,value in (('codec_lifetime','observation'),('source_lifetime','unknown'),('construction','reverse'),('generator','entropy'),('seed',True),('call_entry','slice'),('output_policy','reuse'),('missing_mask',-1),('required_mask',True),('changed_mask',-1)):
                    changed=copy.deepcopy(row);changed['workload'][field]=value
                    with self.assertRaises(ValueError):sampling.validate_workload(changed,profile)
                for value in (None,{}):
                    changed=dict(row,workload=value)
                    with self.assertRaises(ValueError):sampling.validate_workload(changed,profile)
        self.assertEqual(covered,126)

    def test_hotspot_rng_must_keep_original_generation_volume_and_continue(self):
        for _,_,case,_,_ in sampling.HOTSPOT_CASES:
            rows=self.records('hotspot',case)
            sampling.validate_workload_sequence(rows,'hotspot')
            for field in ('bytes_before','bytes_after'):
                changed=copy.deepcopy(rows);changed[3]['workload']['rng'][field]+=1
                with self.assertRaisesRegex(ValueError,'consumption'):sampling.validate_workload_sequence(changed,'hotspot')
            changed=copy.deepcopy(rows)
            changed[3]['workload']['rng']['next_before']=list(rows[0]['workload']['rng']['next_before'])
            with self.assertRaisesRegex(ValueError,'reset'):sampling.validate_workload_sequence(changed,'hotspot')

    def test_rng_previews_reject_missing_boolean_negative_and_wrong_width(self):
        case=sampling.HOTSPOT_CASES[0][2]
        for value in (None,[1],[1,True],[-1,1],[2**64,1]):
            rows=self.records('hotspot',case);rows[0]['workload']['rng']['next_after']=value
            with self.assertRaises(ValueError):sampling.validate_workload_sequence(rows,'hotspot')
        rows=self.records('hotspot',case);rows[0]['workload']['rng']=None
        with self.assertRaises(ValueError):sampling.validate_workload_sequence(rows,'hotspot')

    def test_native_row_seed_and_hotspot_api_shape_must_match_actual_contract(self):
        for profile in ('smoke','small_file','stream'):
            case=next(iter(evidence.expected_cases(profile)));row=self.records(profile,case)[0]
            row['rows'][0]['seed']+=1
            with self.assertRaisesRegex(ValueError,'row seed'):sampling.validate_workload(row,profile)
        for _,_,case,_,_ in sampling.HOTSPOT_CASES:
            for field,value in (('data_shards',1),('parity_shards',1),('shard_size',1),('useful_shards',100),('missing_pattern','none'),('required_pattern','none'),('baseline_operation','skip'),('candidate_operation','skip')):
                row=self.records('hotspot',case)[0];row['rows'][0][field]=value
                with self.assertRaisesRegex(ValueError,'actual API/mask'):sampling.validate_workload(row,'hotspot')

    def test_published_and_candidate_build_outputs_ignore_a_shared_caller_cache(self):
        with __import__('tempfile').TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary)
            caller={'CARGO_TARGET_DIR':str(root/'shared'),'RUSTUP_TOOLCHAIN':'stable'}
            baseline=sampling.build_environment(root/'baseline',caller)
            candidate=sampling.build_environment(root/'candidate',caller)
            self.assertNotEqual(baseline['CARGO_TARGET_DIR'],candidate['CARGO_TARGET_DIR'])
            self.assertEqual(baseline['CARGO_TARGET_DIR'],str(root.resolve()/'baseline/target'))
            self.assertEqual(caller['CARGO_TARGET_DIR'],str(root/'shared'))
            self.assertEqual(candidate['RUSTUP_TOOLCHAIN'],'stable')

    def test_rendered_smoke_and_hotspot_use_one_prepared_entry_and_no_entropy(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        rendered=sampling.render(root)
        self.assertNotIn('rand::random',rendered['hotspot.rs'])
        self.assertEqual(rendered['smoke.rs'].count('fn execute_smoke_kernel('),1)
        for profile in ('smoke','hotspot','small_file','stream'):
            self.assertIn('sampling::run_prepared',rendered[profile+'.rs'])
        for name in ('data','some'):
            body=sampling.block(rendered['hotspot.rs'],'fn bench_reconstruct_'+name+'_hotspot')
            self.assertNotIn('ReedSolomon::new',body)
            self.assertIn('option_shards_with_missing',body)
            self.assertIn('let baseline_start = Instant::now();',body)
            self.assertIn('let candidate_start = Instant::now();',body)
        self.assertEqual({r[2] for r in sampling.HOTSPOT_CASES},evidence.expected_cases('hotspot'))

if __name__=='__main__':unittest.main()
