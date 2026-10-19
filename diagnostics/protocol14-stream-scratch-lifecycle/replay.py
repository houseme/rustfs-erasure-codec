"""Read-only replay of numeric journals, stack accounting and the scoped control."""
import argparse
import collections
import hashlib
import json
import pathlib
import subprocess
import sys

parser=argparse.ArgumentParser();parser.add_argument('--tool',required=True,type=pathlib.Path);args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parent;tool=args.tool.resolve();sys.path.insert(0,str(tool/'scripts'))
import release_sampling as sampling
import release_evidence as evidence
import perf_stack_audit as stacks
import stream_allocator_audit as allocator

def load(path):return json.loads((root/path).read_text())
binding=load('analysis-binding.json')
assert hashlib.sha256((tool/'scripts/perf_stack_audit.py').read_bytes()).hexdigest()==binding['analyzer_sha256'],'use the bound analyzer revision'
assert hashlib.sha256((tool/'scripts/stream_allocator_audit.py').read_bytes()).hexdigest()==binding['allocator_analyzer_sha256'],'use the bound allocator analyzer revision'
source=subprocess.check_output(['git','show',binding['source']+':src/core/stream.rs'],cwd=tool,text=True)
anchors=stacks.source_anchors(source)
observations=0
for directory,names in [('fp',('faults','clock')),('lifecycle',('trace','dwarf')),('full_dwarf_rejected',('dwarf_full',)),('service64',('A1','A2'))]:
    metadata=load(directory+'/provenance.json');case=metadata['case'];count=metadata['iterations'];context=metadata['context']
    for name in names:
        rows=load(directory+'/'+name+'-samples.json');timing=load(directory+'/'+name+'-timing.json');numeric=(root/directory/(name+'.numeric.log')).read_text()
        assert rows==sampling.parse_output(numeric) and timing==sampling.parse_output(numeric,'RSE_TIMING ')
        assert len(rows)==len(timing)==12
        for row,journal in zip(rows,timing):assert all(row[k]==v for k,v in journal.items())
        normalize=lambda p,row,revision:next(iter(evidence.indexed_rows(p,[row],revision,{case:count},complete=False)))
        sampling.sample_groups(rows,'stream',binding['source'],{case},{case:count},normalize,context['isolation']['cpu_affinity'],context['runtime']['page_size'])
        observations+=len(rows)
for path in ('fp/faults-stacks.json','fp/clock-stacks.json','lifecycle/dwarf-stacks.json'):
    report=load(path);frames=report['frames'];roles={};unknown=empty=sample_count=period_sum=0
    assert report['source_anchors']==anchors
    stats_path={'fp/faults-stacks.json':'fp/faults-record-stats.txt','fp/clock-stacks.json':'fp/clock-record-stats.txt','lifecycle/dwarf-stacks.json':'lifecycle/dwarf-record-stats.txt'}[path]
    assert stacks.parse_record_stats((root/stats_path).read_text())==report['record_types']
    assert not any(report['record_types'].get(k,0) for k in stacks.BAD_RECORDS)
    for row in report['stacks']:
        chain=[frames[i] for i in row['frame_ids']]
        assert row['role']==stacks.classify(chain,anchors)
        group=roles.setdefault(row['role'],{'samples':0,'period_sum':0})
        for field in group:group[field]+=row[field]
        sample_count+=row['samples'];period_sum+=row['period_sum']
        if chain[0]['symbol'] in ('[unknown]','[unresolved-callchain]'):unknown+=row['samples']
        if chain[0]['symbol']=='[unresolved-callchain]':empty+=row['samples']
    assert roles==report['roles']
    assert sample_count==report['samples']==report['record_types']['PERF_RECORD_SAMPLE']
    assert sample_count==sum(row['samples'] for row in report['actual_period_histogram'])
    assert period_sum==report['period_sum']==sum(row['period']*row['samples'] for row in report['actual_period_histogram'])
    assert unknown==report['unresolved_leaf_samples'] and empty==report['empty_callchain_samples']
    assert report['release_approval'] is False and report['candidate_attribution'] is None
ledger=load('lifecycle/allocator-ledger.json')
assert sum(group['syscalls'][kind] for group in ledger['groups'].values() for kind in group['syscalls'])==len(ledger['events'])==ledger['measuring_task_vm_syscalls']
assert ledger['all_decoded_syscalls']==ledger['measuring_task_all_syscalls']+ledger['other_task_syscalls']
for interval,group in ledger['groups'].items():
    selected=[event for event in ledger['events'] if event['interval']==interval]
    assert collections.Counter(event['kind'] for event in selected)==group['syscalls']
    advice=collections.Counter()
    for event in selected:
        if event['success'] and event['kind']=='madvise':advice[event['advice']]+=event['bytes']
    assert advice==group['successful_madvise_bytes']
meta=load('service64/provenance.json');case=meta['case'];counts={case:64};selected={}
for phase in ('A1','A2'):
    rows=load('service64/'+phase+'-samples.json')
    normalize=lambda p,row,revision:next(iter(evidence.indexed_rows(p,[row],revision,counts,complete=False)))
    medians,widths=sampling.selected_row(rows,'stream','ns_per_block',binding['source'],{case},counts,normalize,meta['context']['isolation']['cpu_affinity'],meta['context']['runtime']['page_size'])
    assert all(row['central_spread']<=.10 for row in widths);selected[phase]=medians[0]
    assert widths==load('service64/capture-result.original.json')['phases'][phase]['spreads']
    if phase=='A1':fixture=sampling.fixture_catalog(rows);workload=sampling.workload_catalog(rows)
    else:assert fixture==sampling.fixture_catalog(rows) and workload==sampling.workload_catalog(rows)
evidence.compatible('stream',{case:selected['A1']},{case:selected['A2']})
a,b=(selected[p]['ns_per_block'] for p in ('A1','A2'));drift=abs(b-a)/a
assert drift<=.05 and drift==load('service64/capture-result.original.json')['repeat_drift']
assert load('full_dwarf_rejected/quality.json')['status']=='rejected'
for result in load('checks-and-restoration.json').values():assert all(result['restoration'][k] for k in ('child_removed','controller_restored','parent_affinities_restored'))
assert observations==84
print('84 complete observations, all journals/stack totals/VM groups and the single-case 64-call control replayed; no release approval.')
