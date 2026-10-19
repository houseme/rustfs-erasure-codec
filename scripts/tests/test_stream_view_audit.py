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
    return {'schema':'stream-buffer-views-v1','observation':observation,'iterations':iterations,'calls':iterations,'completed':iterations,'page_size':page_size,'overflow':0,'release_approval':False,'events':events}


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
        for name in before:
            if name not in ('root.rs','stream.rs'):self.assertEqual(before[name],traced[name])
        self.assertIn('super::stream_views::cursors(&cursors, 8)',traced['stream.rs'])
        self.assertIn('drop(writers); drop(readers);',traced['stream.rs'])

    def test_four_phase_inventory_reports_differences_without_performance_approval(self):
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
