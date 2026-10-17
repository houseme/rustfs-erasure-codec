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


def strict_row(**changes):
    record = row(
        target_triple="x86_64-unknown-linux-gnu",
        features="std|simd-accel",
        benchmark_metrics_enabled=False,
        backend="rust-avx2",
        cpu_model="Example CPU 16C",
        compiler="rustc 1.98.1 (012345abc 2026-09-10)",
        build_profile="release",
        rayon_threads=4,
        available_parallelism=16,
        max_parallel_jobs=2,
    )
    record.update(changes)
    return record


class BenchmarkRegressionGateTests(unittest.TestCase):
    def run_gate(self, baseline, current, *arguments, baseline_csv=False, current_csv=False):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            baseline_path = root / ("baseline.csv" if baseline_csv else "baseline.json")
            current_path = root / ("current.csv" if current_csv else "current.json")
            for path, records, use_csv in (
                (baseline_path, baseline, baseline_csv),
                (current_path, current, current_csv),
            ):
                if use_csv:
                    with path.open("w", newline="") as output:
                        writer = csv.DictWriter(output, fieldnames=list(records[0]))
                        writer.writeheader()
                        writer.writerows(records)
                else:
                    path.write_text(json.dumps(records))
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
            "compiler": ("rustc 1.98.1", "rustc 1.99.0"),
            "build_profile": ("release", "dev"),
            "profile": ("fast", "extended"),
            "rayon_threads": (2, 4),
            "available_parallelism": (8, 16),
            "max_parallel_jobs": (0, 2),
            "effective_max_jobs": (2, 4),
            "target_cpu": ("generic", "native"),
            "rustflags": ("", "-C target-cpu=native"),
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

    def test_strict_provenance_accepts_complete_json_and_csv(self):
        for baseline_csv, current_csv in ((False, False), (True, False), (False, True), (True, True)):
            with self.subTest(baseline_csv=baseline_csv, current_csv=current_csv):
                result = self.run_gate(
                    [strict_row()], [strict_row()], "--strict-provenance",
                    baseline_csv=baseline_csv, current_csv=current_csv,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(json.loads(result.stdout)["strict_provenance"])

    def test_strict_provenance_requires_each_dimension_even_with_legacy_override(self):
        fields = (
            "target_triple", "features", "benchmark_metrics_enabled", "backend",
            "cpu_model", "compiler", "build_profile", "rayon_threads",
            "available_parallelism", "max_parallel_jobs",
        )
        for field in fields:
            for side in ("baseline", "current", "both"):
                with self.subTest(field=field, side=side):
                    baseline, current = strict_row(), strict_row()
                    if side in ("baseline", "both"):
                        baseline.pop(field)
                    if side in ("current", "both"):
                        current.pop(field)
                    result = self.run_gate(
                        [baseline], [current], "--strict-provenance", "--allow-missing-provenance",
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("strict provenance missing", result.stderr)
                    self.assertIn(field, result.stderr)

    def test_strict_provenance_rejects_legacy_rows_and_unmatched_incomplete_rows(self):
        legacy = self.run_gate([row()], [row()], "--strict-provenance")
        self.assertNotEqual(legacy.returncode, 0)
        extra = self.run_gate([strict_row()], [strict_row(), row(shard_size=2048)], "--strict-provenance")
        self.assertNotEqual(extra.returncode, 0)
        self.assertIn("current: row 2", extra.stderr)

    def test_strict_provenance_aliases_work_across_json_and_csv(self):
        aliases = {
            "backend": "backend_name", "cpu_model": "cpu", "compiler": "rustc_version",
            "build_profile": "cargo_profile", "rayon_threads": "rayon_num_threads",
            "max_parallel_jobs": "max_jobs",
        }
        canonical = strict_row()
        alternate = {aliases.get(key, key): value for key, value in canonical.items()}
        alternate["cpu"] = " Example   CPU 16C "
        alternate["rustc_version"] = "1.98.1 (012345abc 2026-09-10)"
        alternate["rayon_num_threads"] = "04"
        alternate["max_jobs"] = "02"
        for baseline, current in ((canonical, alternate), (alternate, canonical)):
            for current_csv in (False, True):
                result = self.run_gate([baseline], [current], "--strict-provenance", current_csv=current_csv)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_all_compiler_aliases_keep_build_identity(self):
        for alias in ("compiler", "compiler_version", "rustc", "rustc_version"):
            current = strict_row()
            current.pop("compiler")
            current[alias] = "1.98.1 (012345abc 2026-09-10)"
            result = self.run_gate([strict_row()], [current], "--strict-provenance")
            self.assertEqual(result.returncode, 0, result.stderr)
            current[alias] = "1.98.1 (different 2026-09-10)"
            result = self.run_gate([strict_row()], [current], "--strict-provenance", "--allow-missing-provenance")
            self.assertEqual(result.returncode, 1)
            self.assertIn("incompatible provenance", result.stdout)

    def test_conflicting_aliases_in_one_row_fail_even_in_legacy_mode(self):
        for first, second, old, new in (
            ("cpu_model", "cpu", "cpu-a", "cpu-b"),
            ("compiler", "rustc", "rustc 1.98.1", "rustc 1.99.0"),
            ("build_profile", "cargo_profile", "release", "dev"),
            ("rayon_threads", "rayon_num_threads", 2, 4),
            ("max_parallel_jobs", "max_jobs", 0, 4),
        ):
            with self.subTest(first=first):
                conflicting = row(**{first: old, second: new})
                result = self.run_gate([conflicting], [conflicting], "--allow-missing-provenance")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("conflicting aliases", result.stderr)

    def test_identical_aliases_in_one_row_are_accepted(self):
        record = strict_row(cpu=" Example CPU 16C ", rustc="1.98.1 (012345abc 2026-09-10)", max_jobs="02")
        result = self.run_gate([record], [strict_row()], "--strict-provenance")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_empty_cpu_compiler_and_profiles_are_invalid_in_both_modes(self):
        for field in ("cpu_model", "cpu", "compiler", "rustc_version", "build_profile", "cargo_profile", "profile"):
            for value in ("", "  "):
                for arguments in ((), ("--strict-provenance",)):
                    with self.subTest(field=field, value=value, arguments=arguments):
                        record = strict_row(**{field: value})
                        result = self.run_gate([record], [record], *arguments)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("must not be empty", result.stderr)

    def test_strict_rejects_unknown_placeholder_and_null_metadata(self):
        for field in ("target_triple", "features", "backend", "cpu_model", "compiler", "build_profile"):
            for value in (None, "unknown", "NaN", "n/a"):
                for current_csv in (False, True):
                    with self.subTest(field=field, value=value, current_csv=current_csv):
                        result = self.run_gate(
                            [strict_row()], [strict_row(**{field: value})], "--strict-provenance",
                            "--allow-missing-provenance", current_csv=current_csv,
                        )
                        self.assertNotEqual(result.returncode, 0)

    def test_strict_feature_none_is_explicit_but_blank_is_missing(self):
        result = self.run_gate([strict_row(features="none")], [strict_row(features="none")], "--strict-provenance")
        self.assertEqual(result.returncode, 0, result.stderr)
        for value in ("", " | "):
            result = self.run_gate([strict_row(features=value)], [strict_row(features=value)], "--strict-provenance")
            self.assertNotEqual(result.returncode, 0)

    def test_thread_budgets_are_normalized_integers_with_only_max_jobs_allowing_zero(self):
        for field in ("rayon_threads", "available_parallelism", "max_parallel_jobs", "effective_max_jobs"):
            for value in (True, False, -1, 1.5, "1.5", "nan", "", " 4 "):
                with self.subTest(field=field, value=value):
                    result = self.run_gate([row(**{field: value})], [row(**{field: value})])
                    self.assertNotEqual(result.returncode, 0)
        for field in ("rayon_threads", "available_parallelism", "effective_max_jobs"):
            result = self.run_gate([strict_row(**{field: 0})], [strict_row(**{field: 0})], "--strict-provenance")
            self.assertNotEqual(result.returncode, 0)
        result = self.run_gate([strict_row(max_parallel_jobs=0)], [strict_row(max_parallel_jobs="000")], "--strict-provenance")
        self.assertEqual(result.returncode, 0, result.stderr)
        mismatch = self.run_gate([strict_row(max_parallel_jobs=0)], [strict_row(max_parallel_jobs=2)], "--strict-provenance")
        self.assertEqual(mismatch.returncode, 1)

    def test_strict_stream_cases_require_both_io_dimensions(self):
        stream = strict_row(operation="encode_stream", stream_block_size=1024, stream_io_backend="memory", stream_io_mode="serial")
        passed = self.run_gate([stream], [stream], "--strict-provenance")
        self.assertEqual(passed.returncode, 0, passed.stderr)
        for field in ("stream_io_backend", "stream_io_mode"):
            missing = dict(stream)
            missing.pop(field)
            result = self.run_gate([missing], [missing], "--strict-provenance", "--allow-missing-provenance")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(field, result.stderr)

    def test_workload_profile_is_compared_but_is_not_a_build_profile_alias(self):
        record = strict_row(profile="release")
        record.pop("build_profile")
        result = self.run_gate([record], [record], "--strict-provenance")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("build_profile", result.stderr)
        mismatch = self.run_gate([strict_row(profile="fast")], [strict_row(profile="extended")], "--strict-provenance", "--allow-missing-provenance")
        self.assertEqual(mismatch.returncode, 1)

    def test_strict_does_not_require_equal_revision_iterations_or_seed(self):
        baseline = strict_row(git_revision="old", iterations=5, seed=1)
        current = strict_row(git_revision="new", iterations=10, seed=2)
        result = self.run_gate([baseline], [current], "--strict-provenance")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
