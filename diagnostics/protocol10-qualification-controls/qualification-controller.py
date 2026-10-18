"""Two predeclared same-version baseline qualifications, never candidate attribution."""
import argparse,hashlib,json,os,pathlib,re,runpy,subprocess,sys
p=argparse.ArgumentParser()
for n in ('tool','baseline','output','group'):p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();root=a.output.parent;q=root/'qualification';q.mkdir()
sys.path.insert(0,str(a.tool/'scripts'))
import release_evidence as e
import release_sampling as s
import diagnose_fixture_layout as inv
assert not any(e.allocator_override_key(k) for k in os.environ), 'unexpected inherited allocator override'
policies=(('default',None),('retained_arena','glibc.malloc.arena_max=1:glibc.malloc.mmap_max=0:glibc.malloc.trim_threshold=1099511627776'))
manifest={'schema_version':'protocol10-baseline-qualification-plan-v1','source':e.source_info(a.tool),'baseline':e.source_info(a.baseline),'policies':[{'name':n,'GLIBC_TUNABLES':v} for n,v in policies],'protocol':s.PROTOCOL,'controller_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'purpose':'independent same-version qualification per declared allocator context; no cross-policy speedup comparison','release_approval':False,'candidate_attribution':None,'results':{}}
def save(): (root/'qualification-plan.json').write_text(json.dumps(manifest,indent=2)+'\n')
save()
# Run real source tests once; the counter never enters a measurement executable.
checks={}
for name in ('check_sampler_telemetry.py','release_sampling.py','diagnose_shard_slot.py'):
 path=q/(name+'.log')
 with path.open('x') as log:
  command=['python3',str(a.tool/'scripts'/name)]+([] if name=='check_sampler_telemetry.py' else ['--check'])
  subprocess.run(command,cwd=a.tool,env=dict(os.environ,RUSTUP_TOOLCHAIN='stable'),stdout=log,stderr=subprocess.STDOUT,check=True)
 counts=re.findall(r'test result: ok\. (\d+) passed',path.read_text());assert counts
 checks[name]={'passed':int(counts[-1]),'scope':'per executable; shared tests are not summed'}
 if name=='check_sampler_telemetry.py':
  rows=s.parse_output(path.read_text(),'RSE_METADATA_ALLOCATION ');assert len(rows)==12
  assert all(r['original_alloc']>0 and r['new_alloc']==r['new_realloc']==r['new_dealloc']==0 for r in rows)
  (q/'pattern-allocations.json').write_text(json.dumps(rows,indent=2)+'\n')
(q/'qualification.json').write_text(json.dumps({'source':manifest['source'],'checks':checks,'release_approval':False,'candidate_attribution':None},indent=2)+'\n')
driver=root/'full-baseline-controller.py'
assert driver.exists()
for name,tunables in policies:
 if tunables is None:os.environ.pop('GLIBC_TUNABLES',None)
 else:os.environ['GLIBC_TUNABLES']=tunables
 inventory=root/(name+'-inventory')
 report=inv.capture(a.tool,a.baseline,a.tool,inventory,a.group)
 manifest['results'][name]={'inventory_mismatches':len(report['layout_mismatches']),'full_baseline_started':False};save()
 if report['layout_mismatches']:
  manifest['results'][name]['stop_reason']='caller structure mismatch; no performance qualification';save();continue
 manifest['results'][name]['full_baseline_started']=True;save()
 output=root/(name+'-baseline')
 sys.argv=[str(driver),'--tool',str(a.tool),'--baseline',str(a.baseline),'--output',str(output),'--group',str(a.group)]
 try:runpy.run_path(str(driver),run_name='__main__')
 except SystemExit as issue:
  if issue.code not in (None,0):raise
 report=json.loads((output/'diagnosis.json').read_text())
 manifest['results'][name].update(baseline_controls_passed=report['baseline_controls_passed'],repeat_completed=report['repeat_completed'],release_approval=False,candidate_attribution=None);save()
os.environ.pop('GLIBC_TUNABLES',None)
