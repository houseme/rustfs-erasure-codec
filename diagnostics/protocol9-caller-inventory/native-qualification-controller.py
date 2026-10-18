"""Native checks followed by complete caller-layout inventory; never release approval."""
import argparse,hashlib,json,os,pathlib,re,subprocess,sys
p=argparse.ArgumentParser()
for n in ('tool','baseline','output','group'):p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();q=a.output.parent/'qualification';q.mkdir()
env=dict(os.environ,RUSTUP_TOOLCHAIN='stable');checks={}
for name in ('check_sampler_telemetry.py','release_sampling.py','diagnose_shard_slot.py'):
 path=q/(name+'.log')
 with path.open('x') as log:
  command=['python3',str(a.tool/'scripts'/name)]+([] if name=='check_sampler_telemetry.py' else ['--check'])
  subprocess.run(command,cwd=a.tool,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 counts=re.findall(r'test result: ok\. (\d+) passed',path.read_text());assert counts
 checks[name]={'passed':int(counts[-1]),'scope':'per executable; includes shared unit tests; do not sum as unique tests'}
sys.path.insert(0,str(a.tool/'scripts'));import release_evidence as e
(q/'qualification.json').write_text(json.dumps({'source':e.source_info(a.tool),'checks':checks,'controller_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'release_approval':False,'candidate_attribution':None},indent=2)+'\n')
# Replace the coordinator, so no extra ancestor remains in the measurement cgroup.
os.execvpe('python3',['python3',str(a.tool/'scripts/diagnose_fixture_layout.py'),'--tool',str(a.tool),'--baseline',str(a.baseline),'--candidate',str(a.tool),'--output',str(a.output),'--isolation-cgroup',str(a.group)],env)
