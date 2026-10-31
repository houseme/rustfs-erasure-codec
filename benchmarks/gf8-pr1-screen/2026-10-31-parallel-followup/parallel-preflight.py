import json,os,subprocess,sys
from pathlib import Path
base=next(p for p in Path("parallel-base-target/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
cases=["classic.reconstruct_data_opt/10x4/1048576/missing1+0", "classic.reconstruct_data_opt/10x4/1048576/missing2+0", "classic.reconstruct_data_slots/10x4/32768/missing1+0", "classic.reconstruct_data_slots/4x2/16384/missing1+0", "classic.reconstruct_some_parity/32x16/65536/missing1+1"]
for i,case in enumerate(cases):
 env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="150",RSE_RECOVERY_CASE_FILTER=case,RAYON_NUM_THREADS="4")
 for run in (1,2):
  vals={}
  for stage in ("A1","B1","B2","A2"):
   if Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()!="0-15" or subprocess.run(["pgrep","-x","leopard_recover"],stdout=subprocess.DEVNULL).returncode==0:sys.exit("resource reservation changed")
   output=subprocess.check_output(["taskset","-c","8-11",str(base)],env=env,text=True);Path(f"parallel-aa-{i}-{run}-{stage}.jsonl").write_text(output)
   rows=list(map(json.loads,output.splitlines()));assert len(rows)==1 and rows[0]["backend"]=="rust-avx2" and rows[0]["rayon_threads"]==4 and rows[0]["available_parallelism"]==4;vals[stage]=rows[0]["median_ns_per_iter"]
  a=(vals["A2"]/vals["A1"]-1)*100;b=(vals["B2"]/vals["B1"]-1)*100;print(case,"round",run,"A=%.2f%% B=%.2f%%"%(a,b),flush=True)
  if abs(a)>5 or abs(b)>5:sys.exit("baseline drift gate FAILED; stopping")
print("all baseline preflights PASS")
