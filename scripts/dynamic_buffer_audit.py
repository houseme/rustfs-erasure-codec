"""Unified remaining byte-storage/allocator inventory; invoked by fixture inventory.

All modifications are reversible source-copy observers. No production allocator,
sampler, algorithm, original allocation statement or release gate is changed.
"""
from contextlib import contextmanager
import copy
import json
import os
import pathlib
import re
import subprocess

import release_evidence as e
import release_sampling as s
from diagnostic_overlay import overlay
import stream_core_source as edits
import dynamic_leopard_source as leopard

MODULE = '__rse_dynamic_buffers'
PREFIX = 'RSE_DYNAMIC_BUFFERS '
PROFILES = ('smoke', 'small_file', 'hotspot', 'stream')
SUPPLEMENTS = {
    'classic_uncached:10:4:16384': ('Classic',1,False),
    'tower_gf16:10:4:65': ('Classic',2,False),
    'leopard_gf8:10:4:32768': ('LeopardGF8',1,True),
    'leopard_gf8:10:4:65600': ('LeopardGF8',1,True),
    'leopard_gf16:10:4:32768': ('LeopardGF16',1,True),
    'leopard_gf16:10:4:65600': ('LeopardGF16',1,True),
}
NAMES = ('src/lib.rs', 'src/matrix.rs', 'src/core/reconstruct.rs', 'src/galois_8/policy.rs') + leopard.FILES
TEMPLATE = pathlib.Path(__file__).resolve().parent/'diagnostics/dynamic_buffers.rs'
API = 'rustfs_erasure_codec::' + MODULE + '::'
LIB = 'crate::' + MODULE + '::'


def native_template():
    text = TEMPLATE.read_text().replace('mod page_residency;', 'mod page_residency {\n'+(TEMPLATE.parent/'page_residency.rs').read_text()+'\n}', 1)
    engine = (s.TEMPLATES/'engine.rs').read_text()
    return text + ''.join('\n#[cfg(target_os="linux")]\n'+s.block(engine, name)+'\n' for name in ('fn read_bounded', 'fn process_faults'))


def patch_matrix(text):
    heap = 'self.data.spilled()' if 'data: SmallVec<' in text else 'true'
    helper = ('\n    pub(crate) fn __rse_buffer_view(&self, stage: &\'static str) {\n'
              f'        {LIB}span("matrix",stage,0,self.data.as_ptr(),self.data.len(),Some(self.data.capacity()),if {heap} {{"heap"}} else {{"inline"}},true);\n'
              '    }\n')
    marker = 'impl<F: Field> Matrix<F> {'
    if text.count(marker) != 1: raise ValueError('matrix implementation changed')
    text = text.replace(marker, marker+helper, 1)
    def change(original, edit):
        if original.count('        let mut work = self.augment(&Self::identity(row_count))?;') != 1: raise ValueError('matrix scratch initializer changed')
        marker = '        work.gaussian_elim()?;'
        edit(marker, '        work.__rse_buffer_view("invert_ready");\n'+marker+'\n        work.__rse_buffer_view("invert_coded");')
    text = edits.method(text, '    pub fn invert(', change)
    # Observe every original matrix constructor's initialized element storage.
    def change_new(original, edit):
        marker = '        Matrix {'
        h = 'data.spilled()' if 'data: SmallVec<' in text else 'true'
        edit(marker, f'        {LIB}span("matrix","constructed",0,data.as_ptr(),data.len(),Some(data.capacity()),if {h} {{"heap"}} else {{"inline"}},true);\n'+marker)
    return edits.method(text, '    pub fn new(rows:', change_new)


def region(text, declaration):
    def change(original, edit):
        prefix = original[:original.index('{')+1]
        edit(prefix, prefix+f'\n        let _rse_region = {LIB}Region::new("decode");')
    return edits.method(text, declaration, change)


def patch_decode(text):
    for declaration in ('    pub fn reconstruct<', '    pub fn reconstruct_data<', '    pub fn reconstruct_some<'):
        text = region(text, declaration)
    def matrix(original, edit):
        marker = '            if let Some(entry) = cache.get(invalid_indices) {'
        edit(marker, marker+'\n                entry.__rse_buffer_view("decode_cached");')
        marker = '        let data_decode_matrix = Arc::new('
        edit(marker, '        sub_matrix.__rse_buffer_view("decode_submatrix");\n'+marker)
        marker = '        Ok(data_decode_matrix)'
        edit(marker, '        data_decode_matrix.__rse_buffer_view("decode_ready");\n'+marker)
    text = edits.method(text, '    pub(crate) fn get_data_decode_matrix(', matrix)
    text=region(text, '    pub(crate) fn get_data_decode_matrix(')
    if '    fn reconstruct_leopard_impl<' in text:
        def owned(original,edit):
            if 'let mut recovered: SmallVec<[Vec<u8>; 32]>' in original:
                marker='        let mut outputs: SmallVec<[&mut [u8]; 32]> ='
                edit(marker,'        '+LIB+'buffers("leopard_decoded","prepared",&recovered);\n'+marker)
                marker='        drop(input_data);'
                edit(marker,marker+'\n        '+LIB+'buffers("leopard_decoded","decoded",&recovered);')
            else:
                marker='        let mut outputs: Vec<&mut [u8]> = output_bufs'
                edit(marker,'        '+LIB+'buffers("leopard_decoded","prepared",&output_bufs);\n'+marker)
                marker='        for i in 0..total {\n            if present[i] {'
                edit(marker,'        '+LIB+'buffers("leopard_decoded","decoded",&output_bufs);\n'+marker)
        text=edits.method(text,'    fn reconstruct_leopard_impl<',owned)
    return text


def patch_inputs(original):
    return {'src/lib.rs': original['src/lib.rs']+'\n#[doc(hidden)]\npub mod '+MODULE+';\n',
            'src/matrix.rs': patch_matrix(original['src/matrix.rs']),
            'src/core/reconstruct.rs': patch_decode(original['src/core/reconstruct.rs']),
            'src/galois_8/policy.rs': region(original['src/galois_8/policy.rs'], '    pub fn reconstruct_opt('),
            'src/'+MODULE+'.rs': native_template(), **leopard.patch(original)}


def bindings(original):
    return {name: {'original_sha256': e.digest(original[name].encode()) if name in original else None,
                   'diagnostic_sha256': e.digest(text.encode())} for name, text in patch_inputs(original).items()}


@contextmanager
def source_overlay(root):
    if any((root/n).is_symlink() or not (root/n).is_file() for n in NAMES): raise ValueError('require regular source-copy inputs')
    original = {n: (root/n).read_text() for n in NAMES}
    changes = patch_inputs(original)
    added = root/('src/'+MODULE+'.rs')
    if added.exists() or added.is_symlink(): raise ValueError('preserve an existing dynamic observer')
    try:
        for name, text in changes.items(): (root/name).write_text(text)
        yield bindings(original)
    finally:
        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text() != text for n, text in changes.items()):
            raise ValueError('unexpected diagnostic source change; preserve copy')
        for name, text in original.items(): (root/name).write_text(text)
        added.unlink()


def patch_loop(loop, transform):
    changes = []
    def change(before, after, count=1):
        nonlocal loop
        if loop.count(before) != count: raise ValueError('dynamic caller boundary changed: '+before)
        loop = loop.replace(before, after); changes.append((before, after, count))
    original = loop
    transform(change)
    restored = loop
    for before, after, count in reversed(changes):
        if restored.count(after) != count: raise ValueError('caller observer is not reversible')
        restored = restored.replace(after, before)
    if restored != original: raise ValueError('observer changed original caller work')
    return loop


def caller_kernel(text, profile):
    declaration='fn execute_smoke_kernel' if profile=='smoke' else 'fn execute_release_kernel'
    kernel=s.block(text,declaration)
    if profile=='smoke':
        def transform(change):
            change('    for _ in 0..iterations {', '    for _ in 0..iterations {\n        let _rse_epoch = '+API+'Epoch::begin();')
            statement = '                    make_full_shards(seed, case.data_shards, case.parity_shards, case.shard_size);'
            # Also retain non-catalog Leopard source arms unchanged and honest.
            change(statement, statement+'\n                '+API+'vectors("source","created",&shards);', 6)
            change('                let old_refs =', '                '+API+'vectors("old_data","cloned",&old_data);\n                '+API+'vectors("updated","changed",&updated);\n                '+API+'vectors("parity","before",&parity);\n                let old_refs =')
            marker='                rs.update(&old_refs, &changes, &mut parity_refs).unwrap();'
            change(marker,marker+'\n                '+API+'vectors("parity","updated",&parity);')
            for method in ('reconstruct', 'reconstruct_data'):
                marker = f'                rs.{method}(&mut shards).unwrap();'
                change(marker, '                '+API+'optional("dynamic","missing",&shards);\n'+marker+'\n                '+API+'optional("dynamic","recovered",&shards);')
            marker = '                rs.encode(&mut shards).unwrap();'
            change(marker, marker+'\n                '+API+'vectors("source","encoded",&shards);', 5)
            marker = '                let mut shards: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();'
            change(marker, marker+'\n                '+API+'optional("dynamic","moved",&shards);', 2)

        changed=patch_loop(kernel,transform)
    else:
        changed=kernel
        entries=s.SMALL_KERNELS if profile=='small_file' else s.STREAM_KERNELS
        for pattern,operation in entries:
            loop=s.original_loop(kernel,pattern)
            def transform(change):
                marker='            for _ in 0..iterations {'
                change(marker,marker+'\n                let _rse_epoch = '+API+'Epoch::begin();\n                '+API+'vectors("source","retained",original);'+('\n                '+API+'vectors("scratch","retained",scratch);' if profile=='small_file' else ''))
                if profile!='small_file' or not operation.startswith('reconstruct'):return
                slot=operation=='reconstruct_shard_slot'
                helper='slots' if slot else 'optional'
                marker='                let mut shards = shards_to_slots(original);' if slot else '                let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();'
                change(marker,marker+'\n                '+API+helper+'("dynamic","cloned",&shards);')
                method='reconstruct' if slot else 'reconstruct_some' if operation=='reconstruct_some_data_only' else operation
                args='&mut shards, required' if operation=='reconstruct_some_data_only' else '&mut shards'
                marker=f'                rs.{method}({args}).unwrap();'
                change(marker,'                '+API+helper+'("dynamic","missing",&shards);\n'+marker+'\n                '+API+helper+'("dynamic","recovered",&shards);')
            changed=changed.replace(loop,patch_loop(loop,transform),1)
    return text.replace(kernel,changed,1)


def hotspot_kernel(text):
    helper=s.block(text,'fn option_shards_with_missing')
    marker='    let mut working = shards_to_option_shards(shards);'
    if helper.count(marker)!=1:raise ValueError('hotspot original clone helper changed')
    text=text.replace(helper,helper.replace(marker,marker+'\n    '+API+'optional("clone","cloned",&working);',1),1)
    for name in ('data','some'):
        declaration = 'fn bench_reconstruct_'+name+'_hotspot'
        kernel = s.block(text,declaration)
        def transform(change):
            change('    for _ in 0..iterations {', '    for _ in 0..iterations {\n        let _rse_epoch = '+API+'Epoch::begin();')
            marker='        let mut shards = make_random_shards!(rng, shard_size, data_shards + parity_shards);'
            change(marker, marker+'\n        '+API+'vectors("source","created",&shards);')
            marker='        r.encode(&mut shards).unwrap();'
            change(marker, marker+'\n        '+API+'vectors("source","encoded",&shards);')
            for role in ('baseline','candidate'):
                marker=f'        let mut {role} = option_shards_with_missing(&shards, missing_indices);'
                change(marker, marker+'\n        '+API+f'optional("{role}","missing",&{role});')
                method = ('reconstruct' if name=='data' else 'reconstruct_data') if role=='baseline' else ('reconstruct_data' if name=='data' else 'reconstruct_some')
                args=f'&mut {role}'+(', required' if method=='reconstruct_some' else '')
                marker=f'        r.{method}({args}).unwrap();'
                change(marker, marker+'\n        '+API+f'optional("{role}","recovered",&{role});')
        text=text.replace(kernel,patch_loop(kernel,transform),1)
    return text


def render(root):
    files=s.render(root)
    for profile in ('smoke','small_file','stream'): files[profile+'.rs']=caller_kernel(files[profile+'.rs'],profile)
    files['hotspot.rs']=hotspot_kernel(files['hotspot.rs'])
    text=files['engine.rs']
    markers=(('    let constructor_budget = std::thread::available_parallelism().unwrap().get();',
              '    let _rse_root = '+API+'Root::start(page_size);\n    let constructor_budget = std::thread::available_parallelism().unwrap().get();'),
             ('            ITERATIONS.with(|n| n.set(count));', '            '+API+'observation(index);\n            ITERATIONS.with(|n| n.set(count));'))
    for old,new in markers:
        if text.count(old)!=1:raise ValueError('original sampler dynamic boundary changed')
        text=text.replace(old,new,1)
    for marker in ('            println!(\n                "\\nRSE_TIMING ', '        println!(\n            "\\nRSE_SAMPLE '):
        if text.count(marker)!=1:raise ValueError('original numeric export boundary changed')
        indent=marker.split('println!',1)[0]
        text=text.replace(marker,indent+'let rse_diagnostic_workload = RseDiagnosticWorkload(&workload);\n'+marker,1)
    marker='\\"workload\\":{workload}'
    if text.count(marker)!=2:raise ValueError('original workload formatter changed')
    text=text.replace(marker,'\\"workload\\":{rse_diagnostic_workload}')
    text+='\nstruct RseDiagnosticWorkload<\'a>(&\'a super::workload::OptionalWorkload);\nimpl std::fmt::Display for RseDiagnosticWorkload<\'_> {fn fmt(&self,f:&mut std::fmt::Formatter<\'_>)->std::fmt::Result {write!(f, "{{\\"observer\\":\\"dynamic-source-copy-v1\\",\\"contract\\":{}}}", self.0)}}\n'
    text+='\npub(super) fn rse_dynamic_placement(budget:usize)->impl Fn() {let placement=Placement::prepare(budget); move || {if let Some(p)=&placement {p.check(); assert!(p.is_current());}}}\n'
    helper_start=text.index('\nstruct RseDiagnosticWorkload')
    helpers=text[helper_start:]
    text=text[:helper_start]
    marker='\n#[cfg(test)]\nmod tests {'
    if text.count(marker)!=1:raise ValueError('sampler test module boundary changed')
    text=text.replace(marker,helpers+marker,1)
    files['engine.rs']=text
    files['root.rs']+='\n#[global_allocator]\nstatic RSE_DYNAMIC_ALLOCATOR: '+API+'Allocator = '+API+'Allocator;\n'
    semantics=(TEMPLATE.parent/'dynamic_buffer_semantics.rs').read_text().replace('__RSE_PAGE_SIZE__','observer::native_page_size()').replace('__RSE_CURRENT_SOURCE__','current_source()')
    files['root.rs']+='\n'+(TEMPLATE.parent/'dynamic_decoder_supplement.rs').read_text()+'\n'+semantics

    return files
# Appended to dynamic_buffer_audit.py; shared strict capture/replay contract.
MODES = ('calibrate', 'sample')
EVENT_FIELDS = {'kind','observation','call','thread','region','allocation','allocation_offset','previous',
                'role','stage','index','len','capacity','element_size','shape','storage','occupied','present',
                'bytes','align','offsets','elapsed_ns','faults','snapshot'}


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= 2**64-1


def snapshot(event, page, native):
    value=event['snapshot']; size=event['bytes']; offsets=event['offsets']
    if not isinstance(value,dict) or set(value)!={'status','pages','resident_pages','errno'}:
        raise ValueError('invalid dynamic native snapshot')
    if size==0:
        if value!={'status':'empty','pages':0,'resident_pages':0,'errno':None}:
            raise ValueError('nonempty snapshot for empty dynamic range')
        return
    if not native:
        if value!={'status':'unsupported','pages':None,'resident_pages':None,'errno':None}:
            raise ValueError('unrecorded native dynamic residency')
        return
    pages=(offsets[2]+size+page-1)//page
    if type(value['pages']) is not int or value['pages']!=pages:raise ValueError('dynamic page extent differs')
    status=value['status']; resident=value['resident_pages']; error=value['errno']
    if status=='mapped':
        if not integer(resident) or resident>pages or error is not None:raise ValueError('invalid dynamic resident count')
    elif status=='not_fully_mapped' and event['kind']=='free':
        if resident is not None or type(error) is not int or error!=12:raise ValueError('invalid former dynamic range')
    elif status=='over_budget':
        if pages<=4097 or resident is not None or error is not None:raise ValueError('invalid bounded dynamic query')
    else:raise ValueError('unavailable live dynamic residency')



def inventory_steps():
    steps=[]
    for role in ('candidate','baseline'):
        steps.extend({'role':role,'mode':mode,'profile':profile,'case':case}
                     for mode in MODES for profile in PROFILES for case in sorted(e.expected_cases(profile)))
        steps.extend({'role':role,'mode':'structure','profile':'decoder','case':case} for case in SUPPLEMENTS)
    return steps


def supplement_report(header,ledger,case,page,cpus,source):
    if case not in SUPPLEMENTS:raise ValueError('unknown fixed decoder supplement')
    family,width,cache=SUPPLEMENTS[case];_,k,m,size=case.split(':');k,m,size=map(int,(k,m,size))
    expected={'schema':'decoder-owner-supplement-v1','observer':'dynamic-source-copy-v1','case':case,
              'data_shards':k,'parity_shards':m,'shard_size_elements':size,'element_size':width,'family':family,
              'inversion_cache':cache,'fast_one_parity':False,'source_lifetime':'worker','construction':'codec-then-source-then-explicit-pool-preparation','output_policy':'iteration-cloned-options-drop-missing-reconstruct-all','missing_mask':1025,'required_mask':16383,'pool_threads':4,'native_placement_checked':cpus is not None,
              'normal_outputs_match_source':True,'release_approval':False}
    if not isinstance(header,dict) or set(header)!=set(expected)|{'constructor_budget'} or any(type(header.get(key)) is not type(value) or header[key]!=value for key,value in expected.items()) or not integer(header['constructor_budget'],1) or cpus is not None and header['constructor_budget']>len(cpus):raise ValueError('invalid source-bound decoder supplement')
    return validate_ledger(ledger,'decoder',case,page,native=cpus is not None,legacy=source['commit']==e.BASELINE_COMMIT)


def validate_internal_view(v,profile,case,legacy):
    role=v['role']
    if role=='leopard_work':
        if profile!='decoder' or not SUPPLEMENTS[case][0].startswith('Leopard'):raise ValueError('work view outside declared decoder family')
        shape=v['shape']
        if not isinstance(shape,list) or len(shape)!=3 or any(not integer(x) for x in shape) or shape[0]!=16 or not 0<shape[2] or shape[1]>shape[2] or v['capacity']!=shape[0]*shape[2] or v['len']!=v['capacity'] or v['element_size']!=(2 if case.startswith('leopard_gf16:') else 1) or v['stage'] not in ('leased','returned','chunk_ready','chunk_coded') or v['stage'].startswith('chunk_') and shape[1]==0 or v['index']!=0 or v['present'] is not None or v['occupied'] is not True or v['storage']!='heap':raise ValueError('invalid actual Leopard work/lane shape')
    else:
        if v['shape'] is not None:raise ValueError('non-work view contains lane metadata')
        if role.startswith('leopard_'):
            if profile!='decoder' or not SUPPLEMENTS[case][0].startswith('Leopard'):raise ValueError('Leopard storage outside declared family')
            if v['present'] is not None or v['occupied'] is not True or v['capacity'] is None:raise ValueError('invalid internal decoder owner')
            if role=='leopard_locator':
                width=2 if case.startswith('leopard_gf16:') else 1;length=65536 if width==2 else 256
                if v['len']!=length or v['capacity']!=length or v['element_size']!=width or v['storage']!='inline' or v['stage'] not in ('leased','returned','chunk_ready','chunk_coded') or v['index']!=0:raise ValueError('invalid error-locator storage')
            elif role in ('leopard_scratch','leopard_out_scratch'):
                if v['element_size']!=(2 if case.startswith('leopard_gf16:') else 1) or v['storage']!='heap' or v['stage'] not in ('leased','returned','chunk_ready','chunk_coded') or v['index']!=0:raise ValueError('invalid FFT conversion/scaling scratch')
            elif role=='leopard_decoded':
                _,k,m,size=case.split(':');k,m,size=map(int,(k,m,size))
                length=size if legacy or v['index'] in (0,k) else 0
                if not v['index']<k+m or v['len']!=length or v['capacity']<length or v['element_size']!=1 or v['storage']!='heap' or v['stage'] not in ('prepared','decoded'):raise ValueError('invalid decoded-before-publication owner')

def dimensions(profile,case):
    if profile=='decoder':
        _,k,m,size=case.split(':');return int(k),int(m),int(size),(0,int(k)),None
    if profile=='hotspot':
        k,m,_,missing,required=s.hotspot_case(case)
        return k,m,1048576,missing,required
    parts=case.split(':');k,m,size=map(int,parts[1:4])
    return k,m,size,(),None


def payload_contract(events,profile,case):
    k,m,size,missing,required=dimensions(profile,case);n=k+m
    operation=case.split(':')[0]
    recovery=operation.startswith('reconstruct') or profile in ('hotspot','decoder')
    count=k if profile=='stream' and operation=='encode_stream' else n
    schedule=[]
    def group(role,stage,amount):schedule.extend((role,stage,index) for index in range(amount))
    if profile in ('small_file','stream','decoder'):
        group('source','retained',count)
        if profile in ('small_file','decoder'):
            if operation=='verify_with_buffer':group('scratch','retained',m)
            if recovery:
                for stage in ('cloned','missing','recovered'):group('dynamic',stage,n)
    else:
        group('source','created',n);group('source','encoded',n)
        if profile=='smoke' and operation=='update':
            group('old_data','cloned',k);group('updated','changed',k)
            group('parity','before',m);group('parity','updated',m)
        elif profile=='smoke' and recovery:
            for stage in ('moved','missing','recovered'):group('dynamic',stage,n)
        elif profile=='hotspot':
            for role in ('baseline','candidate'):
                group('clone','cloned',n);group(role,'missing',n);group(role,'recovered',n)
    payload_roles={'source','scratch','dynamic','baseline','candidate','clone','old_data','updated','parity'}
    views=[v for v in events if v['kind']=='view' and v['role'] in payload_roles]
    if [(v['role'],v['stage'],v['index']) for v in views]!=schedule:
        raise ValueError('incomplete or reordered dynamic payload stages')
    mask=(1|(1<<k)) if profile=='decoder' else s.workload_contract(profile,case)['missing_mask']
    wanted=(1<<n)-1 if profile=='decoder' else s.workload_contract(profile,case)['required_mask']
    width=SUPPLEMENTS[case][1] if profile=='decoder' else 1
    clone_groups=[];current=[];previous=None
    for v in views:
        key=(v['role'],v['stage'])
        if previous is not None and (key!=previous or v['index']==0):
            clone_groups.append(current);current=[]
        current.append(v);previous=key
        expected=True
        if v['role'] in ('dynamic','baseline','candidate'):
            absent=bool(mask&(1<<v['index']))
            if v['stage']=='missing':expected=not absent
            elif v['stage']=='recovered':
                requested=(v['index']<k if required is not None else True) if v['role']=='baseline' else bool(wanted&(1<<v['index']))
                expected=not absent or requested
        owner=True if operation=='reconstruct_shard_slot' and v['role']=='dynamic' else expected
        if type(v['present']) is not bool or v['present']!=expected or v['occupied']!=owner:
            raise ValueError('dynamic presence differs from actual missing/required policy')
        length=size if owner else 0
        if v['len']!=length or v['capacity'] is None or v['capacity']<length or v['element_size']!=width:
            raise ValueError('dynamic payload initialized length/capacity differs')
        if not owner and v['capacity']!=0:raise ValueError('absent dynamic owner retains undocumented capacity')
        if length and (v['storage']!='heap' or v['allocation'] is None or v['allocation_offset']!=0):
            raise ValueError('dynamic payload lacks an observed allocation generation')
    if current:clone_groups.append(current)
    # Moved owners retain request identity. Slot flags never release backing
    # storage; ordinary Option erasures do. New generations cannot masquerade
    # as retained storage even when the numerical pointer is reused.
    by={(g[0]['role'],g[0]['stage']):g for g in clone_groups}
    if profile in ('smoke','hotspot'):
        shape=lambda group:[(v['allocation'],v['allocation_offset'],v['len'],v['capacity'],v['offsets']) for v in group]
        if shape(by['source','created'])!=shape(by['source','encoded']):raise ValueError('encoding replaced generated source storage')
    if profile=='smoke' and operation=='update':
        if [(v['allocation'],v['capacity']) for v in by['parity','before']]!=[(v['allocation'],v['capacity']) for v in by['parity','updated']]:raise ValueError('update replaced parity backing storage')
    if profile=='smoke' and recovery:
        if [v['allocation'] for v in by['source','encoded']]!=[v['allocation'] for v in by['dynamic','moved']]:
            raise ValueError('smoke source ownership move became a clone')
    if operation=='reconstruct_shard_slot':
        first=[(v['allocation'],v['allocation_offset']) for v in by['dynamic','cloned']]
        if any([(v['allocation'],v['allocation_offset']) for v in by['dynamic',stage]]!=first for stage in ('missing','recovered')):
            raise ValueError('ShardSlot storage was replaced while changing presence')
    return views


def validate_ledger(data,profile,case,page,native=False,legacy=False):
    if profile not in PROFILES+('decoder',) or case not in (SUPPLEMENTS if profile=='decoder' else e.expected_cases(profile)) or not s.valid_page_size(page):raise ValueError('unknown dynamic inventory contract')
    if not isinstance(data,dict) or set(data)!={'schema','page_size','errors','calls','release_approval','events'} or data['schema']!='dynamic-buffer-lifecycle-v1' or type(data['page_size']) is not int or data['page_size']!=page or data['release_approval'] is not False or type(data['errors']) is not int or data['errors']!=0 or data['calls']!=[2]*12 or any(type(v) is not int for v in data['calls']):
        raise ValueError('invalid or incomplete dynamic lifecycle ledger')
    events=data['events']
    if not isinstance(events,list) or not 1<=len(events)<=65536 or events[-1].get('kind')!='finish':raise ValueError('bounded complete dynamic ledger required')
    allocations={};live={};regions={};last_faults={};last_time=-1;next_id=0;epoch=0;opened=None;epochs=[];current=[];retained={}
    for ordinal,v in enumerate(events):
        if not isinstance(v,dict) or set(v)!=EVENT_FIELDS:raise ValueError('unknown dynamic event fields')
        kind=v['kind']
        if any(not isinstance(v[k],str) for k in ('kind','region','role','stage','storage')):raise ValueError('invalid dynamic labels')
        if v['region'] not in ('setup','caller','between','decode'):raise ValueError('unknown dynamic region')
        if kind not in ('alloc','alloc_zeroed','realloc','free','view','enter','leave','begin','end','finish'):raise ValueError('unknown dynamic event kind')
        if any(not integer(v[key]) for key in ('call','thread','index','len','element_size','bytes','align','elapsed_ns')) or v['thread']>=64 or v['element_size']==0 or v['elapsed_ns']<last_time:
            raise ValueError('invalid dynamic scalar/order')
        last_time=v['elapsed_ns']
        observation=v['observation']
        if observation is not None and (not integer(observation) or observation>=12):raise ValueError('invalid dynamic observation')
        for key in ('allocation','previous','allocation_offset','capacity'):
            if v[key] is not None and not integer(v[key],0 if key in ('allocation_offset','capacity') else 1):raise ValueError('invalid dynamic allocation reference')
        if type(v['occupied']) is not bool or v['present'] is not None and type(v['present']) is not bool:raise ValueError('invalid dynamic owner flags')
        offsets=v['offsets']
        if v['bytes']:
            if not isinstance(offsets,list) or len(offsets)!=3 or any(not integer(x) or x>=mod for x,mod in zip(offsets,(64,4096,page))) or offsets[1]%64!=offsets[0] or offsets[2]%min(page,4096)!=offsets[1]%min(page,4096):raise ValueError('invalid dynamic offsets')
        elif offsets is not None:raise ValueError('empty dynamic range has offsets')
        snapshot(v,page,native)
        if kind!='view' and (v['role'] or v['stage'] or v['index'] or v['len'] or v['capacity'] is not None or v['element_size']!=1 or v['storage'] or v['occupied'] is not False or v['present'] is not None or v['shape'] is not None or v['faults'] is not None):raise ValueError('non-view event contains forged view metadata')
        if kind in ('enter','leave','begin','end','finish') and (v['bytes'] or v['align'] or any(v[k] is not None for k in ('allocation','allocation_offset','previous'))):raise ValueError('scope marker contains allocation data')
        if kind=='view' and (v['previous'] is not None or v['align'] or v['role'] in ('source','scratch','dynamic','baseline','candidate','clone','old_data','updated','parity') and v['call']==0):raise ValueError('view escapes its declared scope')
        stack=regions.setdefault(v['thread'],[])
        if kind=='enter':stack.append(v['region'])
        elif kind=='leave':
            if not stack or stack.pop()!=v['region']:raise ValueError('unbalanced decoder region')
        if kind=='finish' and ordinal!=len(events)-1:raise ValueError('duplicate dynamic finish marker')
        if kind=='begin':
            epoch+=1
            if opened is not None or v['thread']!=0 or v['call']!=epoch or observation!=(epoch-1)//2:raise ValueError('overlapping or reordered dynamic iteration')
            opened=(v['call'],observation);current=[]
        if v['call']:
            if opened!=(v['call'],observation):raise ValueError('event escapes its dynamic iteration')
            current.append(v)
        if kind in ('alloc','alloc_zeroed','realloc'):
            if not integer(v['align'],1) or v['align']&(v['align']-1) or v['bytes']==0 or v['allocation']!=next_id+1 or v['allocation_offset']!=0:raise ValueError('invalid allocation request generation')
            if offsets[2]%min(v['align'],page):raise ValueError('allocator alignment differs from request')
            if kind=='realloc' and v['previous'] is not None:
                if v['previous'] not in live or live[v['previous']]['align']!=v['align']:raise ValueError('realloc references a dead/foreign generation')
                del live[v['previous']]
            elif kind!='realloc' and v['previous'] is not None:raise ValueError('unexpected previous allocation')
            next_id+=1;live[next_id]=v;allocations[next_id]=v
        elif kind=='free':
            ident=v['allocation']
            if ident is not None:
                if ident not in live or any(v[k]!=live[ident][k] for k in ('bytes','align','offsets')) or v['allocation_offset']!=0:raise ValueError('free generation/layout mismatch')
                del live[ident]
            elif v['allocation_offset'] is not None:raise ValueError('unattributed free has an allocation offset')
        elif kind=='view':
            if v['role'] not in {'matrix','source','scratch','dynamic','baseline','candidate','clone','old_data','updated','parity','leopard_work','leopard_locator','leopard_scratch','leopard_out_scratch','leopard_decoded'} or v['storage'] not in ('heap','inline','empty','borrowed') or not isinstance(v['stage'],str):raise ValueError('unknown dynamic view role/storage')
            if v['capacity'] is not None and v['len']>v['capacity'] or v['bytes']!=v['element_size']*(v['capacity'] if v['capacity'] is not None else v['len']):raise ValueError('dynamic element/byte extent mismatch')
            if native:
                if not isinstance(v['faults'],list) or len(v['faults'])!=2 or any(not integer(a) for a in v['faults']) or any(a>b for a,b in zip(last_faults.get(v['thread'],v['faults']),v['faults'])):raise ValueError('invalid native dynamic fault counters')
                last_faults[v['thread']]=v['faults']
            elif v['faults'] is not None:raise ValueError('unrecorded native dynamic counters')
            validate_internal_view(v,profile,case,legacy)
            if v['role']=='matrix':
                k=dimensions(profile,case)[0]
                if v['stage'] not in ('constructed','invert_ready','invert_coded','decode_submatrix','decode_ready','decode_cached') or v['index']!=0 or v['capacity'] is None or v['occupied'] is not True or v['present'] is not None or v['storage'] not in ('heap','inline') or v['element_size']!=(SUPPLEMENTS[case][1] if profile=='decoder' else 1):raise ValueError('unknown decoder matrix view')
                if v['stage'] in ('decode_submatrix','decode_ready','decode_cached') and v['len']!=k*k or v['stage'] in ('invert_ready','invert_coded') and v['len']!=2*k*k:raise ValueError('wrong decoder matrix dimensions')
            ident=v['allocation'];offset=v['allocation_offset']
            if ident is not None:
                if ident not in live or offset is None or offset+v['bytes']>live[ident]['bytes'] or v['offsets']!=[(a+offset)%mod for a,mod in zip(live[ident]['offsets'],(64,4096,page))]:raise ValueError('view is not inside its live allocation generation')
            elif offset is not None or v['storage']=='heap' and v['bytes']:raise ValueError('heap view has no live allocation')
        if kind=='end':
            if v['thread']!=0 or opened!=(v['call'],observation) or any(regions.values()):raise ValueError('dynamic iteration ended inside a decoder region')
            leopard_case=profile=='decoder' and SUPPLEMENTS[case][0].startswith('Leopard')
            if leopard_case:
                if not any(q['kind']=='view' and q['role']=='leopard_work' and q['stage']=='chunk_coded' and q['region']=='decode' for q in current):raise ValueError('iteration lacks actual Leopard decode work')
            elif (case.startswith('reconstruct') or profile in ('hotspot','decoder')) and not any(q['kind']=='view' and q['role']=='matrix' and q['stage'] in ('decode_ready','decode_cached') and q['region']=='decode' for q in current):raise ValueError('iteration lacks actual decoder matrix storage')
            views=payload_contract(current,profile,case)
            for view in views:
                ident=view['allocation']
                if ident is None:continue
                if profile in ('small_file','stream','decoder') and view['role'] in ('source','scratch'):
                    key=(view['role'],view['index']);signature=(ident,view['len'],view['capacity'],view['offsets'])
                    if retained.setdefault(key,signature)!=signature or ident not in live:raise ValueError('retained caller storage changed between iterations')
                elif ident in live:raise ValueError('iteration-owned payload outlives its original scope')
            epochs.append(current);opened=None
        if kind=='finish' and (ordinal!=len(events)-1 or opened is not None or v['call']!=0 or any(regions.values())):raise ValueError('incomplete dynamic owner teardown')
    if epoch!=24 or len(epochs)!=24:raise ValueError('incomplete dynamic iteration catalog')
    for ident,_,_,_ in retained.values():
        if ident in live:raise ValueError('prepared caller owner was not released before final ledger')

    return {'iterations':24,'events':len(events),'allocation_generations':len(allocations),'retained_requests':len(live),
            'payload_views':sum(v['kind']=='view' and v['role']!='matrix' for v in events),
            'decoder_matrix_views':sum(v['kind']=='view' and v['role']=='matrix' and v['region']=='decode' for v in events),
            'bounded_residency_queries':sum(v['snapshot']['status']=='over_budget' for v in events)}



def underlying_records(records):
    """Unwrap only the known diagnostic envelope in a private validator copy.

    Formal workload validation rejects the actual raw observations. Never strip
    this marker in a stored journal or promote these numbers to release data.
    """
    result=copy.deepcopy(records)
    for record in result:
        value=record.get('workload')
        if not isinstance(value,dict) or set(value)!={'observer','contract'} or value['observer']!='dynamic-source-copy-v1' or not isinstance(value['contract'],dict):
            raise ValueError('missing explicit dynamic diagnostic workload marker')
        record['workload']=value['contract']
    return result


def validate_envelope(report,output,repository):
    import release_memory_policy
    fields={'schema','release_approval','candidate_attribution','formal_performance_geometry_equivalence','tool','sources','cases','source_overlays','harness_sha256','overlay_sha256','context','page_size','cpu_affinity','native_sha256','sampler_implementation_sha256','controller_sha256'}
    if set(report)!=fields:raise ValueError('unexpected dynamic inventory envelope')
    context=report['context']
    if not isinstance(context,dict) or set(context)!={'compiler','machine','cpu_identity','cpu_affinity','page_size','kernel_memory_policy','libc','allocator_environment','settings'}:raise ValueError('missing dynamic runtime provenance')
    if any(not isinstance(context[k],str) or not context[k] for k in ('compiler','machine','cpu_identity')) or context['cpu_affinity']!=report['cpu_affinity'] or context['page_size']!=report['page_size'] or context['settings']!=e.SAMPLING_ENV:raise ValueError('dynamic runtime/settings differ')
    if not isinstance(context['libc'],list) or len(context['libc'])!=2 or any(not isinstance(v,str) for v in context['libc']) or not isinstance(context['allocator_environment'],dict) or any(not e.allocator_override_key(k) or not isinstance(v,str) for k,v in context['allocator_environment'].items()):raise ValueError('invalid dynamic allocator provenance')
    if report['cpu_affinity'] is not None:release_memory_policy.validate(context['kernel_memory_policy'],report['page_size'])
    elif context['kernel_memory_policy'] is not None:raise ValueError('unrecorded native memory policy')
    tool=report['tool']
    for source in (tool,*report['sources'].values()):
        if not isinstance(source,dict) or set(source)!={'commit','tree','lock_sha256'} or not isinstance(source['commit'],str) or not e.REVISION.fullmatch(source['commit']) or not isinstance(source['tree'],str) or not e.REVISION.fullmatch(source['tree']) or not isinstance(source['lock_sha256'],str) or not e.DIGEST.fullmatch(source['lock_sha256']):raise ValueError('invalid source/tool identity')
        if source!=e.source_at(repository,source['commit']):raise ValueError('original dynamic source/tool differs')
    if tool!=report['sources']['candidate']:raise ValueError('dynamic tool/candidate identity differs')
    for key in ('cases','source_overlays','harness_sha256','overlay_sha256'):
        if set(report[key])!={'candidate','baseline'}:raise ValueError('incomplete per-source dynamic envelope')
    plan=e.json_data(output/'plan.json')
    steps=inventory_steps()
    if set(plan)!=fields|{'steps'} or plan['steps']!=steps or plan['cases']!={'candidate':{},'baseline':{}} or any(plan[k]!=report[k] for k in fields-{'cases'}):raise ValueError('frozen complete dynamic plan differs')
    path=output/'execution.jsonl'
    if path.is_symlink() or not path.is_file() or not 0<path.stat().st_size<=e.MAX_BYTES:raise ValueError('bounded original execution journal required')
    entries=[json.loads(line) for line in path.read_text().splitlines()]
    if len(entries)!=2*len(steps):raise ValueError('incomplete original execution journal')
    for i,step in enumerate(steps):
        begin,end=entries[2*i:2*i+2]
        entry=report['cases'][step['role']][step['mode']+'/'+step['profile']+'/'+step['case']]
        if begin!={'stage':'begin','step':step,'context':context} or end!={'stage':'end','step':step,'context':context,'entry':entry}:raise ValueError('dynamic execution order/context/member differs')

def worker_report(rows,ledger,profile,case,source,page,cpus,mode):
    rows=underlying_records(rows)
    if mode not in MODES or len(rows)!=12:raise ValueError('require complete diagnostic mode observations')
    kinds=(['calibration_warmup']*3+['calibration']*9) if mode=='calibrate' else (['warmup']*3+['sample']*9)
    if [r['kind'] for r in rows]!=kinds:raise ValueError('wrong diagnostic mode schedule')
    s.validate_workload_sequence(rows,profile)
    for index,row in enumerate(rows):
        if row['revision']!=source['commit'] or row['case']!=case or row['index']!=index or type(row['index']) is not int or type(row['iterations']) is not int or row['iterations']!=2 or type(row['calibration_batch']) is not int or row['calibration_batch']!=(1 if mode=='calibrate' else 0) or row['isolated_case'] is not True:raise ValueError('wrong original dynamic worker')
        if set(e.indexed_rows(profile,row['rows'],source['commit'],{case:2},complete=False))!={case}:raise ValueError('dynamic original raw case differs')
        s.validate_fixture(row,profile,page);s.validate_timing(row,profile,2)
        if cpus is not None:s.validate_telemetry(row,cpus);s.validate_placement(row,cpus)
        elif any(row[k] is not None for k in ('before','after','placement')):raise ValueError('native worker lacks recorded affinity')
    if mode=='calibrate':s.calibration_batch(rows,2,1)
    e.observation_contracts(profile,rows)
    result=validate_ledger(ledger,profile,case,page,native=cpus is not None,legacy=source['commit']==e.BASELINE_COMMIT)
    for event in ledger['events']:
        if event['kind']=='view' and event['call'] and event['role'] in ('source','scratch') and event['stage']=='retained':
            row=rows[event['observation']];position=row['fixture'][event['role']][event['index']]
            if event['len']!=position['len'] or event['capacity']!=position['capacity'] or event['offsets']!=[position['offset_64'],position['offset_4096'],position['offset_page']]:raise ValueError('dynamic view differs from original retained caller fixture')
    if cpus is not None:
        for event in ledger['events']:
            if event['kind']=='view' and event['call']:
                row=rows[event['observation']]
                for i,field in enumerate(('process_minor_faults','process_major_faults')):
                    if not row['before'][field]<=event['faults'][i]<=row['after'][field]:raise ValueError('dynamic counters escape original observation')
    return result


def original_inputs(repository,source):
    return {name:subprocess.check_output(['git','show',source['commit']+':'+name],cwd=repository).decode() for name in NAMES}


def verify(output,repository):
    if any((output/name).exists() for name in ('failure.json','diagnostic-status.json')):raise ValueError('failed dynamic inventory is retained, not accepted')
    report=e.json_data(output/'result.json')
    if report.get('schema')!='dynamic-buffer-check-v1' or report.get('release_approval') is not False or report.get('candidate_attribution') is not None or report.get('formal_performance_geometry_equivalence') is not False:raise ValueError('invalid diagnostic-only dynamic inventory')
    if report.get('native_sha256')!=e.digest(native_template().encode()) or report.get('sampler_implementation_sha256')!=s.implementation_digest() or report.get('controller_sha256')!=e.digest(pathlib.Path(__file__).read_bytes()):raise ValueError('dynamic diagnostic implementation differs')
    validate_envelope(report,output,repository)
    page=report.get('page_size');cpus=report.get('cpu_affinity')
    if not s.valid_page_size(page) or cpus is not None and (not isinstance(cpus,list) or not cpus or any(not integer(c) for c in cpus) or cpus!=sorted(set(cpus))):raise ValueError('invalid dynamic page/CPU contract')
    sources=report.get('sources',{})
    if set(sources)!={'baseline','candidate'} or sources['baseline']['commit']!=e.BASELINE_COMMIT or sources['candidate']['commit']==e.BASELINE_COMMIT:raise ValueError('require both actual codec sources')
    expected={v['mode']+'/'+v['profile']+'/'+v['case'] for v in inventory_steps() if v['role']=='candidate'}
    contracts={};workloads={};traces={};members={};totals={'workers':0,'observations':0,'iterations':0,'events':0};differences={}
    for role,source in sources.items():
        if source!=e.source_at(repository,source['commit']) or report['source_overlays'][role]!=bindings(original_inputs(repository,source)):raise ValueError('original dynamic source/lock/overlay differs')
        if report['harness_sha256'][role]!=e.harness_digest(repository) or report['overlay_sha256'][role]!=overlay_digest(repository):raise ValueError('dynamic original worker/harness differs')
        cases=report['cases'][role]
        if set(cases)!=expected:raise ValueError('incomplete dynamic mode/profile/catalog')
        for key,entry in cases.items():
            mode,profile,case=key.split('/',2);name=role+'-'+mode+'-'+profile+'-'+case.replace(':','_')
            member=output/(name+'.json');journal=output/(name+'-journal.jsonl')
            if entry.get('path')!=member.name or entry.get('numeric_journal_path')!=journal.name:raise ValueError('invalid dynamic member path')
            for p,h in ((member,entry.get('sha256')),(journal,entry.get('numeric_journal_sha256'))):
                if p.is_symlink() or not p.is_file() or not 0<p.stat().st_size<=e.MAX_BYTES or e.digest(p.read_bytes())!=h:raise ValueError('dynamic member changed')
            private=output/(name+'.log')
            if not e.DIGEST.fullmatch(entry.get('raw_log_sha256','')) or private.exists() and (private.is_symlink() or e.digest(private.read_bytes())!=entry['raw_log_sha256']):raise ValueError('original private dynamic log changed')
            data=e.json_data(member);raw=journal.read_text();ledgers=s.parse_output(raw,PREFIX)
            if profile=='decoder':
                headers=s.parse_output(raw,'RSE_DYNAMIC_SUPPLEMENT ')
                if set(data)!={'supplement','dynamic'} or headers!=[data['supplement']] or ledgers!=[data['dynamic']]:raise ValueError('decoder supplement journal differs')
                summary=supplement_report(data['supplement'],data['dynamic'],case,page,cpus,source)
                rows=None
            else:
                rows=s.parse_output(raw);timing=s.parse_output(raw,'RSE_TIMING ')
            if profile!='decoder':
                if set(data)!={'samples','dynamic'} or rows!=data['samples'] or ledgers!=[data['dynamic']] or len(timing)!=12 or any(any(t[n]!=r[n] for n in ('case','index','iterations','wall_ns','timed_ns','fixture','workload','placement','before','after')) for t,r in zip(timing,rows)):raise ValueError('dynamic original journals differ')
                summary=worker_report(rows,data['dynamic'],profile,case,source,page,cpus,mode)
                contract_key=profile+'/'+case
                original=underlying_records(rows)
                contracts[contract_key]=e.observation_contracts(profile,original,contracts.get(contract_key))
                workload=s.workload_catalog(original);trace=[r['workload'] for r in original]
                if contract_key in workloads and (workloads[contract_key]!=workload or traces[contract_key]!=trace):raise ValueError('dynamic calibration/measurement input differs')
                workloads[contract_key]=workload;traces[contract_key]=trace
            if any(entry.get(k)!=v for k,v in summary.items()):raise ValueError('dynamic summary differs')
            differences[role+'/'+key]=sorted({(v['role'],v['stage'],v['index'],v['len'],v['capacity'],v['storage'],v['element_size'],tuple(v['shape']) if v['shape'] is not None else (),v['occupied'],v['present'],tuple(v['offsets']) if v['offsets'] is not None else ()) for v in data['dynamic']['events'] if v['kind']=='view'})
            members[member.name]=entry['sha256'];members[journal.name]=entry['numeric_journal_sha256']
            totals['workers']+=1;totals['observations']+=12;totals['iterations']+=summary['iterations'];totals['events']+=summary['events']
    mismatches=[]
    for key in sorted(expected):
        if differences['baseline/'+key]!=differences['candidate/'+key]:mismatches.append(key)
    return {'schema':'dynamic-buffer-replay-v1','release_approval':False,'candidate_attribution':None,
            'formal_performance_geometry_equivalence':False,'case_count':132,'original_case_count':126,'decoder_supplement_case_count':6,'source_count':2,'modes':list(MODES),**totals,
            'members':members,'diagnostic_geometry_differences':mismatches,
            'coverage':{'classic_decoder_matrix_storage':True,'decoder_heap_request_generations':True,'non_stream_dynamic_payloads':True,
                        'smoke_hotspot_source_positions':True,'calibration_and_measurement_lifetimes':True,
                        'leopard_decode_work_leases':True,'leopard_locator_conversion_output_scratch':True,'physical_allocation_identity':False,'first_fault_instruction':False,'formal_performance_acceptance':False}}


def overlay_digest(root):
    return e.digest(b''.join(n.encode()+b'\0'+t.replace(str(root),'<SOURCE>').encode() for n,t in sorted(render(root).items())))



def dynamic_context(root,env):
    import platform
    import release_memory_policy
    cpus=sorted(os.sched_getaffinity(0)) if hasattr(os,'sched_getaffinity') else None
    linux=platform.system()=='Linux'
    return {'compiler':e.command(['rustc','--version'],root,env),'machine':platform.machine(),
            'cpu_identity':e.cpu_identity(pathlib.Path('/proc/cpuinfo').read_text(),platform.machine(),cpus) if linux else platform.processor() or platform.machine(),
            'cpu_affinity':cpus,'page_size':s.base_page_size(),
            'kernel_memory_policy':release_memory_policy.capture() if linux else None,
            'libc':list(platform.libc_ver()),'allocator_environment':{k:v for k,v in sorted(env.items()) if e.allocator_override_key(k)},
            'settings':{k:env[k] for k in e.SAMPLING_ENV}}


def check(repository,output,baseline_ref=None):
    if output.resolve().is_relative_to(repository.resolve()):raise ValueError('keep dynamic raw evidence outside the source repository')
    output.mkdir(parents=True,mode=0o700)
    try:
        tool=e.source_info(repository);env=e.capture_environment();context=dynamic_context(repository,env)
        revisions={'candidate':tool['commit'],'baseline':e.source_at(repository,baseline_ref or e.BASELINE_COMMIT)['commit']}
        if revisions['baseline']!=e.BASELINE_COMMIT or revisions['candidate']==e.BASELINE_COMMIT:raise ValueError('require published baseline and distinct candidate')
        report={'schema':'dynamic-buffer-check-v1','release_approval':False,'candidate_attribution':None,'formal_performance_geometry_equivalence':False,
                'tool':tool,'sources':{},'cases':{},'source_overlays':{},'harness_sha256':{},'overlay_sha256':{},'context':context,
                'page_size':context['page_size'],'cpu_affinity':context['cpu_affinity'],
                'native_sha256':e.digest(native_template().encode()),'sampler_implementation_sha256':s.implementation_digest(),
                'controller_sha256':e.digest(pathlib.Path(__file__).read_bytes())}
        roots={}
        # Resolve every original source and reversible transform before running
        # either version; a failure cannot erase source/protocol provenance.
        for role,revision in revisions.items():
            root=output/role;s.checkout_baseline(repository,root,revision);roots[role]=root
            source=e.source_info(root);report['sources'][role]=source;report['cases'][role]={}
            report['source_overlays'][role]=bindings(original_inputs(repository,source))
            report['harness_sha256'][role]=e.harness_digest(root);report['overlay_sha256'][role]=overlay_digest(root)
        if len(set(report['harness_sha256'].values()))!=1 or len(set(report['overlay_sha256'].values()))!=1:raise ValueError('original dynamic harness differs across versions')
        steps=inventory_steps()
        (output/'plan.json').write_text(json.dumps(dict(report,steps=steps),indent=2)+'\n')
        with (output/'execution.jsonl').open('x') as execution:
            for role,root in roots.items():
                source=report['sources'][role]
                with source_overlay(root) as patches,overlay(root,render(root)):
                    if patches!=report['source_overlays'][role]:raise ValueError('dynamic source transform changed')
                    patched=patch_inputs(original_inputs(repository,source))
                    def unchanged():
                        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text()!=text for n,text in patched.items()):raise ValueError('dynamic source changed during observation')
                        if set(e.command(['git','diff','--name-only'],root).splitlines())!=set(NAMES):raise ValueError('foreign tracked diagnostic source change')
                        if e.source_info(repository)!=tool or dynamic_context(repository,env)!=context:raise ValueError('dynamic tool or runtime context changed')
                    unchanged()
                    with (output/(role+'-build.log')).open('x') as log:
                        subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=root,env=s.build_environment(root,env),stdout=log,stderr=subprocess.STDOUT,check=True)
                    with (output/(role+'-semantics.log')).open('x') as log:
                        controls=dict(s.build_environment(root,env),RSE_DYNAMIC_CURRENT_SOURCE='0' if role=='baseline' else '1')
                        subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','dynamic_semantics::active_observer_preserves_normal_selective_invalid_and_late_failure_semantics','--','--exact','--nocapture','--test-threads=1'],cwd=root,env=controls,stdout=log,stderr=subprocess.STDOUT,check=True)
                    for step in (v for v in steps if v['role']==role):
                        mode,profile,case=(step[k] for k in ('mode','profile','case'));unchanged()
                        execution.write(json.dumps({'stage':'begin','step':step,'context':context})+'\n');execution.flush()
                        settings=dict(s.worker_environment(env),RSE_SAMPLER_PROFILE=profile,RSE_SAMPLER_MODE=mode,RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='2',RSE_SAMPLER_BATCH='1' if mode=='calibrate' else '0',RSE_SAMPLER_PAGE_SIZE=str(report['page_size']))
                        if profile=='decoder':settings['RSE_DYNAMIC_DECODER_CASE']=case
                        name=role+'-'+mode+'-'+profile+'-'+case.replace(':','_');log=output/(name+'.log');numeric=output/(name+'-journal.jsonl')
                        try:
                            with log.open('x') as stream:
                                proc=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_dynamic_decoder_profile' if profile=='decoder' else 'release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=s.build_environment(root,settings),stdout=stream,stderr=subprocess.STDOUT)
                        finally:
                            if log.exists():numeric.write_text(''.join(line for line in log.read_text().splitlines(keepends=True) if line.startswith(('RSE_TIMING ','RSE_SAMPLE ','RSE_DYNAMIC_SUPPLEMENT ',PREFIX))))
                        proc.check_returncode();raw=numeric.read_text();rows=s.parse_output(raw);ledgers=s.parse_output(raw,PREFIX)
                        if len(ledgers)!=1:
                            (output/(name+'.partial.json')).write_text(json.dumps({'samples':rows,'dynamic':ledgers},indent=2)+'\n')
                            raise ValueError('missing complete dynamic allocation ledger')
                        member=output/(name+'.json')
                        if profile=='decoder':
                            headers=s.parse_output(raw,'RSE_DYNAMIC_SUPPLEMENT ')
                            if len(headers)!=1:raise ValueError('missing fixed decoder supplement header')
                            member.write_text(json.dumps({'supplement':headers[0],'dynamic':ledgers[0]},indent=2)+'\n')
                            summary=supplement_report(headers[0],ledgers[0],case,report['page_size'],report['cpu_affinity'],source)
                        else:
                            member.write_text(json.dumps({'samples':rows,'dynamic':ledgers[0]},indent=2)+'\n')
                            summary=worker_report(rows,ledgers[0],profile,case,source,report['page_size'],report['cpu_affinity'],mode)
                        unchanged()
                        entry=dict(summary,path=member.name,sha256=e.digest(member.read_bytes()),numeric_journal_path=numeric.name,numeric_journal_sha256=e.digest(numeric.read_bytes()),raw_log_sha256=e.digest(log.read_bytes()))
                        report['cases'][role][mode+'/'+profile+'/'+case]=entry
                        execution.write(json.dumps({'stage':'end','step':step,'context':context,'entry':entry})+'\n');execution.flush()
                if e.source_info(root)!=source:raise ValueError('original dynamic source was not restored')
        if e.source_info(repository)!=tool or dynamic_context(repository,env)!=context:raise ValueError('dynamic source/context changed before replay')
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
        result=verify(output,repository);(output/'replay.json').write_text(json.dumps(result,indent=2)+'\n');return result
    except BaseException as error:
        (output/'diagnostic-status.json').write_text(json.dumps({'status':'failed','release_approval':False,'original_records_preserved':True})+'\n')
        (output/'failure.json').write_text(json.dumps({'error':str(error),'release_approval':False})+'\n')
        raise
