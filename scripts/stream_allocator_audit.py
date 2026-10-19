"""Offline address-free allocator syscall ledger, bound to actual sampler writes."""
import argparse
import ast
import collections
import decimal
import json
import pathlib
import re
import release_sampling as sampling

PREFIX=re.compile(r'^\s*(\d+)\s+(\d+\.\d+)\s+(.*)$')
CALL=re.compile(r'^(brk|mmap|munmap|madvise|write)\((.*)\)\s+=\s+(.*?)\s+<(\d+\.\d+)>$')
RESUME=re.compile(r'^<\.\.\. (\w+) resumed>(.*)$')


def nanoseconds(value):return int(decimal.Decimal(value)*10**9)


def pointer(value):return 0 if value=='NULL' else int(value,16)


def decode_calls(lines):
    calls=[];pending={};signals=0
    for number,line in enumerate(lines,1):
        m=PREFIX.fullmatch(line.rstrip('\n'))
        if not m:raise ValueError(f'unrecognized syscall prefix at line {number}')
        tid,timestamp,body=m.groups();timestamp=nanoseconds(timestamp)
        if body.startswith(('--- ','+++ ')):signals+=1;continue
        resumed=RESUME.fullmatch(body)
        if resumed:
            if tid not in pending:raise ValueError('orphan resumed syscall')
            start,first=pending.pop(tid)
            if not first.startswith(resumed[1]+'('):raise ValueError('resumed syscall name mismatch')
            timestamp=start;body=first+resumed[2]
        elif '<unfinished ...>' in body:
            if tid in pending:raise ValueError('overlapping unfinished syscalls in one task')
            pending[tid]=(timestamp,body.replace('<unfinished ...>',''));continue
        if tid in pending:raise ValueError('new syscall before unfinished syscall resumed')
        match=CALL.fullmatch(body)
        if not match:raise ValueError(f'unrecognized syscall at line {number}')
        kind,args,result,duration=match.groups()
        calls.append({'tid':tid,'time':timestamp,'kind':kind,'args':args,'result':result,'duration_ns':nanoseconds(duration)})
    if pending:raise ValueError('incomplete syscall trace')
    return sorted(calls,key=lambda c:c['time']),signals


def audit_trace(lines,samples):
    if len(samples)!=12 or [r['index'] for r in samples]!=list(range(12)) or [r['kind'] for r in samples]!=['warmup']*3+['sample']*9:raise ValueError('complete original observation schedule required')
    calls,signals=decode_calls(lines)
    buffers=collections.defaultdict(bytes);markers=[]
    for call in calls:
        if call['kind']!='write':continue
        match=re.fullmatch(r'1, ("(?:\\.|[^"\\])*"), (\d+)',call['args'])
        if not match:continue # Other fds cannot contain the sampler stdout.
        count=int(call['result'].split()[0])
        if count<0:continue
        payload=ast.literal_eval(match[1]).encode('latin1')
        if count>len(payload):raise ValueError('truncated stdout write in trace')
        buffers[call['tid']]+=payload[:count]
        if len(buffers[call['tid']])>1048576:raise ValueError('bounded stdout reconstruction overflow')
        while b'\n' in buffers[call['tid']]:
            line,buffers[call['tid']]=buffers[call['tid']].split(b'\n',1)
            for kind,prefix in (('timing','RSE_TIMING '),('sample','RSE_SAMPLE ')):
                rows=sampling.parse_output(line.decode('utf8')+'\n',prefix)
                for row in rows:markers.append((call['time'],call['tid'],kind,row))
    if len(markers)!=24 or len({m[1] for m in markers})!=1:raise ValueError('missing or ambiguous measuring-thread markers')
    for i,row in enumerate(samples):
        timing,sample=markers[i],markers[12+i]
        if timing[2]!='timing' or sample[2]!='sample' or sample[3]!=row or any(row[k]!=v for k,v in timing[3].items()):raise ValueError('syscall/stdout/numeric journal mismatch')
    tid=markers[0][1];selected=[c for c in calls if c['tid']==tid and c['kind']!='write']
    def interval(time):
        if time<markers[0][0]:return 'startup_and_warmup_0'
        for i in range(1,12):
            if markers[i-1][0]<=time<markers[i][0]:return 'observation_'+str(i)
        if time<=markers[-1][0]:return 'deferred_export'
        return 'after_last_sample'
    maps=[];next_map=0;brk=None;events=[]
    def overlap(start,end):
        return [{'mapping_id':m['id'],'offset':max(start,m['start'])-m['origin'],'bytes':min(end,m['end'])-max(start,m['start']),'anonymous':m['anonymous']} for m in maps if max(start,m['start'])<min(end,m['end'])]
    def remove(start,end):
        remaining=[]
        for m in maps:
            if start>=m['end'] or end<=m['start']:remaining.append(m);continue
            if m['start']<start:remaining.append(dict(m,end=start))
            if end<m['end']:remaining.append(dict(m,start=end))
        maps[:]=remaining
    for call in selected:
        kind=call['kind'];args=call['args'].split(', ');result=call['result'].split()[0]
        event={'kind':kind,'interval':interval(call['time']),'duration_ns':call['duration_ns'],'success':not result.startswith('-1')}
        if kind=='brk':
            event['query']=args[0]=='NULL';event['delta_bytes']=None
            if event['success']:
                address=pointer(result);event['delta_bytes']=address-brk if brk is not None else None;brk=address
                event['request_satisfied']=event['query'] or pointer(args[0])==address
                event['success']=event['request_satisfied']
            else:event['request_satisfied']=False
        elif kind=='mmap':
            size=int(args[1]);event.update(bytes=size,protection_at_mmap=args[2],flags=args[3],anonymous='MAP_ANONYMOUS' in args[3],fixed='MAP_FIXED' in args[3].split('|'))
            if event['success']:
                address=pointer(result);event['mapping_id']=next_map;event['replaced']=overlap(address,address+size)
                if event['replaced'] and not event['fixed']:raise ValueError('unexpected overlapping mmap')
                remove(address,address+size);maps.append({'start':address,'end':address+size,'origin':address,'id':next_map,'anonymous':event['anonymous']});next_map+=1
        else:
            address=pointer(args[0]);size=int(args[1]);event['bytes']=size;event['matches']=overlap(address,address+size)
            if kind=='madvise':event['advice']=args[2]
            elif event['success']:remove(address,address+size)
        events.append(event)
    groups={}
    for event in events:
        group=groups.setdefault(event['interval'],{'syscalls':collections.Counter(),'successful_madvise_bytes':collections.Counter(),'successful_munmap_bytes':0,'brk_grow_bytes':0,'brk_shrink_bytes':0})
        group['syscalls'][event['kind']]+=1
        if not event['success']:continue
        if event['kind']=='madvise':group['successful_madvise_bytes'][event['advice']]+=event['bytes']
        if event['kind']=='munmap':group['successful_munmap_bytes']+=event['bytes']
        if event['kind']=='brk' and event['delta_bytes'] is not None:
            delta=event['delta_bytes'];group['brk_grow_bytes']+=max(0,delta);group['brk_shrink_bytes']+=max(0,-delta)
    return {'schema':'stream-allocator-ledger-v1','purpose':'observed VM operations and mapping lifetimes; no physical residency, exact timed-interval or throughput attribution',
            'scope':'task writing all original sampler markers; other traced tasks explicitly counted',
            'interval_scope':'between previous timing write and current timing write; includes boundary telemetry/journal costs; samples export only after all observations, first interval includes startup',
            'observations':12,'all_decoded_syscalls':len(calls),'measuring_task_all_syscalls':sum(c['tid']==tid for c in calls),'measuring_task_vm_syscalls':len(events),
            'coverage':{'mprotect_observed':False,'physical_residency_observed':False,'exact_api_call_boundaries':False},
            'other_task_syscalls':len(calls)-sum(c['tid']==tid for c in calls),'signal_or_exit_lines':signals,
            'groups':groups,'events':events,'release_approval':False,'candidate_attribution':None}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--trace',required=True,type=pathlib.Path);parser.add_argument('--samples',required=True,type=pathlib.Path);parser.add_argument('--output',required=True,type=pathlib.Path)
    args=parser.parse_args()
    if args.output.exists():raise ValueError('refuse to overwrite an existing ledger')
    with args.trace.open() as lines:report=audit_trace(lines,json.loads(args.samples.read_text()))
    with args.output.open('x') as out:json.dump(report,out,indent=2);out.write('\n')


if __name__=='__main__':main()
