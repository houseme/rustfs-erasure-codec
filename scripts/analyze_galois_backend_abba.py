#!/usr/bin/env python3
"""Validate galois_backend ABBA campaigns and gate attribution on both drifts."""

from __future__ import annotations

import argparse
import csv
import json
import math
import pathlib
import re
import sys
from collections import defaultdict
from statistics import mean
from typing import Any


STAGES = ("A1", "B1", "B2", "A2")
REQUIRED_COLUMNS = {"stage", "commit", "criterion_group", "operation", "length_bytes", "mean_ns"}
UNKNOWN_VALUES = {"unknown", "none", "null", "n/a", "na", "unspecified", "unavailable"}
TOKEN = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]*\Z")
REVISION = re.compile(r"(?:[0-9a-f]{4,40}|[0-9a-f]{64})\Z")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--csv", required=True)
    parser.add_argument("--summary-json", required=True)
    parser.add_argument("--summary-md", required=True)
    # Validate after parsing so an invalid threshold also replaces old reports
    # with an explicit failed campaign instead of leaving a stale PASS artifact.
    parser.add_argument("--drift-threshold", default="0.05")
    arguments = list(sys.argv[1:])
    # argparse normally mistakes negative special floats (for example -inf)
    # for option names. Keep the existing two-argument CLI usable while routing
    # those malformed values through the same failed-report path as NaN.
    for index in range(len(arguments) - 1):
        if arguments[index] == "--drift-threshold" and arguments[index + 1].startswith("-"):
            if not arguments[index + 1].startswith("--"):
                arguments[index:index + 2] = [f"--drift-threshold={arguments[index + 1]}"]
                break
    return parser.parse_args(arguments)


def known_token(value: str) -> bool:
    return bool(TOKEN.fullmatch(value)) and value.lower() not in UNKNOWN_VALUES


def load_rows(path: pathlib.Path) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    errors: list[str] = []
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, strict=True)
        fields = reader.fieldnames
        if not fields:
            return [], ["CSV must have a header and a nonempty campaign"]
        if len(fields) != len(set(fields)):
            return [], ["CSV has duplicate column names"]
        missing = REQUIRED_COLUMNS - set(fields)
        if missing:
            return [], [f"CSV missing required columns: {', '.join(sorted(missing))}"]
        override_column = "backend_override" in fields
        row_count = 0
        for row in reader:
            row_count += 1
            prefix = f"CSV row {reader.line_num}"
            if None in row:
                errors.append(f"{prefix}: unexpected extra CSV values")
                continue
            values = {key: (row.get(key) or "").strip() for key in REQUIRED_COLUMNS}
            problems: list[str] = []
            stage = values["stage"]
            if stage not in STAGES:
                problems.append(f"unknown stage {stage!r}; expected A1/B1/B2/A2")
            commit = values["commit"].lower()
            # The extractor records git rev-parse --short HEAD. Mutable branch
            # names cannot establish that repeated stages used the same code.
            if not REVISION.fullmatch(commit) or set(commit) == {"0"}:
                problems.append("commit must be a resolved nonzero hexadecimal revision (4..40 or 64 digits)")
            group = values["criterion_group"]
            operation = values["operation"]
            if group.startswith("galois_mul_slice_xor_"):
                expected_operation = "mul_slice_xor"
            elif group.startswith("galois_mul_slice_"):
                expected_operation = "mul_slice"
            else:
                expected_operation = None
            group_prefix = f"galois_{expected_operation}_"
            backend_label = group[len(group_prefix):] if expected_operation else ""
            if not expected_operation or not known_token(backend_label):
                problems.append("criterion_group must contain a valid galois operation and backend label")
            elif operation != expected_operation:
                problems.append("operation does not match criterion_group provenance")

            backend_override = None
            if override_column:
                backend_override = (row.get("backend_override") or "").strip()
                if not known_token(backend_override):
                    problems.append("backend_override column must be nonempty and known on every row")
            if "_override_" in backend_label:
                embedded_override = backend_label.rsplit("_override_", 1)[1]
                if backend_label.count("_override_") != 1 or not known_token(embedded_override):
                    problems.append("criterion_group has an invalid embedded backend override")
                elif backend_override is not None and backend_override != embedded_override:
                    problems.append("backend_override conflicts with criterion_group provenance")
                elif backend_override is None:
                    backend_override = embedded_override

            try:
                length = int(values["length_bytes"])
                if length <= 0:
                    raise ValueError
            except ValueError:
                length = 0
                problems.append("length_bytes must be a positive integer")
            try:
                estimate = float(values["mean_ns"])
                if not math.isfinite(estimate) or estimate <= 0:
                    raise ValueError
            except ValueError:
                estimate = 0.0
                problems.append("mean_ns must be finite and positive")
            if problems:
                errors.extend(f"{prefix}: {problem}" for problem in problems)
                continue
            rows.append({
                "stage": stage,
                "commit": commit,
                "criterion_group": group,
                "operation": operation,
                "length_bytes": length,
                "mean_ns": estimate,
                "backend_override": backend_override,
            })
        if not row_count:
            errors.append("CSV campaign must be nonempty")
    return rows, errors


def validate_campaign(rows: list[dict[str, Any]]) -> tuple[dict, dict[str, str], list[str]]:
    grouped: dict[tuple[str, str, int], dict[str, float]] = defaultdict(dict)
    revisions: dict[str, set[str]] = defaultdict(set)
    overrides: dict[str, set[str | None]] = defaultdict(set)
    errors: list[str] = []
    for row in rows:
        key = (row["criterion_group"], row["operation"], row["length_bytes"])
        stage = row["stage"]
        if stage in grouped[key]:
            errors.append(f"duplicate estimate for {key!r}, stage {stage}")
        else:
            grouped[key][stage] = row["mean_ns"]
        revisions[stage].add(row["commit"])
        overrides[row["criterion_group"]].add(row["backend_override"])
    for key, stages in sorted(grouped.items()):
        missing = [stage for stage in STAGES if stage not in stages]
        if missing:
            errors.append(f"incomplete ABBA case {key!r}: missing {', '.join(missing)}")
    commits = {}
    for stage in STAGES:
        if len(revisions[stage]) > 1:
            errors.append(f"stage {stage} contains inconsistent revisions")
        elif revisions[stage]:
            commits[stage] = next(iter(revisions[stage]))
    for first, second in (("A1", "A2"), ("B1", "B2")):
        if first in commits and second in commits and commits[first] != commits[second]:
            errors.append(f"revision mismatch: {first} must equal {second}")
    for group, values in sorted(overrides.items()):
        if len(values) > 1:
            errors.append(f"inconsistent backend_override provenance for {group!r}")
    return grouped, commits, errors


def ratio_delta(later: float, earlier: float) -> float:
    result = (later - earlier) / earlier
    if not math.isfinite(result):
        raise ValueError("derived ratio is not finite")
    return result


def analyze(grouped: dict, threshold: float) -> tuple[list[dict[str, Any]], list[str]]:
    cases = []
    errors = []
    for (group, operation, length), stages in sorted(grouped.items()):
        a1, b1, b2, a2 = (stages[stage] for stage in STAGES)
        a_mean = mean([a1, a2])
        b_mean = mean([b1, b2])
        case = {
            "criterion_group": group,
            "operation": operation,
            "length_bytes": length,
            "a1_mean_ns": a1,
            "a2_mean_ns": a2,
            "a_mean_ns": a_mean,
            "b1_mean_ns": b1,
            "b2_mean_ns": b2,
            "b_mean_ns": b_mean,
            "baseline_drift": None,
            "candidate_drift": None,
            "candidate_delta_vs_a_mean": None,
            "drift_failed": True,
        }
        try:
            case["baseline_drift"] = ratio_delta(a2, a1)
            case["candidate_drift"] = ratio_delta(b2, b1)
            case["drift_failed"] = (
                abs(case["baseline_drift"]) > threshold
                or abs(case["candidate_drift"]) > threshold
            )
            # This remains provisional until every case passes the campaign gate.
            case["candidate_delta_vs_a_mean"] = ratio_delta(b_mean, a_mean)
        except (ValueError, OverflowError) as error:
            errors.append(f"{group}:{operation}:{length}: {error}")
        cases.append(case)
    return cases, errors


def render_markdown(payload: dict[str, Any]) -> str:
    threshold = payload["drift_threshold"]
    threshold_text = f"{threshold:.2%}" if threshold is not None else "invalid"
    commits = payload["commits"]
    lines = [
        "# Galois Backend ABBA Summary",
        "",
        f"- Drift threshold: {threshold_text}",
        f"- Input gate: {'FAIL' if payload['validation_errors'] else 'PASS'}",
        f"- Drift gate: {'FAIL' if payload['drift_failed'] else 'PASS'}",
        f"- Candidate attribution: {'PERMITTED' if payload['attribution_allowed'] else 'WITHHELD for the entire campaign'}",
        f"- Commits: A={commits.get('A1', 'unknown')} / B={commits.get('B1', 'unknown')}",
        "",
    ]
    if payload["validation_errors"]:
        lines.extend(["Validation errors:", ""])
        lines.extend(f"- {error}" for error in payload["validation_errors"])
        lines.append("")
    lines.extend([
        "| Criterion group | Operation | Length | A1 mean ns | A2 mean ns | A drift | B1 mean ns | B2 mean ns | B drift | B vs A mean | Gate |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ])
    for case in payload["cases"]:
        def percentage(value: float | None) -> str:
            return f"{value:.2%}" if value is not None else "not attributable"

        lines.append(
            "| {criterion_group} | {operation} | {length_bytes} | {a1_mean_ns:.2f} | {a2_mean_ns:.2f} | "
            "{a_drift} | {b1_mean_ns:.2f} | {b2_mean_ns:.2f} | {b_drift} | {delta} | {gate} |".format(
                **case,
                a_drift=percentage(case["baseline_drift"]),
                b_drift=percentage(case["candidate_drift"]),
                delta=percentage(case["candidate_delta_vs_a_mean"]),
                gate="FAIL" if case["drift_failed"] else "PASS",
            )
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    errors: list[str] = []
    threshold = None
    try:
        threshold = float(args.drift_threshold)
        if not math.isfinite(threshold) or threshold < 0:
            raise ValueError
    except ValueError:
        threshold = None
        errors.append("drift threshold must be finite and nonnegative")
    rows: list[dict[str, Any]] = []
    try:
        rows, input_errors = load_rows(pathlib.Path(args.csv))
        errors.extend(input_errors)
    except (OSError, UnicodeError, csv.Error) as error:
        errors.append(f"cannot read CSV: {error}")
    grouped, commits, campaign_errors = validate_campaign(rows)
    errors.extend(campaign_errors)
    cases: list[dict[str, Any]] = []
    if not errors:
        cases, arithmetic_errors = analyze(grouped, threshold)
        errors.extend(arithmetic_errors)
    drift_failures = sum(case["drift_failed"] for case in cases)
    attribution_allowed = bool(cases) and not errors and drift_failures == 0
    if not attribution_allowed:
        for case in cases:
            case["candidate_delta_vs_a_mean"] = None
    payload = {
        "drift_threshold": threshold,
        # Preserve the old gate key, including invalid campaigns: existing
        # consumers must never interpret malformed input as a successful gate.
        "drift_failed": not attribution_allowed,
        "drift_failures": drift_failures,
        "validation_errors": errors,
        "attribution_allowed": attribution_allowed,
        "commits": commits,
        "criterion_groups": sorted({row["criterion_group"] for row in rows}),
        "cases": cases,
    }
    markdown = render_markdown(payload)
    try:
        pathlib.Path(args.summary_json).write_text(
            json.dumps(payload, indent=2, sort_keys=True, allow_nan=False) + "\n",
            encoding="utf-8",
        )
        pathlib.Path(args.summary_md).write_text(markdown, encoding="utf-8")
    except OSError as error:
        print(f"cannot write ABBA summaries: {error}", file=sys.stderr)
        return 2
    print(markdown, end="")
    if not attribution_allowed:
        print("ABBA campaign failed validation or drift checks; candidate attribution withheld", file=sys.stderr)
    return 0 if attribution_allowed else 1


if __name__ == "__main__":
    raise SystemExit(main())
