"""Read-only replay of all native residency observations; no benchmark execution."""
import argparse,collections,hashlib,json,pathlib,sys,zipfile
p=argparse.ArgumentParser();p.add_argument('--tool',type=pathlib.Path,required=True);args=p.parse_args()
sys.path.insert(0,str(args.tool.resolve()/'scripts'))
import stream_view_audit as audit
import release_sampling as sampling
import release_evidence as evidence
root=pathlib.Path(__file__).resolve().parent
for line in (root/'SHA256SUMS').read_text().splitlines():
 digest,name=line.split('  ',1)
 if pathlib.Path(name).name!=name:raise ValueError('invalid checksum member')
 if hashlib.sha256((root/name).read_bytes()).hexdigest()!=digest:raise ValueError('checksum mismatch')
with zipfile.ZipFile(root/'records.zip') as z:
 def read(name):return json.loads(z.read(name))
 report=read('result.json');cases=sorted(evidence.expected_cases('stream'))
 assert report['capture_complete'] is True and report['cases']==report['completed_cases']==cases
 assert report['iterations']==6 and report['sampling']['version']==14
 assert report['source']['commit']==evidence.BASELINE_COMMIT
 assert audit.implementation_digest()==report['view_audit_sha256']
 observations=entries=0;counts=collections.Counter();point=collections.defaultdict(collections.Counter)
 for n,case in enumerate(cases):
  rows=read(f'{n}-samples.partial.json');views=read(f'{n}-views.partial.json');journals=read(f'{n}-timing.partial.json')
  assert len(rows)==len(views)==len(journals)==12
  sampling.validate_workload_sequence(audit.underlying_records(rows),'stream')
  for index,(row,view,journal) in enumerate(zip(rows,views,journals)):
   assert row['case']==case and row['index']==index and row['revision']==evidence.BASELINE_COMMIT
   assert all(row[k]==v for k,v in journal.items())
   audit.validate(view,case,6,4096,index,row['fixture']['source'],require_counters=True)
   sampling.validate_placement(row,report['context']['isolation']['cpu_affinity'])
   sampling.validate_telemetry(row,report['context']['isolation']['cpu_affinity'])
   observations+=1
   for value in view['residency']:
    snap=value['snapshot'];entries+=1;counts[value['boundary']+':'+snap['status']]+=1
    if case=='encode_stream:10:4:16777216:4194304' and value['boundary'] in {'after_reserve','before_read','after_api_return','after_writer_drop'}:
     point[value['boundary']+':'+value['role']][snap['status']+':'+str(snap['resident_pages'])]+=1
 result={'observations':observations,'api_calls':observations*6,'residency_records':entries,'boundary_status_counts':dict(sorted(counts.items())),'large_encode_histograms':{k:dict(sorted(v.items())) for k,v in sorted(point.items())},'release_approval':False,'candidate_attribution':None}
 assert result==json.loads((root/'summary.json').read_text())
 print(json.dumps(result,indent=2))
