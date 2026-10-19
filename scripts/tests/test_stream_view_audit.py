"""Structural view fixtures never establish performance or release acceptance."""
import copy
import pathlib
import sys
import unittest
from unittest import mock
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import stream_view_audit as audit
import release_sampling as sampling
import release_evidence as evidence
import diagnose_fixture_layout as inventory
import test_release_evidence as fixtures


def view_fixture(case,iterations=2,observation=0,page_size=4096):
    operation,k,m,size,block=case.split(':');k,m,size,block=map(int,(k,m,size,block));count=k if operation=='encode_stream' else k+m
    events=[]
    def add(call,role,index,length,capacity,amount=0):
        events.append({'call':call,'role':role,'index':index,'len':length,'capacity':capacity,
          'offsets':[16,16,16] if (capacity>0 if capacity is not None else length>0) else None,
          'bytes':amount,'same_address_as_previous_view':None})
    for index in range(count):add(0,'retained_source',index,size,size)
    for call in range(1,iterations+1):
        if operation=='reconstruct_stream':
            for role in ('cursor_before_missing','cursor_missing','cursor_after_recovery'):
                for index in range(count):
                    missing=role=='cursor_missing' and index in (0,k)
                    add(call,role,index,0 if missing else size,0 if missing else size)
            continue
        for index in range(count):
            remaining=size
            while remaining:
                length=min(block,remaining)
                add(call,'source_read_view',index,remaining,None,length)
                add(call,'read_scratch_view',index,block,None,length)
                remaining-=length
            add(call,'source_read_view',index,0,None)
            add(call,'read_scratch_view',index,block,None)
        if operation=='encode_stream':
            for index in range(m):
                used=0
                while used<size:
                    n=min(block,size-used)
                    add(call,'parity_write_view',index,n,None,n)
                    add(call,'writer_before',index,used,size)
                    add(call,'writer_after',index,used+n,size,n)
                    used+=n
    windows=[];clock=0;counter=0
    def window(call,phase,index=None,amount=0):
        nonlocal clock,counter
        clock+=10;counter+=2
        w={'call':call,'phase':phase,'index':index,'bytes':amount,'start_ns':clock,'end_ns':clock+5,'before':[counter]*4,'after':[counter+1]*4}
        clock+=10;counter+=2;windows.append(w)
    for call in range(1,iterations+1):
        start=clock;before=counter
        if operation=='encode_stream':
            window(call,'reserve',amount=m*size)
            for event in events:
                if event['call']==call and event['role']=='parity_write_view':window(call,'append',event['index'],event['bytes'])
            window(call,'drop',amount=m*size)
        clock+=10;counter+=2
        windows.append({'call':call,'phase':'call','index':None,'bytes':0,'start_ns':start,'end_ns':clock,'before':[before]*4,'after':[counter]*4})
    return {'schema':'stream-buffer-views-v2','observation':observation,'iterations':iterations,'calls':iterations,'completed':iterations,'page_size':page_size,'overflow':0,'release_approval':False,'events':events,'windows':windows}


class StreamViewTests(unittest.TestCase):
    def test_all_fifteen_case_shapes_require_complete_io_and_cursor_lifecycles(self):
        for case in evidence.expected_cases('stream'):
            audit.validate(view_fixture(case),case,2,4096,0)

    def test_missing_eof_transfer_or_completion_and_overflow_are_rejected(self):
        case='encode_stream:4:2:65536:65536'
        for change in (lambda d:d.update(completed=1),lambda d:d.update(overflow=1),
                       lambda d:d['events'].pop(),lambda d:d['events'].append(dict(d['events'][-1],address=123)),
                       lambda d:d.update(observation=1)):
            data=view_fixture(case);change(data)
            with self.assertRaises(ValueError):audit.validate(data,case,2,4096,0)
        data=view_fixture(case)
        data['events']=[e for e in data['events'] if not(e['role']=='source_read_view' and e['bytes']==0)]
        with self.assertRaises(ValueError):audit.validate(data,case,2,4096,0)

    def test_borrowed_capacity_writer_reservation_and_missing_storage_cannot_be_faked(self):
        for case,role,change in (
            ('encode_stream:4:2:65536:65536','read_scratch_view',lambda e:e.update(capacity=e['len'])),
            ('encode_stream:4:2:65536:65536','writer_after',lambda e:e.update(capacity=e['capacity']+1)),
            ('reconstruct_stream:4:2:65536:65536','cursor_missing',lambda e:e.update(capacity=65536,offsets=[16,16,16]))):
            data=view_fixture(case);event=next(e for e in data['events'] if e['role']==role);change(event)
            with self.assertRaises(ValueError):audit.validate(data,case,2,4096,0)

    def test_observed_source_must_match_retained_fixture(self):
        case='encode_stream:4:2:65536:65536';data=view_fixture(case)
        source=[{'len':65536,'capacity':65536,'offset_64':16,'offset_4096':16,'offset_page':16} for _ in range(4)]
        audit.validate(data,case,2,4096,0,source)
        source[0].update(offset_64=32,offset_4096=32,offset_page=32)
        with self.assertRaisesRegex(ValueError,'retained caller'):audit.validate(data,case,2,4096,0,source)

    def test_diagnostic_entries_are_explicit_and_normalization_keeps_originals(self):
        helper=fixtures.EvidenceTests(methodName='runTest');helper.setUp();self.addCleanup(helper.doCleanups)
        row=helper.fixture_rows('A1','stream')[0];sample=helper.record(row,'stream','baseline','sample',3)
        operation=sample['case'].split(':')[0]
        for field in ('fixture','workload'):sample[field]['call_entry']=audit.ENTRIES[operation]
        normalized=audit.underlying_records([sample])[0]
        self.assertNotEqual(normalized['fixture']['call_entry'],sample['fixture']['call_entry'])
        sampling.validate_workload(normalized,'stream')
        with self.assertRaises(ValueError):sampling.validate_workload(sample,'stream')

    def test_decorator_does_not_change_the_formal_renderer_or_library(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        before=sampling.render(root);traced=audit.render(root)
        self.assertEqual(before,sampling.render(root))
        self.assertNotIn('stream_views.rs',before)
        self.assertIn('stream_views.rs',traced)
        self.assertLess(traced['stream_views.rs'].index('fn read_bounded'),traced['stream_views.rs'].index('mod tests {'))
        engine=(sampling.TEMPLATES/'engine.rs').read_text()
        for declaration in ('fn read_bounded','fn process_faults'):
            self.assertIn(sampling.block(engine,declaration),traced['stream_views.rs'])
        for name in before:
            if name not in ('root.rs','stream.rs'):self.assertEqual(before[name],traced[name])
        self.assertIn('super::stream_views::cursors(&cursors, 8)',traced['stream.rs'])
        self.assertIn('super::stream_views::drop_outputs(writers); drop(readers);',traced['stream.rs'])
        import diagnose_cpu_placement
        placement=diagnose_cpu_placement.render(root,allocation_check=True)
        self.assertNotIn('stream_views::',placement['root.rs'])
        self.assertNotIn('stream_views.rs',placement)

    def test_window_partition_preserves_all_calls_and_rejects_incomplete_or_overlapping_io(self):
        case='encode_stream:4:2:65536:65536'
        original=view_fixture(case)
        for mutation in (
            lambda v:v['windows'].pop(),
            lambda v:v['windows'][1].update(start_ns=0),
            lambda v:v['windows'][1].update(index=9),
            lambda v:v['windows'][1].update(bytes=1),
            lambda v:v['windows'][1].update(after=[0]*4),
            lambda v:v['windows'][1].update(before=None),
            lambda v:v['windows'][1].update(address=123),
            lambda v:v['windows'].append(v['windows'][0]),
        ):
            value=copy.deepcopy(original);mutation(value)
            with self.assertRaises(ValueError):audit.validate(value,case,2,4096,0)
        report=audit.summarize_windows(original)
        self.assertEqual(len(report),2)
        for call in report:
            groups=call['windows']
            for field,value in groups['call'].items():
                self.assertEqual(value,sum(groups[p][field] for p in ('reserve','append','drop','outside_writer_windows')))
        # Non-Linux counters stay unavailable, never fabricated as zero.
        for window in original['windows']:window.update(before=None,after=None)
        audit.validate(original,case,2,4096,0)
        self.assertIsNone(audit.summarize_windows(original)[0]['windows']['call']['process_minor_faults'])

    def inventory_fixture(self):
        helper=fixtures.EvidenceTests(methodName='runTest');helper.setUp();self.addCleanup(helper.doCleanups)
        phases={};views={}
        for phase in evidence.PHASES:
            role='baseline' if phase.startswith('A') else 'candidate';phases[phase]=[];views[phase]=[]
            for row in helper.fixture_rows(phase,'stream'):
                for index in range(12):
                    record=helper.record(row,'stream',role,'warmup' if index<3 else 'sample',index)
                    record['iterations']=2;record['rows'][0]['iterations']=2;record['timed_ns']*=2
                    operation=record['case'].split(':')[0]
                    for field in ('fixture','workload'):record[field]['call_entry']=audit.ENTRIES[operation]
                    phases[phase].append(record);views[phase].append(view_fixture(record['case'],observation=index))
        return helper,phases,views

    def test_cross_phase_backend_io_and_constructor_budget_mismatch_are_rejected(self):
        helper,original,views=self.inventory_fixture()
        for field in ('features','backend_id','stream_io_mode','constructor_budget'):
            phases=copy.deepcopy(original)
            for row in phases['B1']:
                if field=='constructor_budget':row['placement'][field]=7
                else:row['rows'][0][field]='serial' if field=='stream_io_mode' else 'std' if field=='features' else 'changed'
            with self.subTest(field=field):
                with self.assertRaises(ValueError):inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)

    def test_four_phase_inventory_reports_differences_without_performance_approval(self):
        helper,phases,views=self.inventory_fixture()
        report=inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)
        self.assertEqual(report['observations'],720)
        self.assertEqual(report['case_count'],15)
        self.assertFalse(report['release_approval']);self.assertFalse(report['coverage']['internal_verify_scratch'])
        self.assertFalse(report['layout_mismatches'])
        event=next(e for e in views['B1'][0]['events'] if e['role']=='read_scratch_view');event['offsets']=[32,32,32]
        report=inventory.analyze_stream_views(phases,views,helper.sources,list(range(8)),4096,2)
        self.assertTrue(report['layout_mismatches'])
        with self.assertRaises(ValueError):inventory.analyze_stream_views(phases,dict(views,A2=[]),helper.sources,list(range(8)),4096,2)


if __name__=='__main__':unittest.main()
