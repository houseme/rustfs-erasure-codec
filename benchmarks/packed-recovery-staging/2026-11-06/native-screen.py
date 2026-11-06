import hashlib,json,os,pathlib,signal,subprocess,time
root=pathlib.Path(os.environ['RSE_BENCH_ROOT'])
out=root/'native-screen-v5-20261106-01';out.mkdir(exist_ok=False)
ticks=os.sysconf('SC_CLK_TCK')
cases={
 'recovery':('binaries-v5-native/packed_recovery',r'small_leopard_recovery/slots/(gf8|gf16)/(10x4/64/missing4|16x8/128/missing4|10x4/1048576/missing4)$|small_leopard_recovery/option/(gf8|gf16)/10x4/64/missing4$'),
}
paths={root/path/role for path,_ in cases.values() for role in ('baseline','candidate')}
receipts={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
(out/'build-receipts.json').write_text(json.dumps(receipts,indent=2))
def snapshot(exclude):
 data={}
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdigit() or int(p.name) in exclude:continue
  try:
   f=(p/'stat').read_text().rsplit(') ',1)[1].split();data[int(p.name)]=(f[19],int(f[11])+int(f[12]))
  except (FileNotFoundError,ProcessLookupError,PermissionError):pass
 return data
def interrupt(sig,frame):raise InterruptedError('signal '+str(sig))
for sig in (signal.SIGHUP,signal.SIGINT,signal.SIGTERM):signal.signal(sig,interrupt)
child=None
try:
 for case,(binary_dir,pattern) in cases.items():
  for backend in ('rust-gfni-avx2','rust-avx2'):
   for i,stage in enumerate(('A1','B1','B2','A2')):
    modes=('same','candidate') if i%2==0 else ('candidate','same')
    for mode in modes:
     role='baseline' if stage.startswith('A') or mode=='same' else 'candidate'
     dest=out/case/backend/mode/stage;dest.mkdir(parents=True,exist_ok=False)
     env=os.environ.copy();env.update(RSE_BACKEND_OVERRIDE=backend,RSE_EXPECT_BACKEND=backend,RAYON_NUM_THREADS='1',CRITERION_HOME=str(dest/'criterion'))
     args=[str(root/binary_dir/role),'--bench','--noplot','--sample-size','40','--warm-up-time','0.5','--measurement-time','2.0',pattern]
     def pin():os.setsid();os.sched_setaffinity(0,{8})
     with (dest/'stdout.log').open('w') as stdout,(dest/'stderr.log').open('w') as stderr,(dest/'monitor.jsonl').open('w') as monitor:
      child=subprocess.Popen(args,cwd=root,env=env,stdout=stdout,stderr=stderr,preexec_fn=pin)
      old=snapshot({os.getpid(),child.pid});previous=time.monotonic()
      while child.poll() is None:
       time.sleep(1);now=time.monotonic();current=snapshot({os.getpid(),child.pid})
       busy=[pid for pid,(start,value) in current.items() if pid in old and old[pid][0]==start and value-old[pid][1]>ticks*(now-previous)*0.5]
       monitor.write(json.dumps({'elapsed':round(now-previous,3),'competing_count':len(busy)})+'\n');monitor.flush()
       if busy:raise RuntimeError('competing CPU-intensive process detected')
       old,previous=current,now
      if child.returncode:raise RuntimeError('benchmark failed: '+str(child.returncode))
      child=None
     print('DONE',case,backend,mode,stage,flush=True)
 for p,digest in receipts.items():assert hashlib.sha256((root/p).read_bytes()).hexdigest()==digest
 (out/'COMPLETE').write_text('all stages completed and binary receipts verified\n');print('COMPLETE',flush=True)
finally:
 if child is not None and child.poll() is None:
  os.killpg(child.pid,signal.SIGTERM)
  try:child.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
