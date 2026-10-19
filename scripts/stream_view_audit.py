"""Diagnostic decorators for real stream buffer views; never formal release timing."""
import copy
import hashlib
import pathlib
import release_sampling as sampling

TEMPLATE = pathlib.Path(__file__).resolve().parent/'diagnostics/stream_views.rs'
PREFIX = 'RSE_STREAM_VIEWS '
ENTRIES = {"encode_stream":"Vec<stream_views::Reader<'_>> -> Vec<stream_views::Writer>",
           "verify_stream":"Vec<stream_views::Reader<'_>>",
           "reconstruct_stream":"Vec<Cursor<Vec<u8>>>"}


def implementation_digest():
    return hashlib.sha256(pathlib.Path(__file__).read_bytes()+TEMPLATE.read_bytes()).hexdigest()


def render(root):
    result = sampling.render(root)
    text = result['stream.rs']
    kernel = sampling.block(text,'fn execute_release_kernel')
    changed = kernel
    for pattern,operation in sampling.STREAM_KERNELS:
        original = sampling.original_loop(kernel,pattern)
        loop=original;edits=[]
        def edit(before,after):
            nonlocal loop
            if loop.count(before)!=1:raise ValueError('stream diagnostic adapter source changed')
            loop=loop.replace(before,after,1);edits.append((before,after))
        edit('for _ in 0..iterations {','for _ in 0..iterations {\n                super::stream_views::call();')
        if operation != 'reconstruct_stream':
            edit('let mut readers: Vec<&[u8]> = original.iter().map(Vec::as_slice).collect();',
                 'let mut readers: Vec<_> = original.iter().map(|v| super::stream_views::Reader(v.as_slice())).collect();\n                super::stream_views::readers(&readers);')
            if operation=='encode_stream':
                edit('reserved_stream_outputs(case.parity_shards, case.shard_size)','super::stream_views::outputs(case.parity_shards, case.shard_size)')
                edit('                rs.encode_stream(', '                super::stream_views::writers(&writers);\n                rs.encode_stream(')
        else:
            marker='                cursors[0] = Cursor::new(Vec::new());'
            edit(marker,'                super::stream_views::cursors(&cursors, 6);\n'+marker)
            marker='                assert_eq!(cursors[0].get_ref(), &original[0]);'
            edit(marker,marker+'\n                super::stream_views::cursors(&cursors, 7);')
            edit('                rs.reconstruct_stream(', '                super::stream_views::cursors(&cursors, 8);\n                rs.reconstruct_stream(')
        drops='drop(writers); drop(readers);' if operation=='encode_stream' else 'drop(readers);' if operation=='verify_stream' else 'drop(cursors);'
        edit('\n            }','\n                '+drops+'\n                super::stream_views::end_call();\n            }')
        restored=loop
        for before,after in reversed(edits):
            if restored.count(after)!=1:raise ValueError('stream diagnostic edit no longer reversible')
            restored=restored.replace(after,before,1)
        if restored!=original:raise ValueError('diagnostic changed an original non-observer statement')
        changed=changed.replace(original,loop,1)
    text=text.replace(kernel,changed,1)
    marker='        let elapsed = execute_release_kernel(self, iterations);'
    if text.count(marker)!=1:raise ValueError('stream preparation changed')
    text=text.replace(marker,'        super::stream_views::begin(&self.source, iterations, self.fixture.page_size());\n'+marker+'\n        super::stream_views::finish();',1)
    # Keep the raw entry declarations honest: this is an explicitly different,
    # transparent I/O adapter, never an original Vec performance observation.
    text=text.replace('"Vec<&[u8]> -> Vec<Vec<u8>>"','"'+ENTRIES['encode_stream']+'"')
    text=text.replace('"Vec<&[u8]>"','"'+ENTRIES['verify_stream']+'"')
    result['stream.rs']=text
    result['stream_views.rs']=TEMPLATE.read_text()
    marker='fn release_controlled_profile() {'
    if result['root.rs'].count(marker)!=1:raise ValueError('controlled worker entry changed')
    result['root.rs']=result['root.rs'].replace(marker,marker+' stream_views::enable();',1)
    result['root.rs']+='\n#[path="rse_release_sampling/stream_views.rs"] mod stream_views;\n'
    return result


def underlying_records(records):
    """Validate known adapter declarations, then reuse strict scalar/seed validators.

    Only a private copy is normalized. Published observations keep actual entries;
    formal verification rejects their entry/overlay differences.
    """
    result=copy.deepcopy(records)
    for row in result:
        operation=row['case'].split(':')[0]
        expected=ENTRIES[operation]
        for value in (row.get('fixture'),row.get('workload')):
            if not isinstance(value,dict) or value.get('call_entry')!=expected:
                raise ValueError('missing actual stream audit entry')
            value['call_entry']=sampling.fixture_contract('stream',row['case'])['call_entry']
    return result


def validate(view, case, iterations, page_size, observation, source_fixture=None):
    """Require every call, transferred byte, reservation and missing transition."""
    if set(view) != {'schema','observation','iterations','calls','completed','page_size','overflow','release_approval','events'} or view['schema']!='stream-buffer-views-v1' or view['release_approval'] is not False:
        raise ValueError('invalid stream view schema')
    expected={'observation':observation,'iterations':iterations,'calls':iterations,'completed':iterations,'page_size':page_size,'overflow':0}
    if any(type(view.get(k)) is not int or view[k]!=v for k,v in expected.items()):
        raise ValueError('incomplete call/drop schedule or bounded view overflow')
    operation,data,parity,size,block=case.split(':');data,parity,size,block=map(int,(data,parity,size,block))
    if operation not in ENTRIES or not 1<=data<data+parity<=48 or size<=0 or not 1024<=block<=16777216:raise ValueError('invalid stream audit shape')
    source_count=data if operation=='encode_stream' else data+parity
    events=view['events']
    if not isinstance(events,list) or not 1<=len(events)<=4096:raise ValueError('missing bounded stream events')
    roles={'retained_source','source_read_view','read_scratch_view','parity_write_view','writer_before','writer_after','cursor_before_missing','cursor_after_recovery','cursor_missing'}
    calls={i:[] for i in range(iterations+1)}
    for event in events:
        if set(event)!= {'call','role','index','len','capacity','offsets','bytes','same_address_as_previous_view'} or event['role'] not in roles:
            raise ValueError('invalid stream buffer view')
        if any(type(event[k]) is not int or event[k]<0 for k in ('call','index','len','bytes')) or event['call'] not in calls:
            raise ValueError('invalid stream event counters')
        role=event['role'];owned=role in {'retained_source','writer_before','writer_after','cursor_before_missing','cursor_after_recovery','cursor_missing'}
        cap=event['capacity'];length=event['len']
        if owned and (type(cap) is not int or not length<=cap<=2**64-1) or not owned and cap is not None:
            raise ValueError('owned capacity must not be inferred from a borrowed view')
        storage=(cap>0) if owned else length>0
        offsets=event['offsets']
        if storage:
            if not isinstance(offsets,list) or len(offsets)!=3 or any(type(n) is not int for n in offsets) or not all(0<=n<mod for n,mod in zip(offsets,(64,4096,page_size))) or offsets[1]%64!=offsets[0] or offsets[2]%min(4096,page_size)!=offsets[1]%min(4096,page_size):
                raise ValueError('invalid actual view offsets')
        elif offsets is not None:raise ValueError('empty storage must not export dangling positions')
        if event['same_address_as_previous_view'] is not None and type(event['same_address_as_previous_view']) is not bool:
            raise ValueError('invalid private identity comparison')
        limit=parity if role.startswith('writer') or role=='parity_write_view' else source_count
        if event['index']>=limit:raise ValueError('stream role/index mismatch')
        if event['bytes']>length and role!='writer_after':raise ValueError('transfer exceeds actual view')
        calls[event['call']].append(event)
    if [e['call'] for e in events]!=sorted(e['call'] for e in events):raise ValueError('stream calls reordered')
    retained=calls[0]
    if len(retained)!=source_count or [e['index'] for e in retained]!=list(range(source_count)) or any(e['role']!='retained_source' or e['len']!=size for e in retained):raise ValueError('missing retained source views')
    if source_fixture is not None:
        actual=[{'len':e['len'],'capacity':e['capacity'],'offset_64':e['offsets'][0],
                 'offset_4096':e['offsets'][1],'offset_page':e['offsets'][2]} for e in retained]
        if actual!=source_fixture:raise ValueError('stream view source differs from retained caller fixture')
    for index in range(1,iterations+1):
        current=calls[index]
        if operation=='reconstruct_stream':
            if [e['role'] for e in current]!=['cursor_before_missing']*source_count+['cursor_missing']*source_count+['cursor_after_recovery']*source_count:raise ValueError('cursor stages reordered')
            for role in ('cursor_before_missing','cursor_missing','cursor_after_recovery'):
                selected=[e for e in current if e['role']==role]
                if len(selected)!=source_count or {e['index'] for e in selected}!=set(range(source_count)):raise ValueError('missing cursor lifecycle stage')
                for event in selected:
                    missing=role=='cursor_missing' and event['index'] in (0,data)
                    if event['len']!=(0 if missing else size) or missing and event['capacity']!=0:raise ValueError('wrong missing/recovered cursor storage')
            if len(current)!=3*source_count:raise ValueError('unexpected cursor observations')
            continue
        read_events=[e for e in current if e['role'] in {'source_read_view','read_scratch_view'}]
        if len(read_events)%2 or any(a['role']!='source_read_view' or b['role']!='read_scratch_view' or a['index']!=b['index'] or a['bytes']!=b['bytes'] for a,b in zip(read_events[::2],read_events[1::2])):raise ValueError('reader/scratch callback pairs changed')
        for role in ('source_read_view','read_scratch_view'):
            for shard in range(source_count):
                selected=[e for e in current if e['role']==role and e['index']==shard]
                if not selected or sum(e['bytes'] for e in selected)!=size or not any(e['bytes']==0 for e in selected) or any(e['len']>block for e in selected if role=='read_scratch_view'):
                    raise ValueError('incomplete reader/scratch transfer')
        for shard in range(source_count):
            remaining=size
            origin=retained[shard]['offsets']
            for event in (e for e in current if e['role']=='source_read_view' and e['index']==shard):
                if event['len']!=remaining:raise ValueError('source prefix consumption changed')
                if remaining and event['offsets']!=[(start+size-remaining)%mod for start,mod in zip(origin,(64,4096,page_size))]:
                    raise ValueError('source view moved outside retained backing')
                remaining-=event['bytes']
        if operation=='verify_stream':
            if any(e['role'] not in {'source_read_view','read_scratch_view'} for e in current):raise ValueError('unexpected verify view')
            continue
        if any(e['role'] not in {'source_read_view','read_scratch_view','parity_write_view','writer_before','writer_after'} for e in current):raise ValueError('unexpected encode view')
        for shard in range(parity):
            inputs=[e for e in current if e['role']=='parity_write_view' and e['index']==shard]
            before=[e for e in current if e['role']=='writer_before' and e['index']==shard]
            after=[e for e in current if e['role']=='writer_after' and e['index']==shard]
            if not inputs or len(inputs)!=len(before) or len(before)!=len(after) or sum(e['bytes'] for e in inputs)!=size:raise ValueError('incomplete parity/writer transfer')
            previous=0
            for source,old,new in zip(inputs,before,after):
                if not 0<source['len']<=block or old['len']!=previous or new['len']!=old['len']+source['bytes'] or new['bytes']!=source['bytes'] or old['capacity']<size or old['capacity']!=new['capacity'] or old['offsets']!=new['offsets']:
                    raise ValueError('writer reservation or append lifecycle changed')
                previous=new['len']
            if previous!=size:raise ValueError('incomplete writer output')


def check(repository, baseline_ref=None):
    """Compile both untouched codecs and exercise every stream case structurally."""
    import os
    import subprocess
    import tempfile
    from diagnostic_overlay import overlay
    import release_evidence as evidence
    def source(root):
        settings=evidence.capture_environment()
        with overlay(root,render(root)):
            subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel',
                            '--test','rse_release_sampling','stream_views','--','--nocapture'],
                           cwd=root,env=sampling.build_environment(root,settings),check=True)
            result=[]
            for case in sorted(evidence.expected_cases('stream')):
                env=sampling.worker_environment(dict(settings,RSE_SAMPLER_PROFILE='stream',
                    RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='1',
                    RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(sampling.base_page_size())))
                proc=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel',
                    '--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],
                    cwd=root,env=sampling.build_environment(root,env),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
                if proc.returncode:print(proc.stdout,end='');proc.check_returncode()
                rows=sampling.parse_output(proc.stdout);views=sampling.parse_output(proc.stdout,PREFIX)
                if len(rows)!=12 or len(views)!=12:raise ValueError('incomplete native view worker')
                sampling.validate_workload_sequence(underlying_records(rows),'stream')
                for index,(row,view) in enumerate(zip(rows,views)):
                    if row['case']!=case or row['index']!=index:raise ValueError('native view worker case/order mismatch')
                    validate(view,case,1,sampling.base_page_size(),index,row['fixture']['source'])
                    if hasattr(os,'sched_getaffinity'):
                        sampling.validate_placement(row,os.sched_getaffinity(0))
                        sampling.validate_telemetry(row,os.sched_getaffinity(0))
                result.append(case)
            print('All 15 stream view workers executed; structural/semantic checks only, no performance approval')
            return result
    current=source(repository)
    if baseline_ref:
        with tempfile.TemporaryDirectory(prefix='rse-stream-view-baseline-') as directory:
            baseline=pathlib.Path(directory)/'baseline'
            sampling.checkout_baseline(repository,baseline,baseline_ref)
            if current!=source(baseline):raise ValueError('stream view catalogs differ between sources')
