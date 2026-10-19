"""Bind memory-management evidence to the sampler's actual deferred output order."""
import copy
import io
import json
import pathlib
import sys
import unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import stream_allocator_audit as audit


def fixture():
    rows=[{'case':'encode_stream:10:4:16777216:4194304','index':i,'kind':'warmup' if i<3 else 'sample','iterations':6} for i in range(12)]
    lines=[]
    def call(time,body,tid=7):lines.append(f'{tid} {time:.6f} {body} <0.000001>\n')
    def write(time,prefix,row,tid=7):
        text=prefix+json.dumps(row)+'\n';call(time,f'write(1, {json.dumps(text)}, {len(text)}) = {len(text)}',tid)
    call(1,'brk(NULL) = 0x1000')
    call(2,'mmap(NULL, 16384, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x10000')
    call(3,'munmap(0x10000, 4096) = 0')
    call(4,'munmap(0x13000, 4096) = 0')
    for i,row in enumerate(rows):
        if i==1:
            call(20,'madvise(0x11000, 4096, MADV_DONTNEED) = 0')
            call(21,'mmap(0x11000, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0) = 0x11000')
            call(22,'munmap(0x11000, 4096) = -1 EINVAL (Invalid argument)')
            call(23,'madvise(0x11000, 4096, MADV_DONTNEED) = 0')
            call(24,'brk(0x3000) = 0x3000')
            call(25,'brk(0x2000) = 0x2000')
        write(10+i*20,'RSE_TIMING ',row)
    # Actual protocol: every immediate timing row precedes deferred sample export.
    for i,row in enumerate(rows):write(300+i,'RSE_SAMPLE ',row)
    call(400,'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x90000',tid=9)
    return rows,lines


class AllocatorAuditTests(unittest.TestCase):
    def test_all_markers_mapping_splits_fixed_replacement_and_failed_free_are_preserved(self):
        rows,lines=fixture();report=audit.audit_trace(lines,rows)
        self.assertEqual(report['observations'],12)
        self.assertEqual(report['other_task_syscalls'],1)
        group=report['groups']['observation_1']
        self.assertEqual(group['successful_madvise_bytes']['MADV_DONTNEED'],8192)
        self.assertEqual(group['brk_grow_bytes'],8192)
        self.assertEqual(group['brk_shrink_bytes'],4096)
        events=report['events'];advice=[e for e in events if e['kind']=='madvise']
        self.assertEqual(advice[0]['matches'][0]['offset'],4096)
        self.assertEqual(advice[1]['matches'][0]['mapping_id'],1)
        self.assertTrue(any(e['kind']=='munmap' and not e['success'] for e in events))
        self.assertFalse(report['release_approval']);self.assertIsNone(report['candidate_attribution'])
        exported=json.dumps(report)
        for value in ('0x11000','0x10000','"tid"','"time"'):self.assertNotIn(value,exported)

    def test_resumed_syscalls_are_reconstructed_without_double_counting(self):
        lines=['7 1.000000 mmap(NULL, 4096, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0 <unfinished ...>\n',
               '8 1.000001 --- SIGCHLD {si_signo=SIGCHLD} ---\n',
               '7 1.000002 <... mmap resumed>) = 0x1000 <0.000002>\n']
        calls,signals=audit.decode_calls(lines)
        self.assertEqual(len(calls),1);self.assertEqual(signals,1)
        self.assertEqual(calls[0]['duration_ns'],2000)
        self.assertEqual(calls[0]['time'],1000000000)
        for changed in (lines[:-1],[lines[-1]],lines[:1]+['7 1.000001 brk(NULL) = 0x1000 <0.000001>\n']):
            with self.assertRaises(ValueError):audit.decode_calls(changed)

    def test_missing_reordered_or_changed_numeric_markers_are_rejected(self):
        rows,lines=fixture()
        for changed_rows,changed_lines in ((rows[:-1],lines),(rows,lines[:-3])):
            with self.assertRaises(ValueError):audit.audit_trace(changed_lines,changed_rows)
        changed=copy.deepcopy(rows);changed[2]['iterations']=7
        with self.assertRaisesRegex(ValueError,'journal mismatch'):audit.audit_trace(lines,changed)
        # Swapping a timing and a deferred sample changes the real protocol order.
        changed_lines=list(lines);timing=next(i for i,l in enumerate(lines) if 'RSE_TIMING' in l);sample=next(i for i,l in enumerate(lines) if 'RSE_SAMPLE' in l)
        a,b=changed_lines[timing],changed_lines[sample]
        changed_lines[timing]=a[:a.index('write(')]+b[b.index('write('):]
        changed_lines[sample]=b[:b.index('write(')]+a[a.index('write('):]
        with self.assertRaises(ValueError):audit.audit_trace(changed_lines,rows)

    def test_failed_vm_calls_and_null_addresses_do_not_fabricate_residency(self):
        rows,lines=fixture()
        lines.insert(4,'7 5.000000 madvise(NULL, 4096, MADV_DONTNEED) = -1 ENOMEM (Cannot allocate memory) <0.000001>\n')
        lines.insert(5,'7 6.000000 brk(0x9000) = 0x1000 <0.000001>\n')
        result=audit.audit_trace(lines,rows)
        self.assertTrue(any(e['kind']=='madvise' and not e['success'] for e in result['events']))
        self.assertTrue(any(e['kind']=='brk' and e['request_satisfied'] is False for e in result['events']))
        self.assertFalse(result['coverage']['physical_residency_observed'])
        self.assertFalse(result['coverage']['mprotect_observed'])
        self.assertTrue(any(e.get('protection_at_mmap')=='PROT_NONE' for e in result['events']))

    def test_split_stdout_is_reassembled_and_truncation_is_rejected(self):
        rows,lines=fixture();index=next(i for i,l in enumerate(lines) if 'RSE_TIMING' in l)
        original=lines[index];body=original[original.index('write('):]
        call= audit.CALL.fullmatch(body.rstrip())
        import ast,re
        match=re.fullmatch(r'1, ("(?:\\.|[^"\\])*"), (\d+)',call[2]);payload=ast.literal_eval(match[1]);cut=10
        prefix=original[:original.index('write(')]
        parts=[prefix+f'write(1, {json.dumps(v)}, {len(v)}) = {len(v)} <0.000001>\n' for v in (payload[:cut],payload[cut:])]
        changed=lines[:index]+parts+lines[index+1:]
        self.assertEqual(audit.audit_trace(changed,rows)['observations'],12)
        changed[index]=parts[0].replace(f'= {cut} ',f'= {cut+10000} ')
        with self.assertRaisesRegex(ValueError,'truncated'):audit.audit_trace(changed,rows)


if __name__=='__main__':unittest.main()
