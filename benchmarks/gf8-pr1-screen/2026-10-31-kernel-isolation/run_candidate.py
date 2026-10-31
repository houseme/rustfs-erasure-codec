"""Run baseline-only retained-row diagnostics, stopping on drift or resource changes."""
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import threading
import time

binary = sys.argv[1]
output_dir = Path(sys.argv[2])
output_dir.mkdir(parents=True, exist_ok=True)
stop = threading.Event()
bad = threading.Event()


def monitor():
    with (output_dir / "monitor.jsonl").open("w") as out:
        while not stop.is_set():
            cpus = Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()
            active = subprocess.run(
                ["pgrep", "-x", "leopard_recover|baseline|candidate"],
                stdout=subprocess.DEVNULL,
            ).returncode == 0
            lines = Path("/proc/stat").read_text().splitlines()
            cpu = [line for line in lines if line.startswith(("cpu8 ", "cpu9 ", "cpu10 ", "cpu11 ", "cpu12 "))]
            psi = Path("/proc/pressure/cpu").read_text().splitlines()[0]
            out.write(json.dumps({"monotonic_ns": time.monotonic_ns(), "cpus": cpus, "active": active, "cpu": cpu, "pressure": psi}) + "\n")
            out.flush()
            if cpus != "0-15" or active:
                bad.set()
            stop.wait(0.2)


thread = threading.Thread(target=monitor)
thread.start()
try:
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("RSE_", "RS_")):
            env.pop(key)
    env.update(RAYON_NUM_THREADS="4", RSE_DIAG_EXPECT_BACKEND="rust-avx2")
    results = []
    for run in (1, 2):
        if bad.is_set():
            sys.exit("resource reservation changed")
        output = subprocess.check_output(["taskset", "-c", "8-12", binary], env=env, text=True)
        (output_dir / f"candidate-{run}.jsonl").write_text(output)
        if bad.is_set():
            sys.exit("resource reservation changed")
        rows = list(map(json.loads, output.splitlines()))
        assert len(rows) == 3
        failed = False
        for row in rows:
            values = {stage: statistics.median(row[stage]["samples_ns"]) for stage in ("A1", "B1", "B2", "A2")}
            drift_a = (values["A2"] / values["A1"] - 1) * 100
            drift_b = (values["B2"] / values["B1"] - 1) * 100
            bias = ((values["B1"] + values["B2"]) / (values["A1"] + values["A2"]) - 1) * 100
            failed |= max(abs(drift_a), abs(drift_b)) > 5
            results.append((run, row["case_id"], bias))
            print(run, row["case_id"], f"A={drift_a:.2f}% B={drift_b:.2f}%", flush=True)
        if failed:
            sys.exit("candidate phase drift FAILED; stopping")
    print("retained-row candidate drift gates PASS", flush=True)
    for run, case, delta in results:
        print(run, case, f"latency_delta={delta:.2f}%", flush=True)
finally:
    stop.set()
    thread.join()
