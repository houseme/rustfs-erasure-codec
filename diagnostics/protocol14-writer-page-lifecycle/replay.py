import json,collections,statistics,pathlib,argparse,sys
parser=argparse.ArgumentParser();parser.add_argument('--tool',type=pathlib.Path,required=True);args=parser.parse_args()
sys.path.insert(0,str(args.tool.resolve()/'scripts'))
import stream_view_audit as audit
r=pathlib.Path(__file__).resolve().parent
raw=json.loads((r/'views.partial.json').read_text());summary=json.loads((r/'summary.json').read_text())
rows_raw=json.loads((r/'samples.partial.json').read_text());journal=json.loads((r/'timing.partial.json').read_text());provenance=json.loads((r/'provenance.json').read_text())
assert len(raw)==len(rows_raw)==len(journal)==len(summary['observations'])==12
assert [row['kind'] for row in rows_raw]==['warmup']*3+['sample']*9
for i,(view,row,timing) in enumerate(zip(raw,rows_raw,journal)):
    audit.validate(view,provenance['case'],provenance['iterations'],provenance['context']['runtime']['page_size'],i,row['fixture']['source'],require_counters=True)
    assert all(row[k]==v for k,v in timing.items())
    assert audit.summarize_windows(view)==summary['observations'][i]['calls']
rows=[];fault_patterns=collections.Counter();histograms=[]
for observation,view in zip(summary['observations'],raw):
 groups={p:{key:sum(c['windows'][p][key] for c in observation['calls']) for key in observation['calls'][0]['windows'][p]} for p in observation['calls'][0]['windows']}
 rows.append({'index':observation['index'],'kind':observation['kind'],'outer_wall_ns':observation['wall_ns'],'windows':groups})
 if observation['kind']=='sample':
  perwriter=collections.Counter()
  for window in view['windows']:
   if window['phase']=='append':perwriter[(window['call'],window['index'])]+=window['after'][0]-window['before'][0]
  histograms.append({'observation':observation['index'],'writer_append_fault_histogram':[{'minor_faults':k,'writer_lifetimes':v} for k,v in sorted(collections.Counter(perwriter.values()).items())]})
  for call in observation['calls']:
   g=call['windows'];fault_patterns[(g['reserve']['process_minor_faults'],g['append']['process_minor_faults'],g['outside_writer_windows']['process_minor_faults'])]+=1
result={'purpose':'all observations and descriptive per-call window decomposition; not a controlled speedup or release qualification','observations':rows,'measured_call_fault_patterns':[dict(zip(('reserve_minor','append_minor','outside_minor','calls'),(*key,n))) for key,n in sorted(fault_patterns.items())],'all_measured_writer_lifetime_histograms':histograms,'release_approval':False,'candidate_attribution':None}
assert result==json.loads((r/'derived.json').read_text())
print('All 12 observations, 72 calls, numeric journals and derived partitions match.')
for row in rows:
 g=row['windows']; print('|',row['index'],row['kind'],'|',f"{row['outer_wall_ns']/1e6:.3f}",'|',' | '.join(f"{g[p]['wall_ns']/1e6:.3f}" for p in ('reserve','append','drop','outside_writer_windows')),'|',g['append']['process_minor_faults'],'/',g['outside_writer_windows']['process_minor_faults'],'|')
