import json,os,pathlib,signal,subprocess,time
root=pathlib.Path(os.environ['RSE_BENCH_ROOT']); cpus={int(os.environ.get('RSE_BENCH_CPU', '8'))}; ticks=os.sysconf('SC_CLK_TCK')
pattern=r'small_leopard_recovery/slots/gf8/(10x4|16x8)/(64|65600|1048576)/missing4$|small_leopard_recovery/slots/gf16/10x4/64/missing4$'
def snapshot(exclude):
 out={}
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdigit() or int(p.name) in exclude:continue
  try:
   f=(p/'stat').read_text().rsplit(') ',1)[1].split();out[int(p.name)]=(f[19],int(f[11])+int(f[12]))
  except (FileNotFoundError,ProcessLookupError,PermissionError):pass
 return out
child=None
def interrupt(signum, frame):
 raise InterruptedError('received signal '+str(signum))
for signum in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM): signal.signal(signum, interrupt)
try:
 for mode in ('same','candidate'):
  backend=os.environ['RSE_BENCH_BACKEND']
  for stage,role in [('A1','base'),('B1','base' if mode=='same' else 'next'),('B2','base' if mode=='same' else 'next'),('A2','base')]:
   output=root/('derivative-v4-final-20261106-'+mode+'-'+backend)/stage;output.mkdir(parents=True,exist_ok=False)
   env=os.environ.copy();env.update(RSE_BACKEND_OVERRIDE=backend,RSE_EXPECT_BACKEND=backend,RAYON_NUM_THREADS='1',CRITERION_HOME=str(output/'criterion'))
   def enter():os.setsid();os.sched_setaffinity(0,cpus)
   argv=[str(root/'derivative-binaries-v4'/'recovery'/('baseline' if role=='base' else 'candidate')),'--bench','--noplot','--sample-size','40','--warm-up-time','1.0','--measurement-time','3.0',pattern]
   with (output/'stdout.log').open('w') as stdout,(output/'stderr.log').open('w') as stderr,(output/'resource.jsonl').open('w') as journal:
    child=subprocess.Popen(argv,env=env,cwd=root,stdout=stdout,stderr=stderr,preexec_fn=enter)
    print('START',backend,stage,flush=True);old=snapshot({os.getpid(),child.pid});previous=time.monotonic()
    while child.poll() is None:
     time.sleep(1);now=time.monotonic();current=snapshot({os.getpid(),child.pid});busy=[pid for pid,(start,value) in current.items() if pid in old and old[pid][0]==start and value-old[pid][1]>ticks*(now-previous)*0.5]
     journal.write(json.dumps({'elapsed':round(now-previous,3),'competing_count':len(busy)})+'\n');journal.flush()
     if busy:raise RuntimeError('competing CPU-intensive process detected')
     old,previous=current,now
    if child.returncode:raise RuntimeError('benchmark failed: '+str(child.returncode))
    child=None;print('FINISH',backend,stage,flush=True)
 print('DONE',flush=True)
finally:
 if child is not None and child.poll() is None:
  os.killpg(child.pid,signal.SIGTERM)
  try:child.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
