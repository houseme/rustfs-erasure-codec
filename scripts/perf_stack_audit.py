"""Offline, address-free software-event stack accounting; never release approval."""
import argparse
import collections
import hashlib
import json
import pathlib
import re
import sys

HEADER = re.compile(r'^\s*(\d+)\.(\d{1,9}):\s+(\d+)\s+(\S+):\s*$')
FRAME = re.compile(r'^\s+([0-9a-fA-F]+)\s+(.+)\s+\((.*)\)\s*$')
INLINE_FRAME = re.compile(r'^\s+([0-9a-fA-F]+)\s+(.+)\s*$')
BAD_RECORDS = ('PERF_RECORD_LOST','PERF_RECORD_LOST_SAMPLES','PERF_RECORD_THROTTLE','PERF_RECORD_UNTHROTTLE')
MAX_FRAMES = 256
MAX_STACKS = 200000
MAX_UNIQUE_FRAMES = 200000


def validate_recording_log(text, returncode):
    # perf can exit zero after reporting corrupted compressed output. A process
    # exit status alone is not proof that its captured data can be audited.
    bad=re.search(r'failed to|couldn.t decompress|data corruption|bad address|lost \d+ (?:chunks|samples)|check io/cpu overload',text,re.I)
    if returncode!=0 or bad or 'Captured and wrote' not in text:
        raise ValueError('unusable perf capture; retain original log and data')
    return {'returncode':returncode,'capture_message_observed':True,'fatal_diagnostics':False}


def parse_record_stats(text):
    """Read perf's C-side aggregate totals without dumping user stack bytes."""
    started=False;total=None;counts={}
    for line in text.splitlines():
        if not started:
            if line.strip()=='Aggregated stats:':started=True
            continue
        if not line.strip():continue
        if re.fullmatch(r'[^ \t].+ stats:',line):break
        match=re.fullmatch(r'\s*([A-Z0-9_]+) events:\s+(\d+)(?:\s+\(\s*\d+(?:\.\d+)?%\))?\s*',line)
        if not match:raise ValueError('unrecognized aggregate record statistic')
        name,value=match.groups();value=int(value)
        if name=='TOTAL':
            if total is not None:raise ValueError('duplicate aggregate total')
            total=value
        else:
            key='PERF_RECORD_'+name
            if key in counts:raise ValueError('duplicate aggregate record type')
            counts[key]=value
    if not started or not total or total!=sum(counts.values()):raise ValueError('incomplete aggregate record statistics')
    return counts


def source_anchors(text):
    """Bind classification to exact initialization statements, not guessed ranges."""
    lines=text.splitlines()
    read=[i for i,line in enumerate(lines,1) if 'core::cmp::Ordering::Less => buf.resize(max_len, 0)' in line]
    start=text.index('pub fn encode_stream(')
    end=text.index('pub fn verify_stream(',start)
    first=text[:start].count('\n')+1
    parity=[first+i for i,line in enumerate(text[start:end].splitlines()) if 'buf.resize(actual_len, 0);' in line]
    body=text[start:end]
    write=[first+i for i,line in enumerate(body.splitlines()) if line.strip()=='write_block_with_mode(']
    closes=[first+i for i,line in enumerate(body.splitlines()) if line=='    }']
    if len(read)!=1 or len(parity)!=1 or len(write)!=1 or not closes:raise ValueError('stream initialization source anchors changed')
    return {'path':'src/core/stream.rs','sha256':hashlib.sha256(text.encode()).hexdigest(),
            'read_initialization':read[0],'parity_initialization':parity[0],'writer_call':write[0],'encode_return':closes[-1]}


def public_source(value):
    value=value.strip().removesuffix(' (inlined)')
    match=re.fullmatch(r'(.+):(\d+)(?: \(discriminator \d+\))?',value)
    if not match:return None
    name,line=match.groups()
    if name in ('??','[unknown]'):return None
    registry=re.search(r'/registry/src/[^/]+/(.+)$',name)
    if registry:return 'registry/'+registry.group(1),int(line)
    for prefix in ('library/','tests/','benches/','src/'):
        if name.startswith(prefix):break
        if '/'+prefix in name:
            name=prefix+name.rsplit('/'+prefix,1)[1]
            break
    else:
        name=pathlib.PurePosixPath(name).name
    return name,int(line)


def function_name(symbol):
    # Rust generic arguments contain other function/closure names. They do not
    # prove that those functions are active frames in this callchain.
    depth=0;name=[]
    for char in symbol:
        if char=='<':depth+=1
        elif char=='>' and depth:depth-=1
        elif not depth:name.append(char)
    return ''.join(name)


def classify(frames, anchors):
    locations={(f['source'],f['line']) for f in frames}
    if (anchors['path'],anchors['read_initialization']) in locations:return 'read_scratch_initialization'
    if (anchors['path'],anchors['parity_initialization']) in locations:return 'parity_scratch_initialization'
    names=[function_name(f['symbol']) for f in frames]
    stream={name.rsplit('::',1)[-1] for f,name in zip(frames,names)
            if name.startswith('rustfs_erasure_codec::core::stream::') or f['source']==anchors['path']}
    if stream & {'read_one_stream','prepare_read_buffer','read_block_par','read_block_with_mode'}:return 'stream_read_path'
    if stream & {'write_block','write_block_par','write_block_with_mode'} or (anchors['path'],anchors['writer_call']) in locations:return 'stream_write_path'
    freeing=any(name.rsplit('::',1)[-1] in {'__rust_dealloc','dealloc','__GI___libc_free','__libc_free','_int_free','free','__munmap','munmap','madvise'} for name in names)
    vm_release=any(f['module']=='[kernel.kallsyms]' and function_name(f['symbol']).startswith(('zap_','unmap_','free_pages_','tlb_finish_')) for f in frames)
    if 'encode_stream' in stream and (freeing or vm_release and (anchors['path'],anchors['encode_return']) in locations):return 'stream_encode_cleanup'
    if 'encode_stream' in stream:return 'other_stream_encode'
    if any(f['source'] and f['source'].startswith('src/') and function_name(f['symbol']).rsplit('::',1)[-1].startswith('rust_gfni_') for f in frames):return 'codec_arithmetic'
    return 'other_or_unattributed'


def audit(lines, record_types, anchors, expected_event):
    if expected_event not in ('minor-faults:u','cpu-clock','cpu-clock:u'):raise ValueError('unsupported software profile event')
    if type(record_types) is not dict or any(type(v) is not int or v<0 for v in record_types.values()):raise ValueError('invalid raw record counts')
    if any(record_types.get(k,0) for k in BAD_RECORDS):raise ValueError('lost or throttled software profile; retain raw data')
    if type(record_types.get('PERF_RECORD_SAMPLE')) is not int or record_types['PERF_RECORD_SAMPLE']<=0:raise ValueError('raw sample count is required')
    stacks={};frame_index={};frame_table=[];periods=collections.Counter();samples=0;last_time=-1;current=None
    def flush():
        nonlocal samples,current
        if current is None:return
        period,frames=current
        if not frames:
            frames=[{'symbol':'[unresolved-callchain]','module':None,'source':None,'line':None}]
        ids=[]
        for frame in frames:
            identity=(frame['symbol'],frame['module'],frame['source'],frame['line'])
            if identity not in frame_index:
                if len(frame_table)>=MAX_UNIQUE_FRAMES:raise ValueError('bounded unique frame inventory overflow')
                frame_index[identity]=len(frame_table);frame_table.append(frame)
            ids.append(frame_index[identity])
        key=tuple(ids)
        if key not in stacks:
            if len(stacks)>=MAX_STACKS:raise ValueError('bounded stack inventory overflow')
            stacks[key]={'frame_ids':ids,'samples':0,'period_sum':0,'role':classify(frames,anchors)}
        stacks[key]['samples']+=1;stacks[key]['period_sum']+=period
        periods[period]+=1;samples+=1;current=None
    for line_number,line in enumerate(lines,1):
        if not line.strip():flush();continue
        match=HEADER.fullmatch(line.rstrip('\n'))
        if match:
            flush();seconds,fraction,period,event=match.groups()
            timestamp=int(seconds)*10**9+int(fraction.ljust(9,'0'));period=int(period)
            if timestamp<last_time or period<=0 or event!=expected_event:raise ValueError(f'invalid time/period/event at line {line_number}')
            last_time=timestamp;current=(period,[]);continue
        match=FRAME.fullmatch(line.rstrip('\n'))
        inline=None if match else INLINE_FRAME.fullmatch(line.rstrip('\n'))
        if match or inline:
            if current is None:raise ValueError(f'orphan frame at line {line_number}')
            if match:_,symbol,module=match.groups()
            else:
                _,symbol=inline.groups();module=None
            # IPs and binary offsets never enter the public inventory. Unresolved
            # frames remain explicit; merging them does not remove any sample.
            if re.fullmatch(r'(?:0x)?[0-9a-fA-F]+',symbol):symbol='[unknown]'
            module=pathlib.PurePosixPath(module).name if module else None
            if module=='worker' or module and re.fullmatch(r'rse_release_sampling-[0-9a-f]+',module):module='codec-worker'
            current[1].append({'symbol':symbol,'module':module,'source':None,'line':None})
            if len(current[1])>MAX_FRAMES:raise ValueError('bounded frame inventory overflow')
            continue
        if current is not None and current[1] and line.startswith('  '):
            source=public_source(line)
            if source:
                frame=current[1][-1]
                if frame['source'] is not None:raise ValueError(f'duplicate source location at line {line_number}')
                frame['source'],frame['line']=source
            elif not re.fullmatch(r'\s*(?:\S+\[[0-9a-fA-F]+\]|\?\?:[?0]|\[unknown\])(?: \(inlined\))?\s*',line):
                raise ValueError(f'unrecognized source location at line {line_number}')
            continue
        raise ValueError(f'unrecognized perf script input at line {line_number}')
    flush()
    if samples!=record_types['PERF_RECORD_SAMPLE']:raise ValueError('decoded/raw sample conservation failed')
    inventory=sorted(stacks.values(),key=lambda r:(-r['samples'],r['frame_ids']))
    roles={};unresolved=0
    for row in inventory:
        total=roles.setdefault(row['role'],{'samples':0,'period_sum':0})
        total['samples']+=row['samples'];total['period_sum']+=row['period_sum']
        if frame_table[row['frame_ids'][0]]['symbol'] in ('[unknown]','[unresolved-callchain]'):unresolved+=row['samples']
    return {'schema':'software-stack-inventory-v1','purpose':'descriptive stack/event accounting; not throughput or unique fault causality',
            'event':expected_event,'record_types':record_types,'samples':samples,
            'period_sum':sum(p*n for p,n in periods.items()),
            'actual_period_histogram':[{'period':p,'samples':n} for p,n in sorted(periods.items())],
            'unresolved_leaf_samples':unresolved,
            'empty_callchain_samples':sum(row['samples'] for row in inventory if frame_table[row['frame_ids'][0]]['symbol']=='[unresolved-callchain]'),'source_anchors':anchors,'roles':roles,'frames':frame_table,'stacks':inventory,
            'release_approval':False,'candidate_attribution':None}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input',required=True,type=pathlib.Path)
    parser.add_argument('--records',required=True,type=pathlib.Path)
    parser.add_argument('--record-log',required=True,type=pathlib.Path)
    parser.add_argument('--record-exit-code',required=True,type=int)
    parser.add_argument('--source',required=True,type=pathlib.Path)
    parser.add_argument('--event',required=True)
    parser.add_argument('--output',required=True,type=pathlib.Path)
    args=parser.parse_args()
    if args.output.exists():raise ValueError('refuse to overwrite an existing inventory')
    quality=validate_recording_log(args.record_log.read_text(),args.record_exit_code)
    with args.input.open() as lines:
        result=audit(lines,json.loads(args.records.read_text())['record_types'],source_anchors(args.source.read_text()),args.event)
    result['recording_quality']=quality
    with args.output.open('x') as out:json.dump(result,out,indent=2);out.write('\n')


if __name__=='__main__':main()
