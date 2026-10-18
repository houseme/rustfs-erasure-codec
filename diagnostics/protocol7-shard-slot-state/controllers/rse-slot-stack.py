"""Private-launchable, address-free syscall diagnosis; never release evidence."""
import argparse,hashlib,json,os,pathlib,subprocess,sys
p=argparse.ArgumentParser()
for n in ('tool','repository','output','isolation-cgroup'):p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();sys.path.insert(0,str(a.tool/'scripts'))
import release_evidence as e
import release_sampling as s
import diagnose_shard_slot as d
a.output.mkdir(mode=0o700)
env={k:v for k,v in os.environ.items() if not k.startswith(('RSE_','RS_','CARGO_PROFILE_')) and k not in ('CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER')};env.update(e.SAMPLING_ENV)
source=e.source_info(a.repository);assert source['commit']==e.BASELINE_COMMIT
context=e.environment_context(a.repository,env,a.isolation_cgroup);cpu=min(context['isolation']['cpu_affinity']);page=context['runtime']['page_size']
meta={'schema_version':'slot-stack-diagnostic-v1','purpose':'first 64 madvise calls per worker with at most eight stack frames; no performance attribution','source':source,'tool':e.source_info(a.tool),'context':context,'protocol':s.PROTOCOL,'renderer_sha256':s.implementation_digest(),'controller_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'strace_version':subprocess.check_output(['strace','--version'],text=True).splitlines()[0],'release_approval':False,'candidate_attribution':None}
def save(n,v):(a.output/n).write_text(json.dumps(v,indent=2)+'\n')
save('provenance.json',meta)
workers=[]
with d.overlay(a.repository,False):
 command=['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','--no-run','--message-format=json']
 with (a.output/'build.log').open('x') as out,(a.output/'build-errors.log').open('x') as err:
  subprocess.run(command,cwd=a.repository,env=env,stdout=out,stderr=err,check=True)
 artifacts=[json.loads(line) for line in (a.output/'build.log').read_text().splitlines() if line.startswith('{')]
 binaries=[r['executable'] for r in artifacts if r.get('reason')=='compiler-artifact' and r.get('target',{}).get('name')=='rse_release_sampling' and r.get('executable')]
 assert len(binaries)==1
 for worker,mode in enumerate(d.SCHEDULE):
  name=f'worker-{worker}-{mode}';log=a.output/(name+'.log');summary=a.output/(name+'-syscalls.txt')
  worker_env=dict(env,RSE_SAMPLER_CASE=d.CASE,RSE_SAMPLER_PROFILE='small_file',RSE_SAMPLER_MODE=mode,RSE_SAMPLER_ITERATIONS='4096',RSE_SAMPLER_BATCH='1' if mode=='calibrate' else '0',RSE_SAMPLER_PAGE_SIZE=str(page))
  try:
   with log.open('x') as out:
    process=subprocess.Popen(['taskset','--cpu-list',str(cpu),'strace','-f','-qq','--syscall-limit=64','-k','--stack-trace-frame-limit=8','-e','trace=madvise','-o',str(summary),binaries[0],'release_controlled_profile','--exact','--ignored','--nocapture','--test-threads=1'],cwd=a.repository,env=worker_env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    for line in process.stdout:
     out.write(line);out.flush()
    assert process.wait()==0
  finally:
   if log.exists():
    text=log.read_text();records=s.parse_output(text);save(name+'-samples.partial.json',records);save(name+'-timing.partial.json',s.parse_output(text,'RSE_TIMING '))
  assert len(records)==12
  normalize=lambda p,row,rev:next(iter(e.indexed_rows(p,[row],rev,{d.CASE:4096},complete=False)))
  for r in records:
   assert r['revision']==source['commit'] and r['case']==d.CASE and r['iterations']==4096
   s.validate_telemetry(r,{cpu});s.validate_timing(r,'small_file',4096);assert normalize('small_file',r['rows'][0],source['commit'])==d.CASE
  if mode=='calibrate':s.calibration_batch(records,4096,1)
  else:s.sample_groups(records,'small_file',source['commit'],{d.CASE},{d.CASE:4096},normalize,{cpu},page)
  assert e.source_info(a.repository)==source and e.environment_context(a.repository,env,a.isolation_cgroup)==context
  workers.append({'worker':worker,'mode':mode,'summary':summary.name,'observation_count':len(records)})
save('diagnosis.json',dict(meta,workers=workers))
