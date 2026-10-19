"""Software profiles conserve every event and never infer throughput or hidden frames."""
import copy
import io
import json
import pathlib
import sys
import unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import perf_stack_audit as audit

SOURCE='''fn prepare_read_buffer() {
    match n {
        core::cmp::Ordering::Less => buf.resize(max_len, 0),
    }
}
pub fn encode_stream() {
    for buf in parity_bufs.iter_mut() {
        buf.resize(actual_len, 0);
    }
    write_block_with_mode(
    );
    }
pub fn verify_stream() {}
'''


def sample(time='1.000001',period=1,event='minor-faults:u',symbol='[unknown]',source=None,caller=None):
    text=f'{time}: {period} {event}:\n\t abcdef012345 {symbol} (/private/machine/lib/libc.so.6)\n'
    text+='  '+(source or 'libc.so.6[abcd]')+'\n'
    if caller:text+='\t 123456789abc '+caller+' (/private/machine/codec)\n  ??:0\n'
    return text+'\n'


class PerfStackTests(unittest.TestCase):
    def setUp(self):self.anchors=audit.source_anchors(SOURCE)
    def run_audit(self,text,count=1,event='minor-faults:u',records=None):
        return audit.audit(io.StringIO(text),records or {'PERF_RECORD_SAMPLE':count},self.anchors,event)

    def test_actual_periods_unknown_leaves_and_every_stack_are_preserved(self):
        text=sample(source=f'/private/project/src/core/stream.rs:{self.anchors["read_initialization"]}')
        text+=sample('1.000002',source=f'/private/project/src/core/stream.rs:{self.anchors["parity_initialization"]}')
        text+=sample('1.000003',period=1009,symbol='unclassified_function')
        result=self.run_audit(text,3)
        self.assertEqual(result['samples'],3);self.assertEqual(result['period_sum'],1011)
        self.assertEqual(result['actual_period_histogram'],[{'period':1,'samples':2},{'period':1009,'samples':1}])
        self.assertEqual(result['unresolved_leaf_samples'],2)
        self.assertEqual(sum(r['samples'] for r in result['roles'].values()),3)
        self.assertEqual(sum(r['period_sum'] for r in result['roles'].values()),1011)
        self.assertEqual(set(result['roles']),{'read_scratch_initialization','parity_scratch_initialization','other_or_unattributed'})
        exported=json.dumps(result)
        for secret in ('/private/','abcdef012345','123456789abc','libc.so.6[abcd]'):self.assertNotIn(secret,exported)
        self.assertFalse(result['release_approval']);self.assertIsNone(result['candidate_attribution'])

    def test_scope_requires_positive_source_or_codec_symbol_and_explicit_free(self):
        ns='rustfs_erasure_codec::core::stream::'
        cases=[(ns+'read_one_stream','stream_read_path'),(ns+'write_block','stream_write_path'),
               (ns+'ReedSolomon::encode_stream','other_stream_encode'),
               ('different_crate::read_one_stream','other_or_unattributed')]
        for caller,role in cases:
            with self.subTest(caller=caller):self.assertIn(role,self.run_audit(sample(caller=caller))['roles'])
        self.assertIn('stream_encode_cleanup',self.run_audit(sample(symbol='__libc_free',caller=ns+'ReedSolomon::encode_stream'))['roles'])
        self.assertIn('other_stream_encode',self.run_audit(sample(symbol='core::ptr::drop_in_place<Guard>',caller=ns+'ReedSolomon::encode_stream'))['roles'])

    def test_kernel_unknown_frames_and_cpu_periods_remain_explicit(self):
        result=self.run_audit(sample(event='cpu-clock',period=2000011,caller='[unknown]'),event='cpu-clock')
        self.assertEqual(result['unresolved_leaf_samples'],1)
        self.assertEqual(result['period_sum'],2000011)
        self.assertEqual(len(result['stacks'][0]['frame_ids']),2)
        self.assertIn('other_or_unattributed',result['roles'])

    def test_missing_or_malformed_events_and_nonmonotonic_input_are_rejected(self):
        examples=[('',1),(sample(),2),
                  ('\t abcdef orphan (libc.so.6)\n',1),(sample(period=0),1),
                  (sample(event='cpu-clock'),1),(sample('1.000002')+sample('1.000001'),2),
                  (sample().replace('libc.so.6[abcd]','unrecognized source'),1),
                  (sample().replace('libc.so.6[abcd]','/private/file.rs:10\n  /private/file.rs:11'),1)]
        for text,count in examples:
            with self.subTest(text=text):
                with self.assertRaises(ValueError):self.run_audit(text,count)

    def test_loss_throttle_and_invalid_raw_counts_are_never_accepted(self):
        for bad in audit.BAD_RECORDS:
            with self.assertRaisesRegex(ValueError,'lost or throttled'):
                self.run_audit(sample(),records={'PERF_RECORD_SAMPLE':1,bad:1})
        for counts in ({'PERF_RECORD_SAMPLE':True},{'PERF_RECORD_SAMPLE':0},{'PERF_RECORD_SAMPLE':1,'PERF_RECORD_COMM':-1}):
            with self.assertRaises(ValueError):self.run_audit(sample(),records=counts)

    def test_inline_frames_keep_their_source_context_and_count_once(self):
        location=self.anchors['parity_initialization']
        text=sample(symbol='memset',caller='rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream')
        text=text.replace('  ??:0','  /private/src/core/stream.rs:'+str(location))
        result=self.run_audit(text)
        self.assertEqual(result['samples'],1);self.assertEqual(result['unresolved_leaf_samples'],0)
        self.assertEqual(result['roles']['parity_scratch_initialization']['samples'],1)
        self.assertEqual(result['frames'][result['stacks'][0]['frame_ids'][1]]['source'],'src/core/stream.rs')

    def test_real_perf_inline_without_dso_and_full_source_path_are_retained(self):
        line=self.anchors['read_initialization']
        text='1.000001: 1 minor-faults:u:\n'
        text+='\t abcdef012345 __memset_avx512 (libc.so.6)\n  libc.so.6[1234]\n'
        text+='\t 123456789abc resize<u8>\n  /private/tool/library/alloc/src/vec/mod.rs:2048 (inlined)\n'
        text+='\t 123456789abc prepare_read_buffer\n  /private/project/src/core/stream.rs:'+str(line)+' (inlined)\n\n'
        result=self.run_audit(text)
        self.assertIn('read_scratch_initialization',result['roles'])
        frames=[result['frames'][i] for i in result['stacks'][0]['frame_ids']]
        self.assertEqual(len(frames),3)
        self.assertIsNone(frames[1]['module'])
        self.assertEqual(frames[1]['source'],'library/alloc/src/vec/mod.rs')
        self.assertEqual(frames[2]['line'],line)
        self.assertNotIn('/private/',json.dumps(result))

    def test_relative_debug_paths_and_generic_type_arguments_do_not_confuse_scope(self):
        self.assertEqual(audit.public_source('src/core/stream.rs:33 (inlined)'),('src/core/stream.rs',33))
        self.assertEqual(audit.public_source('/private/.cargo/registry/src/index/rayon-1.12.0/src/iter.rs:9'),('registry/rayon-1.12.0/src/iter.rs',9))
        self.assertEqual(audit.public_source('src/backend/gfni.rs:40'),('src/backend/gfni.rs',40))
        caller='rayon::run<rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream>'
        self.assertIn('other_or_unattributed',self.run_audit(sample(symbol='free',caller=caller))['roles'])
        caller='rustfs_erasure_codec::core::stream::helper<some_crate::read_one_stream>'
        self.assertIn('other_or_unattributed',self.run_audit(sample(caller=caller))['roles'])
        self.assertEqual(audit.function_name('rustfs_erasure_codec::core::stream::<impl ReedSolomon<Field>>::encode_stream<Vec<u8>>'),'rustfs_erasure_codec::core::stream::::encode_stream')
        text=sample(symbol='resize<u8>',source='src/core/stream.rs:'+str(self.anchors['parity_initialization'])+' (inlined)')
        self.assertIn('parity_scratch_initialization',self.run_audit(text)['roles'])

    def test_shared_frames_are_interned_without_losing_stack_counts(self):
        text=sample()+sample('1.000002')+sample('1.000003',period=2)
        result=self.run_audit(text,3)
        self.assertEqual(len(result['frames']),1)
        self.assertEqual(len(result['stacks']),1)
        self.assertEqual(result['stacks'][0]['frame_ids'],[0])
        self.assertEqual(result['stacks'][0]['samples'],3)
        self.assertEqual(result['stacks'][0]['period_sum'],4)

    def test_native_aggregate_statistics_keep_digits_and_do_not_double_count_event_sections(self):
        text="""Aggregated stats:
 TOTAL events: 6
 MMAP2 events: 2 (33.3%)
 SAMPLE events: 4 (66.7%)
cpu-clock stats:
 SAMPLE events: 4
"""
        counts=audit.parse_record_stats(text)
        self.assertEqual(counts,{'PERF_RECORD_MMAP2':2,'PERF_RECORD_SAMPLE':4})
        for value in (text.replace('TOTAL events: 6','TOTAL events: 7'),text.replace('SAMPLE events: 4 (66.7%)','unknown input'),text.replace('SAMPLE events: 4 (66.7%)','MMAP2 events: 4 (66.7%)'),''):
            with self.assertRaises(ValueError):audit.parse_record_stats(value)

    def test_writer_callsite_and_kernel_release_require_exact_source_context(self):
        text=sample(symbol='unknown libc routine',source='src/core/stream.rs:'+str(self.anchors['writer_call']))
        self.assertIn('stream_write_path',self.run_audit(text)['roles'])
        text=sample(symbol='zap_pte_range',source=None,caller='rustfs_erasure_codec::core::stream::ReedSolomon::encode_stream')
        text=text.replace('/private/machine/lib/libc.so.6','[kernel.kallsyms]').replace('  ??:0','  src/core/stream.rs:'+str(self.anchors['encode_return']))
        self.assertIn('stream_encode_cleanup',self.run_audit(text)['roles'])
        exported=self.run_audit(sample().replace('libc.so.6)','rse_release_sampling-12345678)'))
        self.assertEqual(exported['frames'][0]['module'],'codec-worker')

    def test_valid_samples_with_no_unwind_are_explicitly_retained_as_unknown(self):
        result=self.run_audit('1.000001: 1 minor-faults:u:\n\n')
        self.assertEqual(result['samples'],1)
        self.assertEqual(result['unresolved_leaf_samples'],1)
        self.assertEqual(result['empty_callchain_samples'],1)
        self.assertEqual(result['frames'][0]['symbol'],'[unresolved-callchain]')
        self.assertIn('other_or_unattributed',result['roles'])

    def test_zero_exit_corrupted_or_lossy_perf_capture_cannot_be_accepted(self):
        good='[ perf record: Captured and wrote 1 MB (10 samples) ]'
        self.assertFalse(audit.validate_recording_log(good,0)['fatal_diagnostics'])
        for message,code in ((good,1),('',0),(good+'\nfailed to write perf data, error: Bad address',0),
                             (good+'\nData corruption detected\nCouldn\'t decompress data',0),
                             (good+'\nlost 2 chunks!',0)):
            with self.assertRaisesRegex(ValueError,'unusable perf capture'):audit.validate_recording_log(message,code)

    def test_current_source_anchors_are_exact_and_missing_or_duplicate_statements_fail(self):
        root=pathlib.Path(__file__).resolve().parents[2]
        current=(root/'src/core/stream.rs').read_text();anchors=audit.source_anchors(current)
        lines=current.splitlines()
        self.assertIn('buf.resize(max_len, 0)',lines[anchors['read_initialization']-1])
        self.assertIn('buf.resize(actual_len, 0);',lines[anchors['parity_initialization']-1])
        for changed in (SOURCE.replace('buf.resize(actual_len, 0);',''),SOURCE+SOURCE):
            with self.assertRaises(ValueError):audit.source_anchors(changed)


if __name__=='__main__':unittest.main()
