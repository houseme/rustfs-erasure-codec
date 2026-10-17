#!/usr/bin/env python3
import argparse
import json
import math
import pathlib
from typing import Dict, Iterable

from check_benchmark_regression import PROVENANCE_FIELDS, positive_integer, positive_metric, provenance_value


CASE_FIELDS = (
    "missing_pattern", "required_pattern", "baseline_operation", "candidate_operation",
    "data_shards", "parity_shards", "shard_size", "useful_shards",
)


def reject_nonfinite_json(value):
    raise ValueError(f"non-finite JSON number: {value}")


def load_results(path: pathlib.Path, metric: str) -> Dict[str, dict]:
    try:
        with path.open() as f:
            rows = json.load(f, parse_constant=reject_nonfinite_json)
        if not isinstance(rows, list) or not rows:
            raise ValueError("expected a nonempty list of hotspot results")
        result = {}
        for row in rows:
            if not isinstance(row, dict):
                raise ValueError("each hotspot result must be an object")
            scenario = row.get("scenario")
            if not isinstance(scenario, str) or not scenario.strip():
                raise ValueError("scenario must be a nonempty string")
            if scenario in result:
                raise ValueError(f"duplicate hotspot scenario: {scenario}")
            # Use the same alias/boolean/thread normalization as the main gate.
            for field in PROVENANCE_FIELDS:
                provenance_value(row, field)
            row[metric] = positive_metric(row[metric], metric)
            for name in ("candidate_mb_s", "baseline_mb_s", "speedup"):
                if name in row:
                    row[name] = positive_metric(row[name], name)
            for name in ("data_shards", "parity_shards", "shard_size", "useful_shards"):
                if name in row:
                    row[name] = positive_integer(row[name], name)
            result[scenario] = row
        return result
    except (OSError, ValueError, TypeError, KeyError) as error:
        raise SystemExit(f"{path}: {error}") from error


def parse_key_value(items: Iterable[str], flag: str) -> Dict[str, float]:
    values: Dict[str, float] = {}
    for item in items:
        if "=" not in item:
            raise SystemExit(f"{flag} expects SCENARIO=VALUE, got: {item}")
        key, raw = item.split("=", 1)
        try:
            value = float(raw)
        except ValueError as error:
            raise SystemExit(f"{flag} expects numeric VALUE, got: {item}") from error
        if not key.strip() or key in values:
            raise SystemExit(f"{flag} requires unique nonempty scenario names")
        if not math.isfinite(value) or value < 0 or (flag != "--min-speedup" and value > 1):
            raise SystemExit(f"{flag} expects a finite nonnegative value (regression at most 1)")
        values[key] = value
    return values


def regression_ratio(current: float, baseline: float) -> float:
    return max(0.0, (baseline - current) / baseline)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--current", required=True)
    parser.add_argument(
        "--require-scenario",
        action="append",
        default=[],
        help="Require that this hotspot scenario exists in both baseline and current results.",
    )
    parser.add_argument(
        "--metric",
        choices=("candidate_mb_s", "baseline_mb_s", "speedup"),
        default="candidate_mb_s",
        help="Metric compared against baseline for regression detection.",
    )
    parser.add_argument(
        "--max-regression",
        type=float,
        default=0.12,
        help="Allowed fractional regression vs baseline for the selected metric.",
    )
    parser.add_argument(
        "--scenario-max-regression",
        action="append",
        default=[],
        help="Per-scenario regression threshold in SCENARIO=VALUE form.",
    )
    parser.add_argument(
        "--min-speedup",
        action="append",
        default=[],
        help="Per-scenario minimum accepted current speedup in SCENARIO=VALUE form.",
    )
    args = parser.parse_args()

    if not math.isfinite(args.max_regression) or not 0 <= args.max_regression <= 1:
        parser.error("--max-regression must be a finite fraction between 0 and 1")

    baseline = load_results(pathlib.Path(args.baseline), args.metric)
    current = load_results(pathlib.Path(args.current), args.metric)
    scenario_max_regression = parse_key_value(
        args.scenario_max_regression, "--scenario-max-regression"
    )
    min_speedup = parse_key_value(args.min_speedup, "--min-speedup")

    required_scenarios = set(baseline) | set(args.require_scenario)
    required_scenarios.update(scenario_max_regression)
    required_scenarios.update(min_speedup)

    missing = sorted(
        scenario
        for scenario in required_scenarios
        if scenario not in baseline or scenario not in current
    )
    if missing:
        raise SystemExit(
            "missing required hotspot scenarios: " + ", ".join(missing)
        )

    failures = []
    for scenario in sorted(required_scenarios):
        baseline_row = baseline[scenario]
        current_row = current[scenario]

        try:
            baseline_metric = baseline_row[args.metric]
            current_metric = current_row[args.metric]
            for field in CASE_FIELDS:
                if baseline_row.get(field) != current_row.get(field):
                    raise ValueError(f"incomparable {field} for {scenario}")
            for field in PROVENANCE_FIELDS:
                before = provenance_value(baseline_row, field)
                after = provenance_value(current_row, field)
                if before != after:
                    raise ValueError(f"incomparable {field} for {scenario}")
            speedup = (
                positive_metric(current_row["speedup"], "speedup")
                if scenario in min_speedup else None
            )
        except (ValueError, TypeError, KeyError) as error:
            raise SystemExit(f"invalid hotspot evidence for {scenario}: {error}") from error
        allowed_regression = scenario_max_regression.get(
            scenario, args.max_regression
        )
        actual_regression = regression_ratio(current_metric, baseline_metric)
        if actual_regression > allowed_regression:
            failures.append(
                (
                    scenario,
                    f"{args.metric} regressed by {actual_regression:.4f}, "
                    f"allowed {allowed_regression:.4f}",
                )
            )

        if scenario in min_speedup:
            if speedup < min_speedup[scenario]:
                failures.append(
                    (
                        scenario,
                        f"speedup {speedup:.4f} is below floor {min_speedup[scenario]:.4f}",
                    )
                )

    if failures:
        details = "\n".join(f"- {scenario}: {message}" for scenario, message in failures)
        raise SystemExit(f"reconstruction hotspot gate failed:\n{details}")

    print(
        "reconstruction hotspot gate passed for "
        + ", ".join(sorted(required_scenarios))
    )


if __name__ == "__main__":
    main()
