"""Synthetic scratch ledgers test refusal contracts, never certify performance."""
import copy
import pathlib
import tempfile
import unittest
from unittest import mock
import sys
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import verify_scratch_audit as audit
import release_sampling as sampling


def ledger(case,iterations=2,page=4096):
    _,_,m,size=case.split(':');size=int(size)*int(m);capacity=max(4096,size)
    events=[]
    for observation in range(12):
        for call in range(1,iterations+1):
            for phase in ('reserved','initialized','verified','released'):
                index=len(events)
                pages=(16+capacity+page-1)//page
                events.append({'observation':observation,'call':call,'phase':phase,
                    'len':0 if phase=='reserved' else size,'capacity':capacity,'offsets':[16]*3,
                    'inline':size<=4096,'elapsed_ns':index+1,'faults':[index*2,0],
                    'snapshot':{'status':'mapped','pages':pages,'resident_pages':pages,'errno':None},
                    'valid':True if phase in ('verified','released') else None})
    return {'schema':'verify-scratch-lifecycle-v1','page_size':page,'errors':0,'calls':[iterations]*12,'release_approval':False,'events':events}


class VerifyScratchTests(unittest.TestCase):
    def test_complete_original_verify_catalog_and_page_sizes(self):
        self.assertEqual(sum(len(audit.cases(p)) for p in ('smoke','small_file')),14)
        for profile in ('smoke','small_file'):
            for case in audit.cases(profile):
                for page in (4096,16384,65536):
                    result=audit.validate(ledger(case,page=page),case,2,page,native=True)
                    self.assertEqual(result['calls'],24)
                    self.assertEqual(result['events'],96)

    def test_every_call_and_order_is_required_without_raw_mutation(self):
        case='verify:4:2:1024'
        for change in (lambda d:d['events'].pop(),lambda d:d.update(errors=1),
                       lambda d:d['calls'].__setitem__(4,1),lambda d:d['events'].reverse(),
                       lambda d:d['events'][2].update(valid=False),
                       lambda d:d['events'][2].update(address=1234),
                       lambda d:d['events'][1].update(capacity=8192),
                       lambda d:d['events'][1].update(offsets=[32]*3),
                       lambda d:d['events'][1].update(faults=[-1,0]),
                       lambda d:d['events'][1].update(faults=None)):
            data=ledger(case);change(data);original=copy.deepcopy(data)
            with self.subTest(data=data['events'][:2]):
                with self.assertRaises(ValueError):audit.validate(data,case,2,4096,native=True)
                self.assertEqual(data,original)

    def test_live_and_former_range_errors_are_distinct(self):
        case='verify:4:2:65536';data=ledger(case)
        for event in data['events']:
            if event['phase']=='released':event['snapshot'].update(status='not_fully_mapped',resident_pages=None,errno=12)
        audit.validate(data,case,2,4096,native=True)
        data['events'][1]['snapshot'].update(status='not_fully_mapped',resident_pages=None,errno=12)
        with self.assertRaisesRegex(ValueError,'live scratch'):audit.validate(data,case,2,4096,native=True)

    def test_nonnative_coverage_is_explicit_and_cannot_gain_fake_counts(self):
        case='verify:4:2:1024';data=ledger(case)
        for event in data['events']:
            event['faults']=None
            event['snapshot']={'status':'unsupported','pages':None,'resident_pages':None,'errno':None}
        audit.validate(data,case,2,4096)
        with self.assertRaises(ValueError):audit.validate(data,case,2,4096,native=True)
        data['events'][0]['snapshot']['resident_pages']=0
        with self.assertRaises(ValueError):audit.validate(data,case,2,4096)

    def test_diagnostic_source_copy_restores_on_failure_and_preserves_tampering(self):
        original=(pathlib.Path(__file__).resolve().parents[2]/'src/core/verify.rs').read_text()
        with tempfile.TemporaryDirectory() as folder:
            root=pathlib.Path(folder);(root/'src/core').mkdir(parents=True)
            (root/'src/lib.rs').write_text('// original')
            (root/'src/core/verify.rs').write_text(original)
            with self.assertRaisesRegex(RuntimeError,'original failure'):
                with audit.source_overlay(root):raise RuntimeError('original failure')
            self.assertEqual((root/'src/core/verify.rs').read_text(),original)
            self.assertEqual((root/'src/lib.rs').read_text(),'// original')
            self.assertFalse((root/('src/'+audit.MODULE+'.rs')).exists())
            with self.assertRaisesRegex(ValueError,'preserved'):
                with audit.source_overlay(root):(root/'src/core/verify.rs').write_text('unexpected edit')
            self.assertEqual((root/'src/core/verify.rs').read_text(),'unexpected edit')

    def test_source_and_sampler_changes_are_local_and_narrow(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        before=sampling.render(root);after=audit.render(root)
        self.assertEqual(before,sampling.render(root))
        self.assertEqual({name for name in before if before[name]!=after[name]},{'engine.rs'})
        source=(root/'src/core/verify.rs').read_text();changed=audit.source_patch(source)
        self.assertEqual(source.split('    /// Verify using a pre-allocated')[1],changed.split('    /// Verify using a pre-allocated')[1])
        self.assertIn('rse_probe.verified(valid)',changed)
        self.assertIn('Guard::new(scratch_len)',changed)
        with self.assertRaises(ValueError):audit.source_patch(source.replace('scratch.resize(scratch_len, F::zero());','unexpected();'))
        native=audit.native_template()
        engine=(sampling.TEMPLATES/'engine.rs').read_text()
        for name in ('fn read_bounded','fn process_faults'):self.assertIn(sampling.block(engine,name),native)


class DiagnosticOwnershipTests(unittest.TestCase):
    def test_existing_capture_is_never_given_a_failure_file_or_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);sentinel=root/'result.json';sentinel.write_text('original')
            with self.assertRaises(FileExistsError):audit.check(root,root,'unused')
            self.assertEqual(sentinel.read_text(),'original')
            self.assertFalse((root/'failure.json').exists())

    def test_new_owned_failure_is_retained_without_a_success_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);out=root/'new'
            with mock.patch.object(audit,'_check',side_effect=RuntimeError('before measurement')):
                with self.assertRaises(RuntimeError):audit.check(root,out,'unused')
            self.assertIn('before measurement',(out/'failure.json').read_text())
            self.assertFalse((out/'result.json').exists())

class ReplayTests(unittest.TestCase):
    def setUp(self):
        import json
        import test_release_evidence as fixtures
        self.json=json;self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=pathlib.Path(self.temp.name);self.repository=pathlib.Path(__file__).resolve().parents[2]
        e=audit.evidence
        sources={role:e.source_at(self.repository,ref) for role,ref in {'candidate':'HEAD','baseline':e.BASELINE_COMMIT}.items()}
        self.report={'schema':'verify-scratch-check-v1','release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,'source_copies':True,
                     'sources':sources,'cases':{},'page_size':4096,'cpu_affinity':list(range(8)),
                     'native_sha256':e.digest(audit.native_template().encode()),'sampler_implementation_sha256':sampling.implementation_digest(),
                     'controller_sha256':'a'*64,'source_overlays':{},'harness_sha256':{},'overlay_sha256':{}}
        factory=fixtures.EvidenceTests(methodName='runTest');factory.sources=sources
        for role,source in sources.items():
            original={name:audit.subprocess.check_output(['git','show',source['commit']+':'+name],cwd=self.repository).decode() for name in ('src/lib.rs','src/core/verify.rs')}
            self.report['source_overlays'][role]=audit.bindings(original)
            self.report['harness_sha256'][role]=e.harness_digest(self.repository)
            self.report['overlay_sha256'][role]=audit.overlay_digest(self.repository,audit.render(self.repository))
            self.report['cases'][role]={}
            for profile in ('smoke','small_file'):
                raw={':'.join(str(r[k]) for k in ('operation','data_shards','parity_shards','shard_size')):r for r in factory.fixture_rows('A1' if role=='baseline' else 'B1',profile)}
                for case in audit.cases(profile):
                    samples=[]
                    for index,kind in enumerate(['warmup']*3+['sample']*9):
                        row=copy.deepcopy(raw[case]);row['iterations']=2
                        record=factory.record(row,profile,role,kind,index);record['iterations']=2;record['timed_ns']*=2
                        record['after']['process_minor_faults']=10000
                        samples.append(record)
                    key=profile+'/'+case;name=role+'-'+profile+'-'+case.replace(':','_')
                    data={'samples':samples,'scratch':ledger(case)}
                    self.write_member(role,key,name,data)
        self.save()

    def save(self):
        (self.root/'result.json').write_text(self.json.dumps(self.report))

    def write_member(self,role,key,name,data):
        e=audit.evidence;p=self.root/(name+'.json');p.write_text(self.json.dumps(data))
        log=self.root/(name+'.log')
        log.write_text(''.join('RSE_TIMING '+self.json.dumps({k:r[k] for k in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')})+'\nRSE_SAMPLE '+self.json.dumps(r)+'\n' for r in data['samples'])+audit.PREFIX+self.json.dumps(data['scratch'])+'\n')
        numeric=self.root/(name+'-journal.jsonl');numeric.write_bytes(log.read_bytes())
        self.report['cases'][role][key]=dict(audit.validate(data['scratch'],key.split('/')[1],2,4096,native=True),path=p.name,sha256=e.digest(p.read_bytes()),raw_log_sha256=e.digest(log.read_bytes()),numeric_journal_path=numeric.name,numeric_journal_sha256=e.digest(numeric.read_bytes()))

    def test_complete_replay_is_read_only_and_never_approves_performance(self):
        before={p.name:p.read_bytes() for p in self.root.iterdir()}
        result=audit.verify(self.root,self.repository)
        self.assertEqual(result['case_count'],14);self.assertFalse(result['release_approval'])
        self.assertEqual(before,{p.name:p.read_bytes() for p in self.root.iterdir()})

    def test_public_numeric_capsule_replays_without_private_build_logs(self):
        for path in self.root.glob('*.log'):path.unlink()
        report=audit.verify(self.root,self.repository)
        self.assertEqual(report['case_count'],14)
        self.assertFalse(report['release_approval'])
        (self.root/'diagnostic-status.json').write_text('{"status":"diagnostic_failed"}')
        with self.assertRaisesRegex(ValueError,'failed diagnostic'):audit.verify(self.root,self.repository)

    def test_source_hook_and_digest_changes_are_refused(self):
        original=copy.deepcopy(self.report)
        for change in (lambda d:d['source_overlays']['baseline'].pop('src/core/verify.rs'),
                       lambda d:d['overlay_sha256'].update(candidate='f'*64),
                       lambda d:d['cases']['candidate'].pop(next(iter(d['cases']['candidate']))),
                       lambda d:next(iter(d['cases']['candidate'].values())).update(sha256='0'*64)):
            self.report=copy.deepcopy(original);change(self.report);self.save()
            with self.assertRaises(ValueError):audit.verify(self.root,self.repository)

    def test_recomputed_digests_cannot_hide_backend_or_counter_mismatch(self):
        role='candidate';key=next(iter(self.report['cases'][role]));entry=self.report['cases'][role][key]
        name=entry['path'][:-5];data=self.json.loads((self.root/entry['path']).read_text());original=copy.deepcopy(data)
        for change in (lambda d:d['samples'][1]['rows'][0].update(backend_id='changed'),
                       lambda d:d['samples'][0]['after'].update(process_minor_faults=0)):
            data=copy.deepcopy(original);change(data);self.write_member(role,key,name,data);self.save()
            with self.assertRaises(ValueError):audit.verify(self.root,self.repository)

if __name__=='__main__':unittest.main()
