"""Predeclared independent baseline placement controls, never release evidence."""
import argparse,hashlib,json,os,pathlib,re,subprocess,sys
p=argparse.ArgumentParser()
for n in ('tool','baseline','output','group'):p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();root=a.output
assert not root.exists();root.mkdir()
sys.path.insert(0,str(a.tool/'scripts'))
import release_evidence as e
import diagnose_cpu_placement as cpu
assert not any(e.allocator_override_key(k) for k in os.environ)
policies=(('default',None),('retained_arena','glibc.malloc.arena_max=1:glibc.malloc.mmap_max=0:glibc.malloc.trim_threshold=1099511627776'))
manifest={'schema_version':'baseline-placement-plan-v1','tool':e.source_info(a.tool),'baseline':e.source_info(a.baseline),'policies':[{'name':n,'GLIBC_TUNABLES':v} for n,v in policies],'iterations':384,'case':cpu.CASE,'controller_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'release_approval':False,'candidate_attribution':None,'results':{}}
def save(): (root/'plan.json').write_text(json.dumps(manifest,indent=2)+'\n')
save()
with (root/'native-check.log').open('x') as log:
 subprocess.run(['python3',str(a.tool/'scripts/diagnose_cpu_placement.py'),'--check'],cwd=a.tool,env=dict(os.environ,RUSTUP_TOOLCHAIN='stable'),stdout=log,stderr=subprocess.STDOUT,check=True)
checks=re.findall(r'test result: ok\. (\d+) passed',(root/'native-check.log').read_text());assert checks
(root/'native-check.json').write_text(json.dumps({'source':manifest['tool'],'active_tests':int(checks[-1]),'scope':'one generated executable; includes shared sampler and formatter checks','release_approval':False},indent=2)+'\n')
for name,value in policies:
 if value is None:os.environ.pop('GLIBC_TUNABLES',None)
 else:os.environ['GLIBC_TUNABLES']=value
 report=cpu.capture(a.tool,a.baseline,root/name,a.group,384)
 groups=[g for w in report['workers'] for g in w['groups']]
 manifest['results'][name]={'workers':len(report['workers']),'groups':len(groups),'observations':len(groups)*12,'short_samples':sum(g['short_samples'] for g in groups),'wide_groups':[{'worker':w['name'],**g} for w in report['workers'] for g in w['groups'] if g['central_spread']>.1],'same_worker_repeat_failures':[{'worker':w['name'],**r} for w in report['workers'] for r in w['same_worker_repeats'] if r['absolute_drift']>.05],'fresh_process_repeat_failures':[r for r in report['fresh_process_repeats'] if r['absolute_drift']>.05],'release_approval':False};save()
os.environ.pop('GLIBC_TUNABLES',None)
