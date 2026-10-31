import json,os,subprocess,sys
from pathlib import Path
base=next(p for p in Path("recovery-base/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK));pr=next(p for p in Path("recovery-narrow/release/deps").glob("recovery_paths-*") if p.is_file() and os.access(p,os.X_OK))
cases=["classic.reconstruct_data_slots/4x2/16384/missing1+0", "classic.reconstruct_data_slots/4x2/16384/missing2+0", "classic.reconstruct_data_opt/10x4/1048576/missing1+0", "classic.reconstruct_data_opt/10x4/1048576/missing2+0", "classic.reconstruct_some_parity/32x16/65536/missing1+1"]
for i,case in enumerate(cases):
 env=os.environ.copy();env.update(RSE_RECOVERY_SAMPLE_MS="150",RSE_RECOVERY_CASE_FILTER=case,RAYON_NUM_THREADS="1")
 for mode in ("aa","abba"):
  vals={}
  for stage in ("A1","B1","B2","A2"):
   cpus=Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()
   if cpus!="0-15" or subprocess.run(["pgrep","-x","leopard_recover"],stdout=subprocess.DEVNULL).returncode==0:sys.exit("resource reservation changed")
   binary=base if mode=="aa" or stage.startswith("A") else pr
   output=subprocess.check_output(["taskset","-c","8",str(binary)],env=env,text=True); Path(f"recovery-narrow-sweep-{i}-{mode}-{stage}.jsonl").write_text(output)
   rows=list(map(json.loads,output.splitlines()));assert len(rows)==1;vals[stage]=rows[0]["median_ns_per_iter"]
  a=(vals["A2"]/vals["A1"]-1)*100;b=(vals["B2"]/vals["B1"]-1)*100
  print(case,mode,"A=%.2f%% B=%.2f%%"%(a,b),flush=True)
  if abs(a)>5 or abs(b)>5:sys.exit("drift gate FAILED; stopping sweep")
  if mode=="abba":print(case,"latency_delta=%.2f%%"%(((vals["B1"]+vals["B2"])/(vals["A1"]+vals["A2"])-1)*100),flush=True)
