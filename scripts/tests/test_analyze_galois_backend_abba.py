import csv
import json
import pathlib
import random
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
ANALYZER = SCRIPTS / "analyze_galois_backend_abba.py"
EXTRACTOR = SCRIPTS / "extract_galois_backend_criterion.py"
FIELDS = [
    "stage", "commit", "backend_override", "criterion_group", "operation",
    "length_bytes", "mean_ns", "mean_lower_ns", "mean_upper_ns",
]
STAGES = ("A1", "B1", "B2", "A2")
GROUP = "galois_mul_slice_rust-neon_RustNeon_RustSimd_override_auto"


def campaign(values=(100, 90, 91, 101), *, operation="mul_slice", length=65536, group=None):
    group = group or f"galois_{operation}_rust-neon_RustNeon_RustSimd_override_auto"
    return [
        {
            "stage": stage,
            "commit": "aaaa111" if stage.startswith("A") else "bbbb222",
            "backend_override": "auto",
            "criterion_group": group,
            "operation": operation,
            "length_bytes": length,
            "mean_ns": value,
            "mean_lower_ns": value,
            "mean_upper_ns": value,
        }
        for stage, value in zip(STAGES, values)
    ]


class GaloisBackendAbbaTests(unittest.TestCase):
    def run_analyzer(self, rows=None, *, raw=None, fields=FIELDS, threshold=None):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            csv_path = root / "estimates.csv"
            json_path = root / "summary.json"
            md_path = root / "summary.md"
            if raw is not None:
                csv_path.write_text(raw)
            else:
                with csv_path.open("w", newline="") as handle:
                    writer = csv.DictWriter(handle, fieldnames=fields, extrasaction="ignore")
                    writer.writeheader()
                    writer.writerows(rows or [])
            # Failed input must replace old PASS reports, not merely exit early.
            json_path.write_text('{"drift_failed": false, "attribution_allowed": true}')
            md_path.write_text("stale PASS report")
            arguments = [
                sys.executable, str(ANALYZER), "--csv", str(csv_path),
                "--summary-json", str(json_path), "--summary-md", str(md_path),
            ]
            if threshold is not None:
                arguments.extend(["--drift-threshold", str(threshold)])
            result = subprocess.run(arguments, capture_output=True, text=True, timeout=20, check=False)
            payload = json.loads(json_path.read_text())
            markdown = md_path.read_text()
            return result, payload, markdown

    def assert_failed_campaign(self, result, payload, markdown):
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(payload["drift_failed"])
        self.assertFalse(payload["attribution_allowed"])
        self.assertTrue(all(case["candidate_delta_vs_a_mean"] is None for case in payload["cases"]))
        self.assertIn("WITHHELD for the entire campaign", markdown)
        self.assertIn("Drift gate: FAIL", markdown)
        self.assertNotIn("stale PASS", markdown)
        self.assertNotIn("Traceback", result.stderr)

    def test_complete_extractor_schema_passes(self):
        rows = campaign() + campaign(operation="mul_slice_xor", length=1048576)
        result, payload, markdown = self.run_analyzer(rows)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(payload["attribution_allowed"])
        self.assertFalse(payload["drift_failed"])
        self.assertEqual(payload["validation_errors"], [])
        self.assertEqual(len(payload["cases"]), 2)
        self.assertAlmostEqual(payload["cases"][0]["baseline_drift"], 0.01)
        self.assertAlmostEqual(payload["cases"][0]["candidate_drift"], 1 / 90)
        self.assertAlmostEqual(payload["cases"][0]["candidate_delta_vs_a_mean"], (90.5 - 100.5) / 100.5)
        self.assertIn("Candidate attribution: PERMITTED", markdown)

    def test_empty_and_header_only_campaigns_fail(self):
        for raw in ("", ",\n", "stage,commit,criterion_group,operation,length_bytes,mean_ns\n"):
            with self.subTest(raw=raw):
                result, payload, markdown = self.run_analyzer(raw=raw)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertTrue(payload["validation_errors"])
                self.assertEqual(payload["cases"], [])

    def test_every_case_requires_all_four_stages(self):
        for stage in STAGES:
            with self.subTest(stage=stage):
                rows = [row for row in campaign() if row["stage"] != stage]
                rows += campaign(operation="mul_slice_xor")
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertIn(f"missing {stage}", " ".join(payload["validation_errors"]))

    def test_duplicate_case_stage_fails_instead_of_averaging(self):
        for identical in (True, False):
            rows = campaign()
            duplicate = dict(rows[0])
            if not identical:
                duplicate["mean_ns"] = 10000
            rows.append(duplicate)
            result, payload, markdown = self.run_analyzer(rows)
            self.assert_failed_campaign(result, payload, markdown)
            self.assertIn("duplicate estimate", " ".join(payload["validation_errors"]))

    def test_distinct_complete_groups_are_never_averaged_together(self):
        other = "galois_mul_slice_scalar_Scalar_Scalar_override_auto"
        rows = campaign() + campaign((400, 200, 200, 400), group=other)
        result, payload, _ = self.run_analyzer(rows)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(payload["cases"]), 2)
        cases = {case["criterion_group"]: case for case in payload["cases"]}
        self.assertEqual(cases[other]["candidate_delta_vs_a_mean"], -0.5)
        self.assertEqual(cases[GROUP]["a1_mean_ns"], 100)

    def test_group_change_cannot_fill_a_missing_stage_in_another_group(self):
        rows = campaign()
        rows[1]["criterion_group"] = "galois_mul_slice_scalar_Scalar_Scalar_override_auto"
        result, payload, markdown = self.run_analyzer(rows)
        self.assert_failed_campaign(result, payload, markdown)
        self.assertIn("incomplete ABBA case", " ".join(payload["validation_errors"]))

    def test_means_must_be_finite_and_positive(self):
        for value in ("", "not-a-number", "nan", "NaN", "inf", "-inf", "1e999", 0, -1):
            with self.subTest(value=value):
                rows = campaign()
                rows[1]["mean_ns"] = value
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertIn("mean_ns must be finite and positive", " ".join(payload["validation_errors"]))

    def test_lengths_must_be_positive_integers(self):
        for value in ("", "nan", "inf", "1.0", "1e3", 0, -1):
            with self.subTest(value=value):
                rows = campaign()
                rows[0]["length_bytes"] = value
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertIn("length_bytes must be a positive integer", " ".join(payload["validation_errors"]))

    def test_unknown_stage_fails(self):
        for stage in ("", "A3", "a1", "stage"):
            rows = campaign()
            rows[0]["stage"] = stage
            result, payload, markdown = self.run_analyzer(rows)
            self.assert_failed_campaign(result, payload, markdown)
            self.assertIn("unknown stage", " ".join(payload["validation_errors"]))

    def test_revision_must_be_resolved_nonzero_hex(self):
        for revision in ("", "unknown", "none", "main", "origin/main", "HEAD", "abc", "0000000", "f" * 41, "f" * 63):
            with self.subTest(revision=revision):
                rows = campaign()
                rows[0]["commit"] = revision
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertIn("resolved nonzero hexadecimal revision", " ".join(payload["validation_errors"]))

    def test_revision_hex_case_and_sha256_are_normalized(self):
        rows = campaign()
        for row in rows:
            row["commit"] = ("A" if row["stage"].startswith("A") else "B") * 64
        rows[-1]["commit"] = "a" * 64
        result, payload, _ = self.run_analyzer(rows)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(payload["commits"]["A1"], "a" * 64)

    def test_revisions_must_match_within_each_stage(self):
        rows = campaign() + campaign(operation="mul_slice_xor")
        rows[4]["commit"] = "cccc333"
        result, payload, markdown = self.run_analyzer(rows)
        self.assert_failed_campaign(result, payload, markdown)
        self.assertIn("stage A1 contains inconsistent revisions", " ".join(payload["validation_errors"]))

    def test_repeated_a_and_b_revisions_must_match(self):
        for stage in ("A2", "B2"):
            rows = campaign()
            next(row for row in rows if row["stage"] == stage)["commit"] = "cccc333"
            result, payload, markdown = self.run_analyzer(rows)
            self.assert_failed_campaign(result, payload, markdown)
            self.assertIn("revision mismatch", " ".join(payload["validation_errors"]))

    def test_same_revision_a_and_b_is_a_valid_stability_campaign(self):
        rows = campaign((100, 100, 100, 100))
        for row in rows:
            row["commit"] = "aaaa111"
        random.Random(7).shuffle(rows)
        result, payload, _ = self.run_analyzer(rows)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(payload["cases"][0]["candidate_delta_vs_a_mean"], 0)

    def test_operation_and_group_provenance_must_agree(self):
        for group, operation in (("", "mul_slice"), ("unknown", "mul_slice"), ("galois_mul_slice_unknown", "mul_slice"), (GROUP, "mul_slice_xor"), (GROUP, "encode")):
            with self.subTest(group=group, operation=operation):
                rows = campaign()
                rows[0].update(criterion_group=group, operation=operation)
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)

    def test_provided_backend_override_cannot_conflict_with_group(self):
        rows = campaign()
        rows[1]["backend_override"] = "scalar"
        result, payload, markdown = self.run_analyzer(rows)
        self.assert_failed_campaign(result, payload, markdown)
        self.assertIn("conflicts with criterion_group", " ".join(payload["validation_errors"]))

    def test_provided_backend_override_cannot_be_missing_or_unknown(self):
        for value in ("", "unknown", "none", "n/a", "auto other"):
            rows = campaign()
            rows[1]["backend_override"] = value
            result, payload, markdown = self.run_analyzer(rows)
            self.assert_failed_campaign(result, payload, markdown)
            self.assertIn("backend_override column must be nonempty", " ".join(payload["validation_errors"]))

    def test_override_is_consistent_even_without_an_embedded_group_override(self):
        rows = campaign(group="galois_mul_slice_rust-neon_RustNeon_RustSimd")
        rows[1]["backend_override"] = "scalar"
        result, payload, markdown = self.run_analyzer(rows)
        self.assert_failed_campaign(result, payload, markdown)
        self.assertIn("inconsistent backend_override", " ".join(payload["validation_errors"]))

    def test_legacy_absent_override_column_uses_exact_group_provenance(self):
        fields = [field for field in FIELDS if field != "backend_override"]
        result, payload, _ = self.run_analyzer(campaign(), fields=fields)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(payload["attribution_allowed"])

    def test_embedded_empty_or_unknown_override_is_not_valid_provenance(self):
        for suffix in ("", "unknown", "auto_override_scalar"):
            rows = campaign(group=f"galois_mul_slice_rust-neon_RustNeon_RustSimd_override_{suffix}")
            result, payload, markdown = self.run_analyzer(rows, fields=[field for field in FIELDS if field != "backend_override"])
            self.assert_failed_campaign(result, payload, markdown)

    def test_a_or_b_drift_withholds_all_case_deltas_and_exits_nonzero(self):
        for values in ((100, 80, 80, 106), (100, 80, 85, 100), (100, 80, 74, 100), (100, 80, 80, 94)):
            with self.subTest(values=values):
                rows = campaign(values) + campaign((200, 100, 100, 200), operation="mul_slice_xor")
                result, payload, markdown = self.run_analyzer(rows)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertEqual(len(payload["cases"]), 2)
                self.assertEqual(payload["drift_failures"], 1)
                self.assertTrue(all(line.split("|")[-3].strip() == "not attributable" for line in markdown.splitlines() if line.startswith("| galois_")))

    def test_both_sides_can_fail_the_same_case(self):
        result, payload, markdown = self.run_analyzer(campaign((100, 100, 120, 120)))
        self.assert_failed_campaign(result, payload, markdown)
        self.assertEqual(payload["drift_failures"], 1)
        self.assertAlmostEqual(payload["cases"][0]["baseline_drift"], 0.2)
        self.assertAlmostEqual(payload["cases"][0]["candidate_drift"], 0.2)

    def test_exact_drift_boundary_and_explicit_threshold_are_preserved(self):
        result, payload, _ = self.run_analyzer(campaign((100, 100, 95, 105)))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(payload["drift_failed"])
        result, payload, _ = self.run_analyzer(campaign((100, 100, 110, 110)), threshold=0.1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(payload["drift_threshold"], 0.1)

    def test_no_unsolicited_candidate_regression_threshold(self):
        result, payload, _ = self.run_analyzer(campaign((100, 150, 150, 100)))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(payload["cases"][0]["candidate_delta_vs_a_mean"], 0.5)

    def test_invalid_threshold_replaces_old_reports_and_fails_closed(self):
        for threshold in ("nan", "inf", "-inf", "-0.01", "not-a-number"):
            with self.subTest(threshold=threshold):
                result, payload, markdown = self.run_analyzer(campaign(), threshold=threshold)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertIn("threshold must be finite and nonnegative", " ".join(payload["validation_errors"]))

    def test_zero_threshold_requires_identical_repeated_means(self):
        result, _, _ = self.run_analyzer(campaign((100, 90, 90, 100)), threshold=0)
        self.assertEqual(result.returncode, 0, result.stderr)
        result, payload, markdown = self.run_analyzer(campaign(), threshold=0)
        self.assert_failed_campaign(result, payload, markdown)

    def test_nonfinite_derived_ratios_never_leak_nan_or_infinity_to_json(self):
        for values in ((1e-308, 1e308, 1e308, 1e-308), (1e-308, 10, 10, 1e308)):
            result, payload, markdown = self.run_analyzer(campaign(values))
            self.assert_failed_campaign(result, payload, markdown)
            self.assertIn("derived ratio is not finite", " ".join(payload["validation_errors"]))
            json.dumps(payload, allow_nan=False)

    def test_malformed_csv_and_headers_fail_without_tracebacks(self):
        for raw in (
            "stage,commit,operation,length_bytes,mean_ns\n",
            "stage,stage,commit,criterion_group,operation,length_bytes,mean_ns\n",
            "stage,commit,criterion_group,operation,length_bytes,mean_ns\nA1,aaaa111,group,mul_slice,1,1,extra\n",
            'stage,commit,criterion_group,operation,length_bytes,mean_ns\n"unterminated\n',
        ):
            with self.subTest(raw=raw):
                result, payload, markdown = self.run_analyzer(raw=raw)
                self.assert_failed_campaign(result, payload, markdown)
                self.assertTrue(payload["validation_errors"])

    def test_current_extractor_output_is_accepted_without_translation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            estimate = root / "criterion" / GROUP / "len_65536" / "new" / "estimates.json"
            estimate.parent.mkdir(parents=True)
            csv_path = root / "estimates.csv"
            jsonl_path = root / "estimates.jsonl"
            for row in campaign():
                value = row["mean_ns"]
                estimate.write_text(json.dumps({"mean": {"point_estimate": value, "confidence_interval": {"lower_bound": value * 0.99, "upper_bound": value * 1.01}}}))
                command = [
                    sys.executable, str(EXTRACTOR), "--criterion-root", str(root / "criterion"),
                    "--stage", row["stage"], "--commit", row["commit"], "--backend-override", "auto",
                    "--csv", str(csv_path), "--jsonl", str(jsonl_path), "--updated-after-epoch", "0",
                ]
                if row["stage"] == "A1":
                    command.append("--write-header")
                result = subprocess.run(command, capture_output=True, text=True, timeout=20, check=False)
                self.assertEqual(result.returncode, 0, result.stderr)
            result, payload, _ = self.run_analyzer(raw=csv_path.read_text())
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(payload["cases"]), 1)


if __name__ == "__main__":
    unittest.main()
