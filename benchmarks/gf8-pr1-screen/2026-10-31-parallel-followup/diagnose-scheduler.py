import json,os,subprocess
from pathlib import Path
base=next(p for p in Path("base-target/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="150",RSE_RECOVERY_CASE_FILTER="encode.serial/6x2/8192/",RAYON_NUM_THREADS="1")
with open("shape-8192-scheduler.jsonl","w") as out:
 for i in range(16):
  if Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()!="0-15" or subprocess.run(["pgrep","-x","leopard_recover"],stdout=subprocess.DEVNULL).returncode==0:raise RuntimeError("resources changed")
  result=subprocess.run(["perf","stat","-x,","-e","task-clock,context-switches,cpu-migrations,page-faults","--","taskset","-c","8",str(base)],env=env,text=True,capture_output=True,check=True)
  data=json.loads(result.stdout);counts={}
  for line in result.stderr.splitlines():
   parts=line.split(",")
   if len(parts)>=3 and parts[2] in ("task-clock","context-switches","cpu-migrations","page-faults"):counts[parts[2]]=parts[0]
  data["diagnostic_perf"]=counts;out.write(json.dumps(data)+"\n");out.flush();print(i,round(data["median_ns_per_iter"],2),counts,flush=True)
