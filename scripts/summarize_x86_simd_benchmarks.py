#!/usr/bin/env python3
import argparse
import csv
import json
import pathlib
import platform
import subprocess
from statistics import mean
from typing import Dict, List, Tuple

KNOWN_BACKENDS = {
    "auto",
    "scalar",
    "rust-ssse3",
    "rust-avx2",
    "rust-avx512",
    "rust-gfni-avx2",
    "rust-gfni-avx512",
}

DEFAULT_POLICY_ELIGIBLE_BACKENDS_X86 = {
    "rust-gfni-avx512",
    "rust-gfni-avx2",
    "rust-avx2",
    "rust-avx512",
    "rust-ssse3",
    "scalar",
}

SUPPORTED_RELEASE_SMOKE_FILES = {
    "smoke-results-release-auto.csv",
    "smoke-results-release-scalar.csv",
    "smoke-results-release-rust-ssse3.csv",
    "smoke-results-release-rust-avx2.csv",
    "smoke-results-release-rust-avx512.csv",
    "smoke-results-release-rust-gfni-avx2.csv",
    "smoke-results-release-rust-gfni-avx512.csv",
}

CURRENT_RUNTIME_PRIORITY_X86 = [
    "rust-gfni-avx512",
    "rust-gfni-avx2",
    "rust-avx2",
    "rust-avx512",
    "rust-ssse3",
    "scalar",
]


def load_json(path: pathlib.Path):
    with path.open() as f:
        return json.load(f)


def load_csv(path: pathlib.Path):
    with path.open() as f:
        return list(csv.DictReader(f))


def capture_machine_info() -> Dict[str, str]:
    info = {
        "hostname": platform.node(),
        "arch": platform.machine(),
        "platform": platform.platform(),
    }

    try:
        info["lscpu"] = subprocess.check_output(["lscpu"], text=True)
    except (FileNotFoundError, subprocess.CalledProcessError):
        info["lscpu"] = ""

    try:
        info["uname_a"] = subprocess.check_output(["uname", "-a"], text=True).strip()
    except (FileNotFoundError, subprocess.CalledProcessError):
        info["uname_a"] = ""

    return info


def collect_criterion(root: pathlib.Path) -> List[Dict]:
    criterion_dir = root / "target" / "criterion"
    rows = []
    for path in sorted(criterion_dir.glob("**/new/estimates.json")):
        rel = path.relative_to(criterion_dir)
        parts = rel.parts
        if len(parts) < 3:
            continue
        benchmark_meta = load_json(path.parent / "benchmark.json")
        bench_name = benchmark_meta["group_id"]
        length = benchmark_meta["function_id"]
        data = load_json(path)
        rows.append(
            {
                "benchmark": bench_name,
                "length": length,
                "mean_ns": data["mean"]["point_estimate"],
                "lower_ns": data["mean"]["confidence_interval"]["lower_bound"],
                "upper_ns": data["mean"]["confidence_interval"]["upper_bound"],
            }
        )
    return rows


def collect_release_smoke(root: pathlib.Path) -> Dict[str, List[Dict]]:
    smoke_dir = root / "target" / "benchmark-smoke"
    out = {}
    for path in sorted(smoke_dir.glob("smoke-results-release-*.csv")):
        if path.name not in SUPPORTED_RELEASE_SMOKE_FILES:
            continue
        out[path.name] = load_csv(path)
    return out


def parse_backend_override_from_benchmark(benchmark_name: str) -> str:
    marker = "_override_"
    if marker not in benchmark_name:
        return benchmark_name
    override = benchmark_name.split(marker, 1)[1]
    return override.rstrip("_")


def backend_rankings(machine_json: Dict) -> Dict[str, List[Dict]]:
    rankings = {}
    smoke = machine_json["release_smoke"]
    focus_case = {
        "data_shards": "10",
        "parity_shards": "4",
        "shard_size": "1048576",
    }
    for op in ["encode", "verify", "reconstruct", "reconstruct_data"]:
        rows = []
        for file_name, records in smoke.items():
            for record in records:
                if record["backend_override"] not in KNOWN_BACKENDS:
                    continue
                if record.get("override_honored", "true").lower() != "true":
                    continue
                if (
                    record["operation"] == op
                    and record["data_shards"] == focus_case["data_shards"]
                    and record["parity_shards"] == focus_case["parity_shards"]
                    and record["shard_size"] == focus_case["shard_size"]
                ):
                    rows.append(
                        {
                            "backend": record["backend"],
                            "backend_override": record["backend_override"],
                            "throughput_mb_s": float(record["throughput_mb_s"]),
                            "source": file_name,
                        }
                    )
        rankings[op] = sorted(rows, key=lambda item: item["throughput_mb_s"], reverse=True)
    return rankings


def criterion_rankings(machine_json: Dict) -> Dict[str, List[Dict]]:
    grouped: Dict[Tuple[str, str], List[float]] = {}
    for row in machine_json["criterion_galois_backend"]:
        benchmark = row["benchmark"]
        length = row["length"]
        grouped.setdefault((benchmark, length), []).append(float(row["mean_ns"]))

    rankings: Dict[str, List[Dict]] = {}
    for op in ["galois_mul_slice", "galois_mul_slice_xor"]:
        rows = []
        for (benchmark, length), values in grouped.items():
            if not benchmark.startswith(f"{op}_"):
                continue
            rows.append(
                {
                    "backend_override": parse_backend_override_from_benchmark(benchmark),
                    "benchmark": benchmark,
                    "length": length,
                    "mean_ns": mean(values),
                }
            )
        rankings[op] = sorted(rows, key=lambda item: item["mean_ns"])
    return rankings


def choose_recommended_priority(machine_json: Dict) -> Dict:
    smoke = machine_json.get("rankings_10x4_1m", {})
    criterion = criterion_rankings(machine_json)

    score: Dict[str, float] = {}

    smoke_weights = {
        "encode": 1.0,
        "verify": 1.2,
        "reconstruct": 1.5,
        "reconstruct_data": 1.5,
    }
    for op, weight in smoke_weights.items():
        rows = smoke.get(op, [])
        if not rows:
            continue
        best = rows[0]["throughput_mb_s"]
        for idx, row in enumerate(rows):
            override = row["backend_override"]
            if override not in KNOWN_BACKENDS:
                continue
            relative = row["throughput_mb_s"] / best if best else 0.0
            score[override] = score.get(override, 0.0) + relative * weight
            score[override] += max(0.0, (len(rows) - idx - 1) * 0.01)

    criterion_focus = {
        "galois_mul_slice": {"len_1048576": 0.5, "len_4194304": 0.75},
        "galois_mul_slice_xor": {"len_1048576": 0.5, "len_4194304": 0.75},
    }
    for op, lengths in criterion_focus.items():
        rows = criterion.get(op, [])
        per_length = {}
        for row in rows:
            per_length.setdefault(row["length"], []).append(row)
        for length, weight in lengths.items():
            length_rows = per_length.get(length, [])
            if not length_rows:
                continue
            best = length_rows[0]["mean_ns"]
            for idx, row in enumerate(length_rows):
                override = row["backend_override"]
                if override not in KNOWN_BACKENDS:
                    continue
                relative = best / row["mean_ns"] if row["mean_ns"] else 0.0
                score[override] = score.get(override, 0.0) + relative * weight
                score[override] += max(0.0, (len(length_rows) - idx - 1) * 0.005)

    if "auto" in score:
        del score["auto"]

    ordered = [
        {"backend_override": backend, "score": round(value, 4)}
        for backend, value in sorted(score.items(), key=lambda item: item[1], reverse=True)
    ]

    recommendation = {
        "priority_order": [row["backend_override"] for row in ordered],
        "scored_backends": ordered,
        "rationale": [
            "Release smoke results for 10x4_1m are weighted most heavily.",
            "Reconstruct and reconstruct_data are weighted above encode.",
            "Criterion mul_slice and mul_slice_xor at 1 MiB and 4 MiB break close ties.",
            "The recommendation is benchmark-driven and may differ from the current runtime dispatch order.",
        ],
        "current_runtime_priority_x86": CURRENT_RUNTIME_PRIORITY_X86,
        "diverges_from_current_runtime_priority_x86": [row["backend_override"] for row in ordered] != CURRENT_RUNTIME_PRIORITY_X86[: len(ordered)],
    }
    return recommendation


def choose_policy_eligible_priority(machine_json: Dict) -> Dict:
    recommendation = choose_recommended_priority(machine_json)
    eligible = [
        row
        for row in recommendation["scored_backends"]
        if row["backend_override"] in DEFAULT_POLICY_ELIGIBLE_BACKENDS_X86
    ]
    return {
        "priority_order": [row["backend_override"] for row in eligible],
        "scored_backends": eligible,
        "rationale": [
            "Starts from the benchmark-driven ranking.",
            "Filters out backends that are not currently eligible for default runtime dispatch.",
            "The eligible set is aligned to the runtime order implemented in src/galois_8/backend.rs.",
        ],
        "eligible_backends_x86": sorted(DEFAULT_POLICY_ELIGIBLE_BACKENDS_X86),
        "current_runtime_priority_x86": CURRENT_RUNTIME_PRIORITY_X86,
        "diverges_from_current_runtime_priority_x86": [row["backend_override"] for row in eligible] != CURRENT_RUNTIME_PRIORITY_X86[: len(eligible)],
    }


def adoption_decision_stub(machine_json: Dict) -> Dict:
    recommendation = machine_json.get("recommended_default_priority", {})
    mismatches = machine_json.get("release_smoke_override_mismatches", {})
    mismatch_count = sum(len(rows) for rows in mismatches.values())
    diverges = recommendation.get("diverges_from_current_runtime_priority_x86", False)

    return {
        "status": "manual-review-required",
        "reason": (
            "same-machine evidence must be reviewed manually before changing the "
            "default runtime priority"
        ),
        "override_mismatch_count": mismatch_count,
        "diverges_from_current_runtime_priority_x86": diverges,
        "recommended_priority_order": recommendation.get("priority_order", []),
        "suggested_labels": [
            "candidate-only",
            "candidate-default",
            "fallback-only",
        ],
    }


def write_machine_json(root: pathlib.Path, out_json: pathlib.Path, machine_slug: str, date_utc: str):
    release_smoke = collect_release_smoke(root)
    machine_info = capture_machine_info()
    report = {
        "date_utc": date_utc,
        "machine_slug": machine_slug,
        "hostname": machine_info["hostname"],
        "arch": machine_info["arch"],
        "platform": machine_info["platform"],
        "lscpu": machine_info["lscpu"],
        "uname_a": machine_info["uname_a"],
        "criterion_galois_backend": collect_criterion(root),
        "release_smoke": release_smoke,
        "release_smoke_override_mismatches": {
            file_name: [
                row for row in rows if row.get("override_honored", "true").lower() != "true"
            ]
            for file_name, rows in release_smoke.items()
        },
    }
    report["rankings_10x4_1m"] = backend_rankings(report)
    report["criterion_rankings"] = criterion_rankings(report)
    report["recommended_default_priority"] = choose_recommended_priority(report)
    report["policy_eligible_default_priority"] = choose_policy_eligible_priority(report)
    report["adoption_decision_stub"] = adoption_decision_stub(report)
    report["default_switch_checklist"] = {
        "same_machine_required": True,
        "repeat_runs_required": True,
        "kernel_and_workload_evidence_required": True,
        "override_mismatches_must_be_empty": True,
        "manual_review_required_when_priority_diverges": True,
    }
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(report, indent=2))


def print_summary(root: pathlib.Path):
    bench_dir = root / "benchmarks" / "x86_64-simd"
    rows = []
    for path in sorted(
        path for path in bench_dir.glob("*.json") if not path.name.endswith(".run-meta.json")
    ):
        data = load_json(path)
        rankings = data.get("rankings_10x4_1m", {})
        recommendation = data.get("recommended_default_priority", {})
        top = {op: (rankings.get(op) or [{}])[0] for op in ["encode", "verify", "reconstruct", "reconstruct_data"]}
        rows.append(
            {
                "file": path.name,
                "encode": top["encode"].get("backend_override", "n/a"),
                "verify": top["verify"].get("backend_override", "n/a"),
                "reconstruct": top["reconstruct"].get("backend_override", "n/a"),
                "reconstruct_data": top["reconstruct_data"].get("backend_override", "n/a"),
                "recommended_default_priority": recommendation.get("priority_order", []),
                "recommendation_rationale": recommendation.get("rationale", []),
                "adoption_decision_status": data.get("adoption_decision_stub", {}).get(
                    "status", "legacy-archive-no-stub"
                ),
                "override_mismatch_count": data.get("adoption_decision_stub", {}).get("override_mismatch_count", 0),
            }
        )
    print(json.dumps(rows, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--machine-json")
    parser.add_argument("--machine-slug")
    parser.add_argument("--date")
    parser.add_argument("--summary", action="store_true")
    args = parser.parse_args()

    root = pathlib.Path(args.root).resolve()

    if args.summary:
        print_summary(root)
        return

    if not args.machine_json or not args.machine_slug or not args.date:
        raise SystemExit("--machine-json, --machine-slug and --date are required unless --summary is used")

    write_machine_json(root, pathlib.Path(args.machine_json), args.machine_slug, args.date)


if __name__ == "__main__":
    main()
