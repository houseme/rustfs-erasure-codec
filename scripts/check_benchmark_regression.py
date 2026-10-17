#!/usr/bin/env python3
"""Compare compatible benchmark rows, rejecting incomplete or invalid evidence.

Legacy artifacts may omit provenance fields on both sides. A field present on
only one side requires --allow-missing-provenance and is reported as unchecked.
That flag never permits unequal values available on both sides. Commit hashes,
iteration counts and seeds are intentionally not equality gates: they can change
between runs of the same workload. Workload profile labels are compared when
present, separately from the build profile.

--strict-provenance requires complete comparable build, CPU and thread-budget
metadata in every row. Legacy omission overrides do not waive these requirements.

Every baseline case must remain in current results. For a deliberately smaller
profile, --allow-missing-cases reports omitted cases without failing the gate;
explicit --require-case checks still apply to both artifacts.
"""

import argparse
import csv
import json
import math
import pathlib
import sys

OPERATIONS = (
    "encode",
    "verify",
    "verify_with_buffer",
    "reconstruct",
    "reconstruct_data",
    "encode_stream",
    "verify_stream",
    "reconstruct_stream",
)
CASE_KEY = ("data_shards", "parity_shards", "shard_size")
OPTIONAL_CASE_KEY = ("stream_block_size",)
PROVENANCE_FIELDS = (
    "schema_version",
    "artifact_kind",
    "target_triple",
    "features",
    "benchmark_metrics_enabled",
    "backend",
    "backend_id",
    "backend_kind",
    "backend_override",
    "override_honored",
    "stream_io_backend",
    "stream_io_mode",
    "cpu_model",
    "compiler",
    "build_profile",
    "profile",
    "rayon_threads",
    "available_parallelism",
    "max_parallel_jobs",
    "effective_max_jobs",
    "target_cpu",
    "rustflags",
)
PROVENANCE_ALIASES = {
    "backend": ("backend", "backend_name"),
    "cpu_model": ("cpu_model", "cpu"),
    "compiler": ("compiler", "compiler_version", "rustc", "rustc_version"),
    "build_profile": ("build_profile", "cargo_profile"),
    "rayon_threads": ("rayon_threads", "rayon_num_threads"),
    "max_parallel_jobs": ("max_parallel_jobs", "max_jobs"),
}
STRICT_PROVENANCE_FIELDS = (
    "target_triple",
    "features",
    "benchmark_metrics_enabled",
    "backend",
    "cpu_model",
    "compiler",
    "build_profile",
    "rayon_threads",
    "available_parallelism",
    "max_parallel_jobs",
)
STREAM_PROVENANCE_FIELDS = ("stream_io_backend", "stream_io_mode")
THREAD_FIELDS = ("rayon_threads", "available_parallelism", "max_parallel_jobs", "effective_max_jobs")
UNKNOWN_PROVENANCE = {"unknown", "unavailable", "unspecified", "n/a", "null", "none", "nan"}
DEFAULT_THRESHOLDS = {
    "encode": 0.10,
    "verify": 0.12,
    "verify_with_buffer": 0.12,
    "reconstruct": 0.15,
    "reconstruct_data": 0.15,
    "encode_stream": 0.12,
    "verify_stream": 0.12,
    "reconstruct_stream": 0.15,
}


def load_rows(path: pathlib.Path):
    if path.suffix == ".json":
        return json.loads(path.read_text())
    if path.suffix == ".csv":
        with path.open() as f:
            return list(csv.DictReader(f))
    raise SystemExit(f"unsupported file type: {path}")


def positive_metric(value, name):
    if isinstance(value, bool):
        raise ValueError(f"{name} must be a finite positive number")
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise ValueError(f"{name} must be a finite positive number")
    return number


def positive_integer(value, name):
    raw = str(value)
    if not raw.isascii() or not raw.isdecimal() or int(raw) <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return str(int(raw))


def normalize_provenance_value(value, field):
    if value is None:
        return None
    if field in ("benchmark_metrics_enabled", "override_honored"):
        value = str(value).strip().lower()
        if value not in ("true", "false"):
            raise ValueError(f"{field} must be true or false")
    elif field in THREAD_FIELDS:
        # Configured max_parallel_jobs=0 means automatic, not a zero-size pool.
        raw = str(value)
        minimum = 0 if field == "max_parallel_jobs" else 1
        if not raw.isascii() or not raw.isdecimal() or int(raw) < minimum:
            kind = "nonnegative" if minimum == 0 else "positive"
            raise ValueError(f"{field} must be a {kind} integer")
        return str(int(raw))
    elif field == "features":
        # Cargo feature order is not semantically significant.
        if not isinstance(value, str):
            raise ValueError("features must be a pipe-separated string")
        value = "|".join(sorted(set(part.strip() for part in value.split("|") if part.strip())))
    else:
        if field != "schema_version" and not isinstance(value, str):
            raise ValueError(f"{field} must be a string")
        value = str(value).strip()
        if not value and field != "rustflags":
            raise ValueError(f"{field} must not be empty when present")
        if field == "compiler":
            # Keep version/hash/date intact; only normalize the optional tool
            # prefix and whitespace, never equate different compiler builds.
            value = " ".join(value.split()).removeprefix("rustc ")
        elif field == "cpu_model":
            value = " ".join(value.split())
        if field != "rustflags" and value.lower() in UNKNOWN_PROVENANCE:
            return None
    return value


def provenance_value(record, field):
    values = []
    for alias in PROVENANCE_ALIASES.get(field, (field,)):
        if alias in record:
            value = normalize_provenance_value(record[alias], field)
            if value is not None:
                values.append((alias, value))
    if not values:
        return None
    first = values[0][1]
    if any(value != first for _, value in values[1:]):
        raise ValueError(f"conflicting aliases for {field}: {', '.join(alias for alias, _ in values)}")
    return first


def strict_missing_fields(record):
    required = list(STRICT_PROVENANCE_FIELDS)
    if (
        record["operation"].endswith("_stream")
        or record["stream_block_size"]
        or record["artifact_kind"] == "stream-path-results"
    ):
        required.extend(STREAM_PROVENANCE_FIELDS)
    missing = []
    for field in required:
        value = record[field]
        unknown = UNKNOWN_PROVENANCE - {"none"} if field == "features" else UNKNOWN_PROVENANCE
        if value is None or value == "" or value.lower() in unknown:
            missing.append(field)
    return missing


def normalize_record(record):
    if not isinstance(record, dict):
        raise ValueError("each benchmark row must be an object")
    normalized = dict(record)
    if not isinstance(record["operation"], str) or not record["operation"].strip():
        raise ValueError("operation must be a nonempty string")
    normalized["operation"] = record["operation"].strip()
    normalized["throughput_mb_s"] = positive_metric(record["throughput_mb_s"], "throughput_mb_s")
    normalized["ns_per_iter"] = positive_metric(record["ns_per_iter"], "ns_per_iter")
    if "ns_per_block" in record:
        normalized["ns_per_block"] = positive_metric(record["ns_per_block"], "ns_per_block")
    for key in CASE_KEY:
        normalized[key] = positive_integer(record[key], key)
    for key in OPTIONAL_CASE_KEY:
        value = record.get(key, "")
        normalized[key] = positive_integer(value, key) if value != "" else ""
    for field in PROVENANCE_FIELDS:
        normalized[field] = provenance_value(record, field)
    return normalized


def index_records(rows, source="artifact", strict_provenance=False):
    if not isinstance(rows, list) or not rows:
        raise ValueError(f"{source}: expected a nonempty list of benchmark rows")
    indexed = {}
    for row_number, row in enumerate(rows, start=1):
        try:
            record = normalize_record(row)
            if strict_provenance:
                missing = strict_missing_fields(record)
                if missing:
                    raise ValueError(f"strict provenance missing or unknown fields: {', '.join(missing)}")
        except (KeyError, TypeError, ValueError, OverflowError) as error:
            raise ValueError(f"{source}: row {row_number}: {error}") from error
        key = (
            record["operation"],
            record["data_shards"],
            record["parity_shards"],
            record["shard_size"],
            record["stream_block_size"],
        )
        if key in indexed:
            raise ValueError(f"{source}: duplicate case {':'.join(key).rstrip(':')}")
        indexed[key] = record
    return indexed


def parse_threshold_overrides(values):
    thresholds = dict(DEFAULT_THRESHOLDS)
    for value in values:
        if "=" not in value:
            raise SystemExit(f"invalid threshold override '{value}', expected op=value")
        op, raw = value.split("=", 1)
        op = op.strip()
        try:
            threshold = float(raw)
        except ValueError as error:
            raise ValueError(f"invalid threshold override '{value}'") from error
        if not op or not math.isfinite(threshold) or threshold < 0:
            raise ValueError(f"invalid threshold override '{value}': expected a finite nonnegative ratio")
        thresholds[op] = threshold
    return thresholds


def metric_value(record, metric):
    if metric == "throughput_mb_s":
        return record["throughput_mb_s"]
    if metric == "ns_per_iter":
        return record["ns_per_iter"]
    if metric == "ns_per_block":
        if "ns_per_block" not in record:
            raise SystemExit("metric ns_per_block requested but artifact does not contain ns_per_block")
        return record["ns_per_block"]
    raise SystemExit(f"unsupported metric: {metric}")


def regression_ratio(baseline_value, current_value, metric):
    if metric == "throughput_mb_s":
        return max(0.0, (baseline_value - current_value) / baseline_value)
    if metric == "ns_per_iter":
        return max(0.0, (current_value - baseline_value) / baseline_value)
    if metric == "ns_per_block":
        return max(0.0, (current_value - baseline_value) / baseline_value)
    raise SystemExit(f"unsupported metric: {metric}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--current", required=True)
    parser.add_argument(
        "--threshold",
        action="append",
        default=[],
        help="Override allowed regression ratio, e.g. reconstruct=0.18",
    )
    parser.add_argument(
        "--metric",
        choices=("throughput_mb_s", "ns_per_iter", "ns_per_block"),
        default="throughput_mb_s",
        help="Metric used for regression detection. Use ns_per_iter for latency-sensitive small-file checks.",
    )
    parser.add_argument(
        "--require-case",
        action="append",
        default=[],
        help="Require a case in both artifacts: operation:data:parity:shard_size[:stream_block_size]",
    )
    parser.add_argument(
        "--allow-missing-cases",
        action="store_true",
        help="Allow an intentional subset of baseline cases; omitted cases remain reported and --require-case still applies",
    )
    parser.add_argument(
        "--strict-provenance",
        action="store_true",
        help="Require target/features/metrics/backend/CPU/compiler/build profile and thread budgets in every row, plus stream I/O provenance for stream cases",
    )
    parser.add_argument(
        "--allow-missing-provenance",
        action="store_true",
        help="Allow optional legacy fields absent on only one side; cannot waive strict requirements or available mismatches",
    )
    args = parser.parse_args()

    baseline_path = pathlib.Path(args.baseline)
    current_path = pathlib.Path(args.current)
    try:
        thresholds = parse_threshold_overrides(args.threshold)
        baseline = index_records(load_rows(baseline_path), "baseline", args.strict_provenance)
        current = index_records(load_rows(current_path), "current", args.strict_provenance)
    except (OSError, TypeError, ValueError) as error:
        parser.error(str(error))

    failures = []
    comparisons = []
    unchecked_provenance = []
    missing_cases = [
        {"case": ":".join(key).rstrip(":"), "error": "baseline case missing from current results"}
        for key in sorted(baseline.keys() - current.keys())
    ]
    if not args.allow_missing_cases:
        failures.extend(missing_cases)

    for key, current_record in sorted(current.items()):
        if key not in baseline:
            continue
        baseline_record = baseline[key]
        op = current_record["operation"]
        case = ":".join(key).rstrip(":")
        incompatible = False
        for field in PROVENANCE_FIELDS:
            old, new = baseline_record[field], current_record[field]
            if old is None or new is None:
                unchecked_provenance.append({"case": case, "field": field})
                if old == new or args.allow_missing_provenance:
                    continue
                reason = "provenance missing from one artifact"
            elif old == new:
                continue
            else:
                reason = "incompatible provenance"
            failures.append({"case": case, "field": field, "error": reason, "baseline": old, "current": new})
            incompatible = True
        if incompatible:
            continue
        baseline_value = metric_value(baseline_record, args.metric)
        current_value = metric_value(current_record, args.metric)
        ratio = regression_ratio(baseline_value, current_value, args.metric)
        comparisons.append(
            {
                "operation": op,
                "data_shards": key[1],
                "parity_shards": key[2],
                "shard_size": key[3],
                "stream_block_size": key[4],
                "metric": args.metric,
                "baseline_metric_value": baseline_value,
                "current_metric_value": current_value,
                "baseline_throughput_mb_s": baseline_record["throughput_mb_s"],
                "current_throughput_mb_s": current_record["throughput_mb_s"],
                "baseline_ns_per_iter": baseline_record["ns_per_iter"],
                "current_ns_per_iter": current_record["ns_per_iter"],
                "regression_ratio": ratio,
                "threshold": thresholds.get(op, 0.15),
            }
        )
        if ratio > thresholds.get(op, 0.15):
            failures.append(comparisons[-1])

    for required in args.require_case:
        parts = required.split(":")
        if len(parts) not in (4, 5):
            raise SystemExit(
                f"invalid required case '{required}', expected operation:data:parity:shard_size[:stream_block_size]"
            )
        try:
            numeric_parts = tuple(positive_integer(value, "required case dimension") for value in parts[1:])
        except ValueError as error:
            parser.error(str(error))
        key = (parts[0], *numeric_parts) if len(parts) == 5 else (parts[0], *numeric_parts, "")
        for source, records in (("baseline", baseline), ("current", current)):
            if key not in records:
                failures.append(
                    {
                        "operation": parts[0],
                        "data_shards": parts[1],
                        "parity_shards": parts[2],
                        "shard_size": parts[3],
                        "error": f"required case missing from {source} results",
                    }
                )

    if not comparisons:
        failures.append({"error": "no comparable benchmark cases"})

    summary = {
        "baseline": str(baseline_path),
        "current": str(current_path),
        "metric": args.metric,
        "strict_provenance": args.strict_provenance,
        "thresholds": thresholds,
        "comparisons": comparisons,
        "failures": failures,
        "unchecked_provenance": unchecked_provenance,
        "missing_cases": missing_cases,
    }
    print(json.dumps(summary, indent=2))

    if failures:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
