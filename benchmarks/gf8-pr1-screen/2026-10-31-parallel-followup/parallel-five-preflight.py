import json,os,subprocess,sys,threading,time
from pathlib import Path
base=next(p for p in Path("parallel-base-target/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
cases=["classic.reconstruct_data_opt/10x4/1048576/missing1+0","classic.reconstruct_data_opt/10x4/1048576/missing2+0","classic.reconstruct_data_slots/4x2/16384/missing1+0"]
stop=threading.Event();bad=threading.Event()
def check():
 cpus=Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip();active=subprocess.run(["pgrep","-x","leopard_recover|baseline|candidate"],stdout=subprocess.DEVNULL).returncode==0
 return cpus,active
def monitor():
 with open("parallel-five-monitor.jsonl","w") as out:
  while not stop.is_set():
   cpus,active=check();stat=Path("/proc/stat").read_text().splitlines();cpu=[x for x in stat if x.startswith(("cpu8 ","cpu9 ","cpu10 ","cpu11 ","cpu12 "))];psi=Path("/proc/pressure/cpu").read_text().splitlines()[0];out.write(json.dumps({"ns":time.monotonic_ns(),"cpus":cpus,"active":active,"stat":cpu,"psi":psi})+"\n");out.flush()
   if cpus!="0-15" or active:bad.set()
   stop.wait(.2)
t=threading.Thread(target=monitor);t.start()
try:
 for i,case in enumerate(cases):
  env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="150",RSE_RECOVERY_CASE_FILTER=case,RAYON_NUM_THREADS="4",RS_PARALLEL_POLICY_MAX_JOBS="4")
  for run in (1,2):
   vals={}
   for stage in ("A1","B1","B2","A2"):
    if bad.is_set():sys.exit("resource reservation changed")
    output=subprocess.check_output(["taskset","-c","8-12",str(base)],env=env,text=True);Path(f"parallel-five-aa-{i}-{run}-{stage}.jsonl").write_text(output)
    if bad.is_set():sys.exit("resource reservation changed")
    rows=list(map(json.loads,output.splitlines()));assert len(rows)==1 and rows[0]["backend"]=="rust-avx2" and rows[0]["rayon_threads"]==4 and rows[0]["available_parallelism"]==5;vals[stage]=rows[0]["median_ns_per_iter"]
   a=(vals["A2"]/vals["A1"]-1)*100;b=(vals["B2"]/vals["B1"]-1)*100;print(case,"round",run,"A=%.2f%% B=%.2f%%"%(a,b),flush=True)
   if abs(a)>5 or abs(b)>5:sys.exit("baseline drift gate FAILED; stopping")
 print("all baseline preflights PASS",flush=True)
finally:
 stop.set();t.join()
