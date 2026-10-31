import json,os,statistics,subprocess,sys,threading,time
from pathlib import Path
stop=threading.Event();bad=threading.Event()
def monitor():
 with open("order-worker-aligned-monitor.jsonl","w") as out:
  while not stop.is_set():
   cpus=Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip();active=subprocess.run(["pgrep","-x","leopard_recover|baseline|candidate"],stdout=subprocess.DEVNULL).returncode==0
   stats=Path("/proc/stat").read_text().splitlines();cpu=[x for x in stats if x.startswith(("cpu8 ","cpu9 ","cpu10 ","cpu11 ","cpu12 "))];psi=Path("/proc/pressure/cpu").read_text().splitlines()[0];out.write(json.dumps({"ns":time.monotonic_ns(),"cpus":cpus,"active":active,"stats":cpu,"psi":psi})+"\n");out.flush()
   if cpus!="0-15" or active:bad.set()
   stop.wait(.2)
t=threading.Thread(target=monitor);t.start()
try:
 env=os.environ.copy();env.pop("RSE_BACKEND_OVERRIDE",None);env["RAYON_NUM_THREADS"]="4"
 for run in (1,2):
  if bad.is_set():sys.exit("reservation changed")
  output=subprocess.check_output(["taskset","-c","8-12","./order-worker-aligned-target/release/rse-paired-row-worker-aa"],env=env,text=True);Path(f"order-worker-aligned-aa-{run}.jsonl").write_text(output)
  if bad.is_set():sys.exit("reservation changed")
  rows=list(map(json.loads,output.splitlines()));assert len(rows)==4;failed=False
  for d in rows:
   m={k:statistics.median(d[k]["samples_ns"]) for k in ("A1","B1","B2","A2")};a=(m["A2"]/m["A1"]-1)*100;b=(m["B2"]/m["B1"]-1)*100;bias=((m["B1"]+m["B2"])/(m["A1"]+m["A2"])-1)*100;failed |= abs(a)>5 or abs(b)>5 or abs(bias)>5;print("run",run,d["case_id"],"A=%.2f%% B=%.2f%% same=%.2f%%"%(a,b,bias),flush=True)
  if failed:sys.exit("baseline drift or same-code bias FAILED; stopping")
 print("same-process baseline preflights PASS",flush=True)
finally: stop.set();t.join()
