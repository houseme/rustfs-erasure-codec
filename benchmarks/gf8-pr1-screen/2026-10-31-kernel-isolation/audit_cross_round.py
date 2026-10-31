"""Re-audit saved baseline A and B levels without changing original summaries."""
import json
from pathlib import Path
import statistics

root = Path(__file__).resolve().parent
runs = {
    "single-implementation-88e52de": root / "results",
    "paired-pre-build-88e52de": root / "paired-aa-results",
    "paired-post-build-88e52de": root / "paired-aa-post-build-results",
    "sched-instrumented-88e52de": root / "sched-aa-results",
    "sched-instrumented-7587132": root.parent / "2026-10-31-freeze-7587132" / "sched-diagnostic",
}
report = {}
for name, directory in runs.items():
    rounds = []
    for number in (1, 2):
        rows = [json.loads(line) for line in (directory / f"baseline-{number}.jsonl").read_text().splitlines() if line.strip()]
        if len(rows) != 3:
            raise ValueError(f"{name}: expected three cases in round {number}")
        rounds.append({row["case_id"]: row for row in rows})
    if set(rounds[0]) != set(rounds[1]):
        raise ValueError(f"{name}: case sets differ")
    cases = []
    for case in sorted(rounds[0]):
        groups = {}
        for label, (left, right) in {"A": ("A1", "A2"), "B": ("B1", "B2")}.items():
            levels = []
            for rows in rounds:
                row = rows[case]
                levels.append(statistics.median(row[left]["samples_ns"]) / 2 + statistics.median(row[right]["samples_ns"]) / 2)
            first, second = levels
            groups[label] = {"round1_ns": first, "round2_ns": second,
                             "cross_round_percent": (second / first - 1) * 100,
                             "within_5_percent": first * 0.95 <= second <= first * 1.05}
        cases.append({"case_id": case, "A": groups["A"], "B": groups["B"]})
    report[name] = {"cross_round_gate_passed": all(case["A"]["within_5_percent"] and case["B"]["within_5_percent"] for case in cases), "cases": cases}
print(json.dumps(report, indent=2))
