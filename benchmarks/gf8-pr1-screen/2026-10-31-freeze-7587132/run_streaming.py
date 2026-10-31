"""Stream one case at a time and stop our probe on a failed gate.

This is a diagnostic harness for the reserved 16-vCPU VM, not product code.
Run baseline-only, or supply a candidate binary to run it immediately after
both baseline preflights. Probe binaries must flush one JSON object per case.
"""
import argparse
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
import threading
import time

STAGES = ("A1", "B1", "B2", "A2")
OFF_TARGET_CPUS = tuple(range(1, 8)) + (13, 14, 15)
EXPECTED_CASES = {
    "retained_recovery_row/10x4/1048576/missing0",
    "retained_recovery_row/10x4/1048577/missing3",
    "retained_recovery_row/4x2/1048576/missing0",
}


class GateError(Exception):
    pass


def off_target_busy(previous, current, streak):
    """Track sustained aggregate load outside the monitor and benchmark CPUs."""
    busy = 0
    total = 0
    for cpu in OFF_TARGET_CPUS:
        name = f"cpu{cpu}"
        if name not in previous or name not in current:
            raise GateError("missing off-target CPU counters")
        before, after = previous[name], current[name]
        if len(before) < 8 or len(after) < 8:
            raise GateError("invalid CPU counters")
        delta = [b - a for a, b in zip(before[:8], after[:8])]
        if any(value < 0 for value in delta):
            raise GateError("CPU counters moved backwards")
        ticks = sum(delta)
        total += ticks
        busy += ticks - delta[3] - delta[4]
    if total <= 0:
        raise GateError("empty CPU counter interval")
    # Across ten off-target CPUs, 5% aggregate equals half a busy CPU.
    loaded = busy * 20 >= total
    return busy / total, streak + 1 if loaded else 0


def evaluate(row, *, baseline):
    """Reject malformed samples before applying the prespecified 5% gates."""
    if row.get("case_id") not in EXPECTED_CASES:
        raise GateError("unexpected case")
    if type(row.get("jobs")) is not int or row["jobs"] != 4:
        raise GateError("unexpected parallel policy")
    if type(row.get("available_parallelism")) is not int or row["available_parallelism"] != 5:
        raise GateError("unexpected parallel policy")
    if type(row.get("chunk_len")) is not int or row["chunk_len"] <= 0:
        raise GateError("invalid chunk length")
    medians = {}
    for stage in STAGES:
        block = row.get(stage, {})
        if not isinstance(block, dict):
            raise GateError("invalid stage")
        samples = block.get("samples_ns", [])
        iterations = block.get("iterations", [])
        if not isinstance(samples, list) or not isinstance(iterations, list) or len(samples) != 3 or len(iterations) != 3:
            raise GateError("missing samples or iteration counts")
        try:
            invalid_sample = any(not isinstance(v, (int, float)) or isinstance(v, bool) or not math.isfinite(v) or v <= 0 for v in samples)
        except OverflowError:
            invalid_sample = True
        if invalid_sample:
            raise GateError("nonfinite or nonpositive sample")
        if any(not isinstance(v, int) or isinstance(v, bool) or v <= 0 for v in iterations):
            raise GateError("invalid iteration count")
        medians[stage] = statistics.median(samples)
    a = (medians["A2"] / medians["A1"] - 1) * 100
    b = (medians["B2"] / medians["B1"] - 1) * 100
    mean_a = medians["A1"] / 2 + medians["A2"] / 2
    mean_b = medians["B1"] / 2 + medians["B2"] / 2
    delta = (mean_b / mean_a - 1) * 100
    if not all(math.isfinite(v) for v in (a, b, delta)):
        raise GateError("nonfinite calculated result")
    if not (medians["A1"] * 0.95 <= medians["A2"] <= medians["A1"] * 1.05) or not (medians["B1"] * 0.95 <= medians["B2"] <= medians["B1"] * 1.05):
        raise GateError(f"phase drift exceeds 5%: A={a:.2f}% B={b:.2f}%")
    if baseline and not (mean_a * 0.95 <= mean_b <= mean_a * 1.05):
        raise GateError(f"same-source bias exceeds 5%: {delta:.2f}%")
    return a, b, delta


def stop_process(proc):
    """Stop only the probe process spawned by this harness."""
    if proc is None or proc.poll() is not None:
        return
    try:
        proc.terminate()
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            proc.kill()
        except ProcessLookupError:
            pass
        proc.wait()


def validated_rows(lines, *, baseline, resource_invalid):
    """Validate each complete case before allowing the next one to be read."""
    seen = set()
    for line in lines:
        if resource_invalid():
            raise GateError("resource monitor rejected the run")
        row = json.loads(line)
        if not isinstance(row, dict):
            raise GateError("invalid case object")
        case = row.get("case_id")
        if case in seen:
            raise GateError("duplicate case")
        a, b, delta = evaluate(row, baseline=baseline)
        seen.add(case)
        yield row, a, b, delta
    if seen != EXPECTED_CASES:
        raise GateError("missing case")


def process_cases(proc, out, *, baseline, resource_invalid, accept):
    """Acknowledge each case only after its gates pass."""
    def recorded_lines():
        for line in proc.stdout:
            out.write(line)
            out.flush()
            yield line

    for row, a, b, delta in validated_rows(
        recorded_lines(), baseline=baseline, resource_invalid=resource_invalid
    ):
        accept(row, a, b, delta)
        proc.stdin.write("continue\n")
        proc.stdin.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-binary", required=True)
    parser.add_argument("--candidate-binary")
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--source-label", required=True)
    args = parser.parse_args()
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=False)
    stop = threading.Event()
    bad = threading.Event()
    ready = threading.Event()
    lock = threading.Lock()
    current = [None]
    errors = []

    def monitor():
        initial_steal = None
        initial_throttled = None
        previous_cpu = None
        busy_streak = 0
        idle_streak = 0
        try:
            with (output_dir / "monitor.jsonl").open("w") as out:
                while not stop.is_set():
                    cpus = Path("/sys/fs/cgroup/cpuset.cpus.effective").read_text().strip()
                    proc = subprocess.run(["pgrep", "-x", "leopard_recover|baseline|candidate"], stdout=subprocess.DEVNULL)
                    if proc.returncode not in (0, 1):
                        raise GateError("could not inspect competing processes")
                    active = proc.returncode == 0
                    lines = Path("/proc/stat").read_text().splitlines()
                    cpu_all = [line for line in lines if line.split()[0].startswith("cpu") and line.split()[0][3:].isdigit()]
                    if len(cpu_all) != 16:
                        raise GateError("unexpected online CPU counters")
                    current_cpu = {line.split()[0]: [int(value) for value in line.split()[1:9]] for line in cpu_all}
                    ratio = None
                    if previous_cpu is not None:
                        ratio, busy_streak = off_target_busy(previous_cpu, current_cpu, busy_streak)
                        idle_streak = idle_streak + 1 if busy_streak == 0 else 0
                    previous_cpu = current_cpu
                    target = [line for line in cpu_all if line.split()[0] in {"cpu8", "cpu9", "cpu10", "cpu11", "cpu12"}]
                    if len(target) != 5:
                        raise GateError("missing target CPU counters")
                    steal = [int(line.split()[8]) for line in target]
                    cpu_stat = Path("/sys/fs/cgroup/cpu.stat").read_text()
                    stat = dict(line.split() for line in cpu_stat.splitlines())
                    throttled = int(stat.get("nr_throttled", "0"))
                    if initial_steal is None:
                        initial_steal = steal
                        initial_throttled = throttled
                    psi = Path("/proc/pressure/cpu").read_text().splitlines()[0]
                    entry = {"monotonic_ns": time.monotonic_ns(), "cpus": cpus, "active": active, "cpu_all": cpu_all, "off_target_busy_ratio": ratio, "off_target_busy_streak": busy_streak, "pressure": psi, "cpu_stat": stat}
                    out.write(json.dumps(entry) + "\n")
                    out.flush()
                    if cpus != "0-15" or active or steal != initial_steal or throttled != initial_throttled:
                        raise GateError("resource reservation, steal or throttle counter changed")
                    if busy_streak >= 3:
                        raise GateError("sustained off-target CPU load")
                    if idle_streak >= 3:
                        ready.set()
                    stop.wait(0.2)
        except Exception as exc:  # fail closed and stop only our own probe
            errors.append(str(exc))
            bad.set()
            ready.set()
            with lock:
                stop_process(current[0])

    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("RSE_", "RS_")):
            env.pop(key)
    env.update(RAYON_NUM_THREADS="4", RSE_DIAG_EXPECT_BACKEND="rust-avx2")
    thread = threading.Thread(target=monitor)
    thread.start()
    candidate_results = []
    phase = "baseline"
    try:
        if not ready.wait(timeout=5) or bad.is_set():
            raise GateError("resource monitor did not establish a valid initial state")
        for phase, binary, baseline in [
            ("baseline", args.baseline_binary, True),
            ("candidate", args.candidate_binary, False),
        ]:
            if binary is None:
                break
            for run in (1, 2):
                filename = output_dir / f"{phase}-{run}.jsonl"
                with filename.open("w") as out:
                    with lock:
                        if bad.is_set():
                            raise GateError("resource monitor rejected the run")
                        proc = subprocess.Popen(["taskset", "-c", "8-12", binary], env=env, text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=1)
                        current[0] = proc
                    try:
                        def accept(row, a, b, delta):
                            case = row["case_id"]
                            print(phase, run, case, f"A={a:.2f}% B={b:.2f}%", flush=True)
                            if not baseline:
                                candidate_results.append((run, case, delta))

                        process_cases(
                            proc, out, baseline=baseline, resource_invalid=bad.is_set, accept=accept
                        )
                        code = proc.wait()
                        if bad.is_set():
                            raise GateError("resource monitor rejected the run")
                        if code != 0:
                            raise GateError(f"probe exited with status {code}")
                    finally:
                        stop_process(proc)
                        with lock:
                            current[0] = None
        # Join the monitor before declaring success, so a final resource change
        # cannot race with the acceptance message.
        stop.set()
        thread.join()
        if bad.is_set():
            raise GateError("resource monitor rejected the run")
        summary = {"status": "pass", "source": args.source_label, "candidate_tested": args.candidate_binary is not None}
        (output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print("all requested gates passed", flush=True)
        for run, case, delta in candidate_results:
            print("candidate", run, case, f"latency_delta={delta:.2f}%", flush=True)
    except (GateError, OSError, ValueError, KeyError, TypeError) as exc:
        summary = {"status": "failed", "source": args.source_label, "phase": phase, "reason": str(exc), "monitor_errors": errors}
        (output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(f"gate failed: {exc}", file=sys.stderr, flush=True)
        return 1
    finally:
        stop.set()
        thread.join()
    return 0


if __name__ == "__main__":
    sys.exit(main())
