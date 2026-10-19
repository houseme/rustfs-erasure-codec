"""Reversible metadata insertions for the two supported stream implementations."""
MODULE='__rse_stream_core'

def method(text,declaration,transform):
    if text.count(declaration)!=1:raise ValueError('stream source declaration changed: '+declaration)
    start=text.index(declaration);end=text.index('\n    }',start)+6
    original=text[start:end];changed=original;edits=[]
    def edit(before,after):
        nonlocal changed
        if changed.count(before)!=1:raise ValueError('stream diagnostic boundary changed: '+before)
        changed=changed.replace(before,after,1);edits.append((before,after))
    transform(original,edit)
    restored=changed
    for before,after in reversed(edits):
        if restored.count(after)!=1:raise ValueError('non-reversible stream observation')
        restored=restored.replace(after,before,1)
    if restored!=original:raise ValueError('changed an original stream statement')
    return text[:start]+changed+text[end:]


def guard(api,variant):return '        let mut rse_probe = crate::'+MODULE+'::Guard::new("'+api+'", "'+variant+'");\n'


def spans(phase,groups,indent=8):
    """Record true initialized length separately from the codec-visible prefix."""
    lines=[]
    for role,name,kind,visible in groups:
        lines.append(' '*indent+f'for (rse_i, rse_b) in {name}.iter().enumerate() {{')
        if kind=='option':
            lines.extend([' '*(indent+4)+'if let Some(rse_b) = rse_b {',
                ' '*(indent+8)+f'rse_probe.span("{phase}", {role}, rse_i, rse_b.as_ptr(), rse_b.len(), rse_b.capacity(), rse_b.len(), true);',
                ' '*(indent+4)+'} else {',
                ' '*(indent+8)+f'rse_probe.span::<u8>("{phase}", {role}, rse_i, core::ptr::null(), 0, 0, 0, false);',
                ' '*(indent+4)+'}'])
        else:
            b='rse_b.bytes' if kind=='read' else 'rse_b'
            v='rse_b.block_len' if kind=='read' else visible
            lines.append(' '*(indent+4)+f'rse_probe.span("{phase}", {role}, rse_i, {b}.as_ptr(), {b}.len(), {b}.capacity(), {v}, true);')
        lines.append(' '*indent+'}')
    return '\n'.join(lines)+'\n'


def source_patch(text):
    current='struct StreamReadBuffer {' in text
    def patch_encode(original,edit):
        first='        let mut data_bufs:'
        edit(first,guard('encode_stream','retained-prefix' if current else 'vec-views')+first)
        groups=[(0,'data_bufs','read' if current else 'vec','0'),(1,'parity_bufs','vec','rse_b.len()')]
        edit('        loop {',spans('reserved',groups)+'        loop {\n            rse_probe.next_block();')
        groups[0]=(0,'data_bufs','read' if current else 'vec','actual_len')
        marker='            if all_eof {'
        edit(marker,spans('read',groups,12)+marker)
        marker='            self.encode_sep_opt(' if current else '            // Encode (parallel codec).'
        edit(marker,spans('ready',groups,12)+marker)
        marker='            // Write parity using the selected stream I/O mode.'
        edit(marker,spans('coded',groups,12)+marker)
        edit('        Ok(())','        rse_probe.complete(None);\n        Ok(())')
    text=method(text,'    pub fn encode_stream(',patch_encode)
    def patch_verify(original,edit):
        first='        let mut bufs:'
        edit(first,guard('verify_stream','retained-scratch' if current else 'per-block-verify-par')+first)
        groups=[(0,'bufs','read' if current else 'vec','0')]
        if current:groups.append((1,'verify_bufs','vec','rse_b.len()'))
        edit('        loop {',spans('reserved',groups)+'        loop {\n            rse_probe.next_block();')
        groups[0]=(0,'bufs','read' if current else 'vec','actual_len')
        marker='            if all_eof {'
        edit(marker,spans('read',groups,12)+marker)
        edit('            let valid = self',spans('ready',groups,12)+'            let valid = self')
        edit('            if !valid {',spans('coded',groups,12)+'            if !valid {')
        edit('                return Ok(false);','                rse_probe.complete(Some(false));\n                return Ok(false);')
        edit('        Ok(true)','        rse_probe.complete(Some(true));\n        Ok(true)')
    text=method(text,'    pub fn verify_stream(',patch_verify)
    def patch_reconstruct(original,edit):
        first='        let mut bufs:'
        edit(first,guard('reconstruct_stream','planned-retained' if current else 'option-transfer')+first)
        groups=[(0,'bufs','vec','rse_b.len()')]
        if not current:groups.append((1,'reconstruct_bufs','option',None))
        if current:
            marker='        let (all_eof, mut actual_len) = read_present_streams_with_mode('
            edit(marker,spans('reserved',groups)+'        rse_probe.next_block();\n'+marker)
            marker='        if all_eof {\n            return Ok(());\n        }'
            edit(marker,spans('read',groups)+ '        if all_eof {\n            rse_probe.complete(None);\n            return Ok(());\n        }')
            marker='            plan.reconstruct_block(&mut bufs, actual_len, &data_rows, &parity_rows);'
            edit(marker,spans('ready',groups,12)+marker+'\n'+spans('coded',groups,12))
            marker='            let (all_eof, next_len) = read_present_streams_with_mode('
            edit(marker,'            rse_probe.next_block();\n'+marker)
            marker='            if all_eof {'
            edit(marker,spans('read',groups,12)+marker)
        else:
            edit('        loop {',spans('reserved',groups)+'        loop {\n            rse_probe.next_block();')
            marker='            if all_eof {'
            edit(marker,spans('read',groups,12)+marker)
            marker='            self.reconstruct(&mut reconstruct_bufs)'
            edit(marker,spans('ready',groups,12)+marker)
            marker='            // Write recovered data into missing shards\' cursors.'
            edit(marker,spans('coded',groups,12)+marker)
            marker='            for idx in 0..total {\n                if let Some(buf) = reconstruct_bufs[idx].take() {\n                    bufs[idx] = buf;\n                }\n            }'
            edit(marker,marker.replace('bufs[idx] = buf;','bufs[idx] = buf;\n                    rse_probe.replaced(0, idx);')+'\n'+spans('recycled',groups,12))
        # The early current empty-stream return was already completed above.
        edit('\n        Ok(())\n','\n        rse_probe.complete(None);\n        Ok(())\n')
    return method(text,'    pub fn reconstruct_stream_from<' if current else '    pub fn reconstruct_stream(',patch_reconstruct)


def verify_par_patch(text):
    def change(original,edit):
        marker='        let mut scratch: SmallVec<[F::Elem; VERIFY_INLINE_SCRATCH_ELEMS]> ='
        edit(marker,guard('verify_par','smallvec-per-block')+marker)
        marker='        scratch.resize(scratch_len, F::zero());'
        edit(marker,'        rse_probe.span("reserved", 0, 0, scratch.as_ptr(), scratch.len(), scratch.capacity(), 0, true);\n'+marker+'\n        rse_probe.span("initialized", 0, 0, scratch.as_ptr(), scratch.len(), scratch.capacity(), scratch.len(), true);\n        let rse_scratch_range = (scratch.as_ptr(), scratch.len(), scratch.capacity());')
        marker='        self.verify_with_buffer_par(slices, &mut buffer_views)'
        edit(marker,'        let rse_result = self.verify_with_buffer_par(slices, &mut buffer_views);\n        rse_probe.span("coded", 0, 0, rse_scratch_range.0, rse_scratch_range.1, rse_scratch_range.2, rse_scratch_range.1, true);\n        if let Ok(valid) = rse_result { rse_probe.complete(Some(valid)); }\n        rse_result')
    return method(text,'    pub fn verify_par<T>',change)
