import csv
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_benchmark_regression.py"


def row(**changes):
    record = {
        "operation": "encode",
        "data_shards": 4,
        "parity_shards": 2,
        "shard_size": 1024,
        "throughput_mb_s": 100.0,
        "ns_per_iter": 1000.0,
    }
    record.update(changes)
    return record


class BenchmarkRegressionGateTests(unittest.TestCase):
    def run_gate(self, baseline, current, *arguments, current_csv=False):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            baseline_path = root / "baseline.json"
            baseline_path.write_text(json.dumps(baseline))
            current_path = root / ("current.csv" if current_csv else "current.json")
            if current_csv:
                with current_path.open("w", newline="") as output:
                    writer = csv.DictWriter(output, fieldnames=list(current[0]))
                    writer.writeheader()
                    writer.writerows(current)
            else:
                current_path.write_text(json.dumps(current))
            return subprocess.run(
                [sys.executable, str(SCRIPT), "--baseline", str(baseline_path), "--current", str(current_path), *arguments],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_empty_artifact_fails_on_either_side(self):
        for baseline, current in (([], []), ([row()], []), ([], [row()])):
            with self.subTest(baseline=baseline, current=current):
                result = self.run_gate(baseline, current)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("nonempty list", result.stderr)

    def test_no_common_case_fails(self):
        result = self.run_gate([row()], [row(shard_size=2048)])
        self.assertEqual(result.returncode, 1)
        self.assertIn("no comparable benchmark cases", result.stdout)

    def test_dropped_baseline_case_fails_without_an_explicit_partial_profile(self):
        baseline = [row(), row(operation="reconstruct")]
        result = self.run_gate(baseline, [row()])
        self.assertEqual(result.returncode, 1)
        self.assertIn("baseline case missing from current results", result.stdout)
        permitted = self.run_gate(baseline, [row()], "--allow-missing-cases")
        self.assertEqual(permitted.returncode, 0, permitted.stderr)
        report = json.loads(permitted.stdout)
        self.assertEqual(report["missing_cases"][0]["case"], "reconstruct:4:2:1024")
        required = self.run_gate(baseline, [row()], "--allow-missing-cases", "--require-case", "reconstruct:4:2:1024")
        self.assertEqual(required.returncode, 1)
        self.assertIn("required case missing from current", required.stdout)

    def test_partial_profile_flag_still_requires_at_least_one_comparison(self):
        result = self.run_gate([row()], [row(shard_size=2048)], "--allow-missing-cases")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no comparable benchmark cases", result.stdout)

    def test_duplicate_case_fails_on_either_side(self):
        for baseline, current in (([row(), row()], [row()]), ([row()], [row(), row()])):
            with self.subTest(baseline=baseline, current=current):
                result = self.run_gate(baseline, current)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("duplicate case", result.stderr)

    def test_required_case_must_exist_in_both_artifacts(self):
        for missing_source in ("baseline", "current"):
            baseline = [row()]
            current = [row()]
            (current if missing_source == "baseline" else baseline).append(row(shard_size=2048))
            result = self.run_gate(baseline, current, "--require-case", "encode:4:2:2048")
            self.assertEqual(result.returncode, 1)
            self.assertIn(f"required case missing from {missing_source}", result.stdout)

    def test_invalid_metrics_fail_on_either_side(self):
        for metric in ("throughput_mb_s", "ns_per_iter", "ns_per_block"):
            for value in (0, -1, float("nan"), float("inf"), -float("inf"), True, "bad", None):
                for side in ("baseline", "current"):
                    with self.subTest(metric=metric, value=value, side=side):
                        baseline = row()
                        current = row()
                        (baseline if side == "baseline" else current)[metric] = value
                        result = self.run_gate([baseline], [current])
                        self.assertNotEqual(result.returncode, 0)

    def test_nonpositive_or_fractional_case_dimensions_fail(self):
        for dimension in ("data_shards", "parity_shards", "shard_size", "stream_block_size"):
            for value in (0, -1, 1.5, True, None):
                with self.subTest(dimension=dimension, value=value):
                    result = self.run_gate([row()], [row(**{dimension: value})])
                    self.assertNotEqual(result.returncode, 0)

    def test_available_provenance_mismatches_cannot_be_ignored(self):
        fields = {
            "schema_version": (1, 2),
            "artifact_kind": ("smoke-results", "small-file-results"),
            "target_triple": ("aarch64-macos-unknown", "x86_64-linux-unknown"),
            "features": ("std", "std|simd-accel"),
            "benchmark_metrics_enabled": (False, True),
            "backend": ("scalar-rust", "rust-avx2"),
            "backend_id": ("ScalarRust", "RustAvx2"),
            "backend_kind": ("Scalar", "RustSimd"),
            "backend_override": ("auto", "rust-avx2"),
            "override_honored": (True, False),
            "stream_io_backend": ("memory", "disk"),
            "stream_io_mode": ("serial", "auto"),
            "cpu_model": ("cpu-a", "cpu-b"),
        }
        for field, (old, new) in fields.items():
            with self.subTest(field=field):
                result = self.run_gate([row(**{field: old})], [row(**{field: new})], "--allow-missing-provenance")
                self.assertEqual(result.returncode, 1)
                self.assertIn("incompatible provenance", result.stdout)

    def test_legacy_metadata_missing_on_both_sides_is_reported(self):
        result = self.run_gate([row()], [row()])
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["comparisons"]), 1)
        self.assertTrue(report["unchecked_provenance"])

    def test_one_sided_missing_metadata_requires_explicit_override(self):
        for baseline, current in ((row(), row(backend="scalar-rust")), (row(backend="scalar-rust"), row())):
            result = self.run_gate([baseline], [current])
            self.assertEqual(result.returncode, 1)
            self.assertIn("provenance missing", result.stdout)
            permitted = self.run_gate([baseline], [current], "--allow-missing-provenance")
            self.assertEqual(permitted.returncode, 0, permitted.stderr)
            self.assertTrue(json.loads(permitted.stdout)["unchecked_provenance"])

    def test_json_and_csv_provenance_compare_consistently(self):
        baseline = row(features="std|simd-accel", benchmark_metrics_enabled=False, schema_version=1)
        current = row(features="simd-accel|std", benchmark_metrics_enabled="false", schema_version="1")
        result = self.run_gate([baseline], [current], current_csv=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_commit_and_iteration_count_can_change(self):
        result = self.run_gate([row(git_revision="old", iterations=5)], [row(git_revision="new", iterations=10)])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_throughput_and_latency_regressions_fail(self):
        for metric, value in (("throughput_mb_s", 80), ("ns_per_iter", 1200)):
            with self.subTest(metric=metric):
                result = self.run_gate([row()], [row(**{metric: value})], "--metric", metric)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(len(json.loads(result.stdout)["failures"]), 1)

    def test_valid_improvement_and_threshold_override_pass(self):
        improvement = self.run_gate([row()], [row(throughput_mb_s=120)])
        self.assertEqual(improvement.returncode, 0, improvement.stderr)
        override = self.run_gate([row()], [row(throughput_mb_s=80)], "--threshold", "encode=0.25")
        self.assertEqual(override.returncode, 0, override.stderr)

    def test_invalid_thresholds_fail(self):
        for value in ("nan", "inf", "-0.1", "bad"):
            with self.subTest(value=value):
                result = self.run_gate([row()], [row()], "--threshold", f"encode={value}")
                self.assertNotEqual(result.returncode, 0)

    def test_stream_case_and_per_block_metric_pass(self):
        stream = row(operation="encode_stream", stream_block_size=4096, ns_per_block=100)
        result = self.run_gate([stream], [stream], "--metric", "ns_per_block", "--require-case", "encode_stream:4:2:1024:4096")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_requested_per_block_metric_fails(self):
        result = self.run_gate([row()], [row()], "--metric", "ns_per_block")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not contain ns_per_block", result.stderr)


if __name__ == "__main__":
    unittest.main()
