"""Reversible decode-work views, including cached leases and partial chunks."""
import release_sampling as s
import stream_core_source as edits

LIB='crate::__rse_dynamic_buffers::'
FILES=('src/core/leopard_gf8/work.rs','src/core/leopard_gf8/decode.rs',
       'src/core/leopard_gf16/work.rs','src/core/leopard_gf16/decode.rs')


def top(text,declaration,transform):
    original=s.block(text,declaration);changed=original;changes=[]
    def edit(old,new,count=1):
        nonlocal changed
        if changed.count(old)!=count:raise ValueError('Leopard diagnostic source boundary changed: '+old)
        changed=changed.replace(old,new);changes.append((old,new,count))
    transform(edit)
    restored=changed
    for old,new,count in reversed(changes):
        if restored.count(new)!=count:raise ValueError('non-reversible Leopard observer')
        restored=restored.replace(new,old)
    if restored!=original:raise ValueError('Leopard observer changed original work')
    return text.replace(original,changed,1)


def work(text,gf16):
    typename='FlatWork16' if gf16 else 'FlatWork';length='len_elems' if gf16 else 'len'
    marker='impl '+typename+' {'
    if text.count(marker)!=1:raise ValueError('Leopard work implementation changed')
    # Decoder constructors zero the complete work allocation; reuse retains
    # initialized elements. No slice/reference to uninitialized data is created.
    helper='''
    pub(crate) fn __rse_decode_view(&self, stage: &'static str, active: usize) {
        '''+LIB+'''work("leopard_work",stage,self.ptr,self.'''+length+''', [self.lanes,active,self.lane_len]);
    }
'''
    return text.replace(marker,marker+helper,1)


def vector(name,role,stage,indent):
    return ' '*indent+LIB+f'span("{role}","{stage}",0,{name}.as_ptr(),{name}.len(),Some({name}.capacity()),"heap",true);\n'


def locator(stage,indent=4):
    return ' '*indent+LIB+f'span("leopard_locator","{stage}",0,err_locs.as_ptr(),err_locs.len(),Some(err_locs.len()),"inline",true);\n'


def gf8(text):
    current='struct DecodeBuffers {' in text
    declaration='fn reconstruct_with_resources(' if current else 'pub(crate) fn reconstruct_with_tables('
    def change(edit):
        marker='    let mut offset = 0usize;'
        edit(marker,locator('leased')+'    work.__rse_decode_view("leased",0);\n'+vector('scratch','leopard_scratch','leased',4)+marker)
        marker='        let size = end - offset;'
        edit(marker,marker+'\n        work.__rse_decode_view("chunk_ready",size);\n'+vector('scratch','leopard_scratch','chunk_ready',8))
        marker='        offset = end;'
        edit(marker,'        work.__rse_decode_view("chunk_coded",size);\n'+vector('scratch','leopard_scratch','chunk_coded',8)+marker)
        marker='\n    Ok(())\n}'
        edit(marker,'\n'+locator('returned')+'    work.__rse_decode_view("returned",0);\n'+vector('scratch','leopard_scratch','returned',4)+marker)
    return top(text,declaration,change)


def gf16(text):
    current='struct DecodeBuffers {' in text
    if current:
        def lease(edit):
            marker='    let result = operation(&mut buffers);'
            def views(stage):
                return '    buffers.work.__rse_decode_view("'+stage+'",0);\n'+vector('buffers.scratch','leopard_scratch',stage,4)+vector('buffers.out_scratch','leopard_out_scratch',stage,4)
            edit(marker,views('leased')+marker+'\n'+views('returned'))
        text=top(text,'fn with_decode_buffers<',lease)
    def chunk(edit):
        marker='    // Step 1:'
        # Both supported revisions start the compute kernel with this comment.
        # The exact full comment is preserved by prefix insertion.
        edit(marker,locator('chunk_ready')+'    work.__rse_decode_view("chunk_ready",size);\n'+vector('scratch','leopard_scratch','chunk_ready',4)+marker)
        marker='\n    Ok(())\n}'
        edit(marker,'\n'+locator('chunk_coded')+'    work.__rse_decode_view("chunk_coded",size);\n'+vector('scratch','leopard_scratch','chunk_coded',4)+marker)
    text=top(text,'fn compute_reconstruct_chunk(',chunk)
    # Output scaling scratch is initialized before each use, in both serial
    # and parallel paths. Its original resize and release statements remain.
    import re
    pattern=r'(?m)^( +)if out_scratch\.len\(\) < size \{\n +out_scratch\.resize\(size, 0\);\n +\}'
    matches=list(re.finditer(pattern,text))
    if len(matches)!=2:raise ValueError('GF16 output scratch initialization changed')
    for match in reversed(matches):
        original=match.group();changed=original+'\n'+vector('out_scratch','leopard_out_scratch','chunk_ready',len(match[1])).rstrip('\n')
        text=text[:match.start()]+changed+text[match.end():]
    return text


def patch(original):
    return {FILES[0]:work(original[FILES[0]],False),FILES[1]:gf8(original[FILES[1]]),
            FILES[2]:work(original[FILES[2]],True),FILES[3]:gf16(original[FILES[3]])}
