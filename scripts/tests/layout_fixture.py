"""Synthetic layout records for refusal tests; never performance evidence."""
import copy
import json
import diagnose_verify_layout as diagnostic
import release_sampling as sampling


def records():
    result = []
    grid = [(a,b) for a in diagnostic.OFFSETS for b in diagnostic.OFFSETS]
    for length in diagnostic.LENGTHS:
        for order in (0,1):
            for a,b in (grid if order == 0 else reversed(grid)):
                for index in range(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']):
                    result.append({'len':length, 'pass':order, 'source_offset':a, 'scratch_offset':b,
                                   'kind':'warmup' if index < sampling.PROTOCOL['warmups'] else 'sample',
                                   'index':index, 'iterations':100, 'wall_ns':10000, 'ns_per_iter':100.0,
                                   'entry':'slice_views', 'fixture_lifetime':'same_backing_per_length',
                                   'input_page_offsets':[a]*6, 'scratch_page_offsets':[b]*2})
    return result


def kernel_policy_fixture():
    return {'kernel_release':'fixture-kernel','process_thp_disable':0,
            'thp':{'enabled':'madvise','defrag':'madvise','shmem_enabled':None,
                   'sizes_kib':{'16':{'anonymous':'never','shmem':None},'2048':{'anonymous':'inherit','shmem':None}}}}


def controlled_context():
    return {'compiler': 'rustc controlled', 'cpu_model': 'controlled CPU',
            'settings': dict(diagnostic.evidence.SAMPLING_ENV),
            'runtime': {'page_size': 4096, 'libc': {'name': 'glibc', 'version': 'test'},
                        'allocator_environment': {},'kernel_memory_policy':kernel_policy_fixture()},
            'isolation': {'scope': 'exclusive-vm-cpu-partition', 'cpu_affinity': [2,3],
                          'reserved_cpu_count': 2, 'physical_host_exclusivity_proven': False}}


def paired_records(iterations=1_000_000, context=None, provenance=None):
    rows = records()
    for row in rows:
        row.update(iterations=1_000_000, wall_ns=2_000_000_000, ns_per_iter=2000.,
                   input_4096_offsets=[row['source_offset']]*6, scratch_4096_offsets=[row['scratch_offset']]*2,
                   page_size=4096, cpu=2, backend='rust-scalar', backend_id='RustScalar',
                   backend_kind='Scalar', features='simd-accel|std', target_arch='x86_64', target_os='linux')
    context = controlled_context() if context is None else context
    for row in rows:
        row.update(iterations=iterations, ns_per_iter=row['wall_ns']/iterations,
                   cpu=min(context['isolation']['cpu_affinity']), page_size=context['runtime']['page_size'])
        if provenance is not None: row.update(provenance)
    return rows


BASELINE = {'commit': diagnostic.evidence.BASELINE_COMMIT, 'tree': 'a'*40, 'lock_sha256': 'a'*64}
CANDIDATE = {'commit': 'b'*40, 'tree': 'b'*40, 'lock_sha256': 'b'*64}


def write_phase(output, source, rows=None, *, tool=None, context=None, iterations=1_000_000, provenance=None):
    output.mkdir(parents=True)
    context = copy.deepcopy(controlled_context() if context is None else context)
    tool = CANDIDATE if tool is None else tool
    rows = paired_records(iterations, context, provenance) if rows is None else rows
    metadata = {'schema_version': diagnostic.PHASE_SCHEMA, 'source': source,
                'tool': tool, 'context': context, 'iterations': iterations,
                'measuring_cpu': min(context['isolation']['cpu_affinity']), 'release_approval': False, 'candidate_attribution': None,
                **diagnostic.tool_contract()}
    diagnostic.save(output, 'provenance.json', metadata)
    diagnostic.save(output, 'samples.json', rows)
    (output/'measurement.log').write_text(''.join(diagnostic.PREFIX+json.dumps(row)+'\n' for row in rows))
    return diagnostic.analyze(rows, iterations, context['runtime']['page_size'], min(context['isolation']['cpu_affinity']))


def write_campaign(output, *, sources=None, tool=None, context=None, iterations=1_000_000, provenance=None):
    sources = {'baseline': BASELINE, 'candidate': CANDIDATE} if sources is None else sources
    tool = CANDIDATE if tool is None else tool
    document = {'schema_version': diagnostic.PAIRED_SCHEMA, 'phase_order': list(diagnostic.evidence.PHASES),
                'sources': sources, 'tool': tool,
                'contract': diagnostic.tool_contract(), 'release_approval': False, 'files': {}}
    for phase in diagnostic.evidence.PHASES:
        write_phase(output/phase, sources['baseline' if phase.startswith('A') else 'candidate'], tool=tool, context=context, iterations=iterations, provenance=provenance)
        document['files'][phase] = {name: {'path': phase+'/'+name, 'sha256': diagnostic.evidence.digest((output/phase/name).read_bytes())}
                                    for name in ('provenance.json', 'samples.json', 'measurement.log')}
    diagnostic.save(output, 'manifest.json', document)
    return document


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
    residency=[]
    def resident(call,boundary,role,index,size,offset=16):
        pages=(offset+size+page_size-1)//page_size if size else 0
        residency.append({'call':call,'boundary':boundary,'role':role,'index':index,'bytes':size,'offset_page':offset if size else None,
                          'snapshot':{'status':'mapped' if size else 'empty','pages':pages,'resident_pages':pages,'errno':None}})
    for call in range(iterations+1):
        current=[e for e in events if e['call']==call]
        if call and operation=='encode_stream':
            for index in range(m):resident(call,'after_reserve','writer_before',index,size)
        for e in current:
            span=e['capacity'] if e['capacity'] is not None else e['len']
            if e['role']=='source_read_view':
                # Native queries precede each delegated read; source/scratch events follow it.
                scratch=current[current.index(e)+1]
                resident(call,'before_read','read_scratch_view',e['index'],scratch['len'])
            resident(call,'view',e['role'],e['index'],span, e['offsets'][2] if e['offsets'] else 0)
        if call and operation=='encode_stream':
            for role in ('read_scratch_view','parity_write_view','writer_after'):
                values={e['index']:e for e in current if e['role']==role}
                for index,e in sorted(values.items()):
                    resident(call,'after_writer_drop' if role=='writer_after' else 'after_api_return',role,index,e['capacity'] if e['capacity'] is not None else e['len'])
    return {'schema':'stream-buffer-views-v3','observation':observation,'iterations':iterations,'calls':iterations,'completed':iterations,'page_size':page_size,'overflow':0,'release_approval':False,'events':events,'windows':windows,'residency':residency}
