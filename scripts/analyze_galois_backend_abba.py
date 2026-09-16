#!/usr/bin/env python3
"""Analyze galois_backend A/B/B/A estimates and enforce baseline drift."""

from __future__ import annotations

import argparse
import csv
import json
import pathlib
from collections import defaultdict
from statistics import mean
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--csv", required=True)
    parser.add_argument("--summary-json", required=True)
    parser.add_argument("--summary-md", required=True)
    parser.add_argument("--drift-threshold", type=float, default=0.05)
    return parser.parse_args()


def load_rows(path: pathlib.Path) -> list[dict[str, Any]]:
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    for row in rows:
        row["length_bytes"] = int(row["length_bytes"])
        row["mean_ns"] = float(row["mean_ns"])
    return rows


def pct(value: float) -> float:
    return value * 100.0


def main() -> None:
    args = parse_args()
    rows = load_rows(pathlib.Path(args.csv))
    grouped: dict[tuple[str, int], dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    commits: dict[str, str] = {}
    backend_groups: set[str] = set()
    for row in rows:
        key = (row["operation"], row["length_bytes"])
        grouped[key][row["stage"]].append(row["mean_ns"])
        commits[row["stage"]] = row["commit"]
        backend_groups.add(row["criterion_group"])

    case_summaries: list[dict[str, Any]] = []
    drift_failures = 0
    for (operation, length), stage_values in sorted(grouped.items()):
        required = ["A1", "B1", "B2", "A2"]
        if not all(stage_values.get(stage) for stage in required):
            continue
        a1 = mean(stage_values["A1"])
        a2 = mean(stage_values["A2"])
        b = mean(stage_values["B1"] + stage_values["B2"])
        a = mean([a1, a2])
        baseline_drift = (a2 - a1) / a1
        candidate_delta_vs_a_mean = (b - a) / a
        drift_failed = abs(baseline_drift) > args.drift_threshold
        drift_failures += int(drift_failed)
        case_summaries.append(
            {
                "operation": operation,
                "length_bytes": length,
                "a1_mean_ns": a1,
                "a2_mean_ns": a2,
                "b_mean_ns": b,
                "baseline_drift": baseline_drift,
                "candidate_delta_vs_a_mean": candidate_delta_vs_a_mean,
                "drift_failed": drift_failed,
            }
        )

    payload = {
        "drift_threshold": args.drift_threshold,
        "drift_failed": drift_failures > 0,
        "drift_failures": drift_failures,
        "commits": commits,
        "criterion_groups": sorted(backend_groups),
        "cases": case_summaries,
    }

    summary_json = pathlib.Path(args.summary_json)
    summary_md = pathlib.Path(args.summary_md)
    summary_json.write_text(json.dumps(payload, indent=2, sort_keys=True))

    lines = [
        "# Galois Backend ABBA Summary",
        "",
        f"- Drift threshold: {pct(args.drift_threshold):.2f}%",
        f"- Drift gate: {'FAIL' if payload['drift_failed'] else 'PASS'}",
        f"- Commits: A={commits.get('A1', 'unknown')} / B={commits.get('B1', 'unknown')}",
        "",
        "| Operation | Length | A1 mean ns | A2 mean ns | Baseline drift | B mean ns | B vs A mean | Gate |",
        "|---|---:|---:|---:|---:|---:|---:|---|",
    ]
    for case in case_summaries:
        lines.append(
            "| {operation} | {length_bytes} | {a1_mean_ns:.2f} | {a2_mean_ns:.2f} | "
            "{baseline_drift:.2%} | {b_mean_ns:.2f} | {candidate_delta_vs_a_mean:.2%} | {gate} |".format(
                **case,
                gate="FAIL" if case["drift_failed"] else "PASS",
            )
        )
    summary_md.write_text("\n".join(lines) + "\n")
    print(summary_md.read_text())


if __name__ == "__main__":
    main()
