import json,os,subprocess,threading,time
from pathlib import Path
binary=next(p for p in Path("recovery-base/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="100",RSE_RECOVERY_CASE_FILTER="classic.reconstruct_data",RAYON_NUM_THREADS="1")
done=threading.Event()
def monitor():
 with open("recovery-aa3-monitor.jsonl","w") as out:
  while not done.is_set():
   lines=Path("/proc/stat").read_text().splitlines(); stat=next(x for x in lines if x.startswith("cpu8 ")); psi=Path("/proc/pressure/cpu").read_text().splitlines()[0]; cpus=Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip(); out.write(json.dumps({"ns":time.monotonic_ns(),"cpu8":stat,"psi":psi,"cpuset":cpus})+"\n");out.flush();done.wait(.2)
t=threading.Thread(target=monitor);t.start()
try:
 for stage in ("A1","B1","B2","A2"):
  with open("recovery-aa3-"+stage+".jsonl","w") as out, open("recovery-aa3-timing.jsonl","a") as times:
   times.write(json.dumps({"stage":stage,"event":"start","ns":time.monotonic_ns()})+"\n");times.flush()
   p=subprocess.Popen(["taskset","-c","8",str(binary)],stdout=subprocess.PIPE,text=True,env=env,bufsize=1)
   for line in p.stdout:
    out.write(line);out.flush();d=json.loads(line);times.write(json.dumps({"stage":stage,"case":d["case_id"],"ns":time.monotonic_ns()})+"\n");times.flush()
   if p.wait():raise RuntimeError("benchmark failed")
finally:
 done.set();t.join()
r={k:{d["case_id"]:d for d in map(json.loads,open("recovery-aa3-"+k+".jsonl"))} for k in ("A1","B1","B2","A2")}
assert all(set(r[k])==set(r["A1"]) for k in r);fail=False
for case in r["A1"]:
 m={k:r[k][case]["median_ns_per_iter"] for k in r};a=(m["A2"]/m["A1"]-1)*100;b=(m["B2"]/m["B1"]-1)*100;fail |= abs(a)>5 or abs(b)>5;print(case,"A=%.2f%% B=%.2f%%"%(a,b))
print("diagnostic_drift_gate:","FAIL" if fail else "PASS")
