"""Complete baseline controls with formal basic telemetry; never release evidence."""
import argparse,hashlib,json,os,pathlib,statistics,subprocess,sys
parser=argparse.ArgumentParser();parser.add_argument('--tool',type=pathlib.Path,required=True);parser.add_argument('--baseline',type=pathlib.Path,required=True);parser.add_argument('--output',type=pathlib.Path,required=True);parser.add_argument('--group',type=pathlib.Path,required=True);args=parser.parse_args()
sys.path.insert(0,str(args.tool/'scripts'))
import release_evidence as evidence
import release_sampling as sampling
output=args.output;output.mkdir(mode=0o700)
env={k:v for k,v in os.environ.items() if not k.startswith(('RSE_','RS_','CARGO_PROFILE_')) and k not in ('CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER')};env.update(evidence.SAMPLING_ENV)
assert 'RSE_SAMPLER_DIAGNOSTIC_PROCESS' not in env
source=evidence.source_info(args.baseline);assert source['commit']==evidence.BASELINE_COMMIT
context=evidence.environment_context(args.baseline,env,args.group)
report={'schema_version':'baseline-basic-controls-v1','purpose':'complete baseline prerequisites only; never release acceptance','candidate_attribution':None,'observer':'basic numeric telemetry; identical worker instrumentation to formal capture','source':source,'tool':evidence.source_info(args.tool),'driver_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'protocol':sampling.PROTOCOL,'renderer_sha256':sampling.implementation_digest(),'overlay_sha256':sampling.overlay_digest(args.baseline),'context':context,'phases':{},'repeat_checks':[],'repeat_failures':[],'layout_mismatches':[],'release_approval':False}
def save(name,value): (output/name).write_text(json.dumps(value,indent=2)+'\n')
def unchanged(): assert evidence.source_info(args.baseline)==source and evidence.environment_context(args.baseline,env,args.group)==context
owned=False
try:
 save('provenance.json',report);sampling.generate(args.baseline);owned=True
 with (output/'build.log').open('x') as log:subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run'],cwd=args.baseline,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 counts={};calibration_layouts={};phase_layouts={};folder=output/'calibration';folder.mkdir()
 for profile in evidence.FILES:
  records=sampling.run_profile(args.baseline,env,profile,'calibrate',folder/(profile+'.log'),cases=evidence.expected_cases(profile));save('calibration/'+profile+'-samples.json',records)
  counts[profile]=evidence.calibration_counts(records,profile,source['commit'],context['runtime']['page_size']);calibration_layouts[profile]=sampling.fixture_catalog(records,calibration=True);unchanged()
  (output/('counts-'+profile+'.tsv')).write_text(''.join(f'{case}\t{count}\n' for case,count in sorted(counts[profile].items())))
 medians={}
 for phase in ('A1','A2'):
  if phase=='A2' and (report['layout_mismatches'] or any(p['short_samples'] or p['wide_cases'] for p in report['phases']['A1'].values())):
   report.update(baseline_controls_passed=False,repeat_completed=False,stop_reason='complete first phase failed quality; no repeat or candidate attribution');save('diagnosis.json',report);raise SystemExit(0)
  medians[phase]={};phase_layouts[phase]={};report['phases'][phase]={};folder=output/phase;folder.mkdir()
  for profile in evidence.FILES:
   records=sampling.run_profile(args.baseline,env,profile,'sample',folder/(profile+'.log'),output/('counts-'+profile+'.tsv'),cases=evidence.expected_cases(profile));save(phase+'/'+profile+'-samples.json',records)
   normalize=lambda p,row,rev:next(iter(evidence.indexed_rows(p,[row],rev,counts[p],complete=False)))
   groups=sampling.sample_groups(records,profile,source['commit'],evidence.expected_cases(profile),counts[profile],normalize,context['isolation']['cpu_affinity'],context['runtime']['page_size'])
   phase_layouts[phase][profile]=sampling.fixture_catalog(records)
   for case,layout in phase_layouts[phase][profile].items():
    if layout!=calibration_layouts[profile][case]:report['layout_mismatches'].append({'phase':phase,'profile':profile,'case':case})
   widths=[];short=0
   for case,group in groups.items():
    samples=group[sampling.PROTOCOL['warmups']:];values=sorted(r['rows'][0][evidence.METRICS[profile]] for r in samples);median=statistics.median(values)
    medians[phase][(profile,case)]=median;widths.append({'case':case,'central_spread':(values[-2]-values[1])/median})
    short+=sum(r['wall_ns']<sampling.PROTOCOL['minimum_wall_ns'] or r['timed_ns']<sampling.PROTOCOL['minimum_timed_ns'] for r in samples)
   report['phases'][phase][profile]={'case_count':len(groups),'sample_count':len(groups)*sampling.PROTOCOL['samples'],'short_samples':short,'wide_cases':[r for r in widths if r['central_spread']>sampling.PROTOCOL['central_spread_limit']]}
   unchanged();save('diagnosis.partial.json',report)
 for key,value in medians['A1'].items():report['repeat_checks'].append({'profile':key[0],'case':key[1],'drift':abs(medians['A2'][key]/value-1)})
 report['repeat_failures']=[r for r in report['repeat_checks'] if r['drift']>.05];report['repeat_completed']=True
 report['baseline_controls_passed']=not report['layout_mismatches'] and not report['repeat_failures'] and not any(p['short_samples'] or p['wide_cases'] for phase in report['phases'].values() for p in phase.values());save('diagnosis.json',report)
finally:
 if owned:
  directory=args.baseline/'tests/rse_release_sampling'
  assert {p.name for p in directory.iterdir()}==set(sampling.render(args.baseline))-{'root.rs'}
  for path in directory.iterdir():path.unlink()
  directory.rmdir();(args.baseline/'tests/rse_release_sampling.rs').unlink()
