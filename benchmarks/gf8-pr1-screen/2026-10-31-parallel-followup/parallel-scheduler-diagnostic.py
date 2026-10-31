import json,os,subprocess
from pathlib import Path
base=next(p for p in Path("parallel-base-target/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="300",RSE_RECOVERY_CASE_FILTER="classic.reconstruct_data_opt/10x4/1048576/missing1+0",RAYON_NUM_THREADS="4",RS_PARALLEL_POLICY_MAX_JOBS="4")
with open("parallel-scheduler-diagnostic.jsonl","w") as out:
 for cpus in ("8-11","8-12"):
  for i in range(8):
   if Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()!="0-15" or subprocess.run(["pgrep","-x","leopard_recover|baseline|candidate"],stdout=subprocess.DEVNULL).returncode==0:raise RuntimeError("resources changed")
   result=subprocess.run(["perf","stat","-x,","-e","task-clock,context-switches,cpu-migrations,page-faults","--","taskset","-c",cpus,str(base)],env=env,text=True,capture_output=True,check=True)
   data=json.loads(result.stdout);counts={}
   for line in result.stderr.splitlines():
    parts=line.split(",")
    if len(parts)>=3 and parts[2] in ("task-clock","context-switches","cpu-migrations","page-faults"):counts[parts[2]]=parts[0]
   data["diagnostic_perf"]=counts;data["cpu_binding"]=cpus;out.write(json.dumps(data)+"\n");out.flush();print(cpus,i,round(data["median_ns_per_iter"],2),counts,flush=True)
