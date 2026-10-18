"""Scoped retained-Vec controls and complete layout grid; never release evidence."""
import argparse, hashlib, json, os, pathlib, statistics, subprocess, sys
parser=argparse.ArgumentParser();parser.add_argument('--tool',type=pathlib.Path,required=True);parser.add_argument('--baseline',type=pathlib.Path,required=True);parser.add_argument('--output',type=pathlib.Path,required=True);parser.add_argument('--group',type=pathlib.Path,required=True);args=parser.parse_args()
sys.path.insert(0,str(args.tool/'scripts'))
import release_evidence as evidence
import release_sampling as sampling
import diagnose_verify_layout as grid
out=args.output;out.mkdir(mode=0o700)
env={k:v for k,v in os.environ.items() if not k.startswith(('RSE_','RS_','CARGO_PROFILE_')) and k not in ('CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER')};env.update(evidence.SAMPLING_ENV)
assert not any(k.startswith(('MALLOC_','LD_')) or k=='GLIBC_TUNABLES' for k in env)
source=evidence.source_info(args.baseline);tool=evidence.source_info(args.tool)
assert source['commit']==evidence.BASELINE_COMMIT
context=evidence.environment_context(args.baseline,env,args.group);measuring=min(context['isolation']['cpu_affinity'])
cases={c for c in evidence.expected_cases('small_file') if c.startswith('verify_with_buffer:')};assert len(cases)==11
report={'schema_version':'retained-vec-controls-v1','purpose':'scoped baseline diagnosis only; never release acceptance','candidate_attribution':None,'release_approval':False,'source':source,'tool':tool,'driver_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'context':context,'measuring_affinity':[measuring],'protocol':sampling.PROTOCOL,'renderer_sha256':sampling.implementation_digest(),'overlay_sha256':sampling.overlay_digest(args.baseline),'case_catalog':sorted(cases),'unit_tests':{},'phases':{},'layout_mismatches':[],'repeat_checks':[]}
def save(name,value):
 p=out/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(value,indent=2)+'\n')
def unchanged():
 assert evidence.source_info(args.baseline)==source and evidence.source_info(args.tool)==tool
 assert evidence.environment_context(args.baseline,env,args.group)==context
def profile(mode,log,counts=None):
 affinity=os.sched_getaffinity(0)
 try:
  os.sched_setaffinity(0,{measuring})
  return sampling.run_profile(args.baseline,env,'small_file',mode,log,counts,cases=cases)
 finally:os.sched_setaffinity(0,affinity)
save('provenance.json',report)
owned=[]
try:
 for role,root in (('baseline',args.baseline),('candidate',args.tool)):
  assert sampling.overlay_digest(root)==report['overlay_sha256']
  sampling.generate(root);owned.append(root)
  command=['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--','--skip','release_controlled_profile']
  with (out/(role+'-unit-tests.log')).open('x') as log:subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  text=(out/(role+'-unit-tests.log')).read_text();assert '7 passed; 0 failed' in text
  report['unit_tests'][role]={'actual_passed':7,'failed':0}
  unchanged();save('diagnosis.partial.json',report)
 records=profile('calibrate',out/'calibration.log');save('calibration-samples.json',records)
 counts={}
 for r in records:
  sampling.validate_telemetry(r,{measuring});sampling.validate_fixture(r,'small_file')
  sampling.validate_timing(r,'small_file',r['iterations'])
  assert set(evidence.indexed_rows('small_file',r['rows'],source['commit'],{r['case']:r['iterations']},complete=False))=={r['case']}
  if r['complete']:counts[r['case']]=r['iterations']
 assert set(counts)==cases
 countfile=out/'counts.tsv';countfile.write_text(''.join(f'{k}\t{v}\n' for k,v in sorted(counts.items())))
 final_layouts=sampling.fixture_catalog(records,calibration=True);medians={};layouts={}
 for phase in ('A1','A2'):
  folder=out/phase;folder.mkdir()
  rows=profile('sample',folder/'measurement.log',countfile);save(phase+'/samples.json',rows)
  normalize=lambda p,row,rev:next(iter(evidence.indexed_rows(p,[row],rev,counts,complete=False)))
  groups=sampling.sample_groups(rows,'small_file',source['commit'],cases,counts,normalize,{measuring})
  layouts[phase]=sampling.fixture_catalog(rows);medians[phase]={};results=[]
  for case,group in groups.items():
   values=sorted(r['timed_ns']/r['iterations'] for r in group[sampling.PROTOCOL['warmups']:]);median=statistics.median(values);medians[phase][case]=median
   results.append({'case':case,'median_ns':median,'central_spread':(values[-2]-values[1])/median,'short_samples':sum(r['wall_ns']<sampling.PROTOCOL['minimum_wall_ns'] or r['timed_ns']<sampling.PROTOCOL['minimum_timed_ns'] for r in group[sampling.PROTOCOL['warmups']:])})
   if layouts[phase][case]!=final_layouts[case]:report['layout_mismatches'].append({'case':case,'pair':'calibration/'+phase})
  report['phases'][phase]={'case_count':len(groups),'formal_samples':len(groups)*sampling.PROTOCOL['samples'],'cases':results}
  unchanged();save('diagnosis.partial.json',report)
 for case in sorted(cases):
  if layouts['A1'][case]!=layouts['A2'][case]:report['layout_mismatches'].append({'case':case,'pair':'A1/A2'})
  else:report['repeat_checks'].append({'case':case,'drift':abs(medians['A2'][case]/medians['A1'][case]-1)})
 report['repeat_failures']=[r for r in report['repeat_checks'] if r['drift']>.05]
 unchanged();save('diagnosis.partial.json',report)
 grid.capture(args.baseline,out/'layout-grid',args.group,262144)
 report['grid_completed']=True
 report['scope_complete']=True
 unchanged();save('diagnosis.json',report)
finally:
 for root in owned:
  directory=root/'tests/rse_release_sampling';expected=set(sampling.render(root))-{'root.rs'}
  assert {p.name for p in directory.iterdir()}==expected
  for path in directory.iterdir():path.unlink()
  directory.rmdir();(root/'tests/rse_release_sampling.rs').unlink()
