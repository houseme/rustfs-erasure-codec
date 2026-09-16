#!/usr/bin/env python3
"""Extract raw Criterion estimates for the galois_backend benchmark."""

from __future__ import annotations

import argparse
import csv
import json
import pathlib
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--criterion-root", default="target/criterion")
    parser.add_argument("--stage", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--backend-override", required=True)
    parser.add_argument("--csv", required=True)
    parser.add_argument("--jsonl", required=True)
    parser.add_argument("--write-header", action="store_true")
    parser.add_argument(
        "--updated-after-epoch",
        type=float,
        required=True,
        help="Only collect estimates updated after this UNIX timestamp.",
    )
    return parser.parse_args()


def load_json(path: pathlib.Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def operation_from_group(group: str) -> str | None:
    if group.startswith("galois_mul_slice_xor_"):
        return "mul_slice_xor"
    if group.startswith("galois_mul_slice_"):
        return "mul_slice"
    return None


def len_from_benchmark_name(name: str) -> int | None:
    prefix = "len_"
    if not name.startswith(prefix):
        return None
    try:
        return int(name[len(prefix) :])
    except ValueError:
        return None


def collect_records(args: argparse.Namespace) -> list[dict[str, Any]]:
    root = pathlib.Path(args.criterion_root)
    records: list[dict[str, Any]] = []
    for estimate_path in sorted(root.glob("galois_mul_slice*/len_*/new/estimates.json")):
        if estimate_path.stat().st_mtime < args.updated_after_epoch:
            continue
        group = estimate_path.parents[2].name
        operation = operation_from_group(group)
        length = len_from_benchmark_name(estimate_path.parents[1].name)
        if operation is None or length is None:
            continue

        estimates = load_json(estimate_path)
        mean = estimates["mean"]["point_estimate"]
        lower = estimates["mean"]["confidence_interval"]["lower_bound"]
        upper = estimates["mean"]["confidence_interval"]["upper_bound"]
        records.append(
            {
                "stage": args.stage,
                "commit": args.commit,
                "backend_override": args.backend_override,
                "criterion_group": group,
                "operation": operation,
                "length_bytes": length,
                "mean_ns": mean,
                "mean_lower_ns": lower,
                "mean_upper_ns": upper,
            }
        )
    return records


def main() -> None:
    args = parse_args()
    records = collect_records(args)
    if not records:
        raise SystemExit("no galois_backend Criterion estimates found")

    csv_path = pathlib.Path(args.csv)
    jsonl_path = pathlib.Path(args.jsonl)
    fieldnames = [
        "stage",
        "commit",
        "backend_override",
        "criterion_group",
        "operation",
        "length_bytes",
        "mean_ns",
        "mean_lower_ns",
        "mean_upper_ns",
    ]
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    jsonl_path.parent.mkdir(parents=True, exist_ok=True)

    with csv_path.open("a", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        if args.write_header:
            writer.writeheader()
        writer.writerows(records)

    with jsonl_path.open("a") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")

    print(f"extracted {len(records)} galois_backend estimates for {args.stage}")


if __name__ == "__main__":
    main()
