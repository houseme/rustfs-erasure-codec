"""Full synthetic contracts exercise qualification refusal, never performance claims."""
from contextlib import ExitStack
import json
import pathlib
import subprocess
import unittest
from unittest import mock

import test_release_evidence as fixtures
import release_evidence as evidence
import release_sampling as sampling


class BaselineQualificationTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.EvidenceTests(methodName="runTest")
        self.fixture.setUp(); self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        self.baseline = self.root/"baseline"; (self.baseline/"tests").mkdir(parents=True)
        self.output = self.root/"qualification"
        self.stack = ExitStack(); self.addCleanup(self.stack.close)
        def patch(target, **kwargs): return self.stack.enter_context(mock.patch(target, **kwargs))
        self.source = self.fixture.sources["baseline"]
        self.info = patch("release_evidence.source_info", return_value=self.source)
        self.context = patch("release_evidence.environment_context", return_value=self.fixture.context)
        patch("release_evidence.sampling.render", return_value={"root.rs": "// entry", "stub.rs": "// module"})
        self.build = patch("release_evidence.subprocess.run")
        patch("release_evidence.time.sleep")
        self.run = patch("release_evidence.sampling.run_profile", side_effect=self.emit)
        self.mutate = lambda profile, mode, folder, rows: None

    def emit(self, root, env, profile, mode, path, count_file=None, **kwargs):
        phase = path.parent.name
        kinds = (["calibration_warmup"]*3+["calibration"]*9 if mode == "calibrate" else ["warmup"]*3+["sample"]*9)
        rows = [self.fixture.record(row, profile, "baseline", kind, index)
                for row in self.fixture.fixture_rows("A1", profile) for index, kind in enumerate(kinds)]
        if mode == "calibrate":
            for row in rows: row.update(calibration_batch=1, complete=row["index"] == 11)
        self.mutate(profile, mode, phase, rows)
        path.write_text("".join("RSE_TIMING "+json.dumps(row)+"\nRSE_SAMPLE "+json.dumps(row)+"\n" for row in rows))
        return rows

    def qualify(self): return evidence.qualify_baseline(self.baseline, self.output, self.root/"unused")

    def assert_no_success(self):
        self.assertFalse((self.output/"qualification.json").exists())
        self.assertFalse((self.baseline/"tests/rse_release_sampling.rs").exists())

    def test_complete_catalog_uses_formal_contract_and_never_approves_release(self):
        result = self.qualify()
        self.assertTrue(result["baseline_controls_passed"])
        self.assertFalse(result["release_approval"])
        self.assertIsNone(result["candidate_attribution"])
        self.assertEqual(len(result["repeat_checks"]), 126)
        self.assertEqual(self.run.call_count, 12)
        for phase in ("A1", "A2"):
            self.assertEqual(sum(v["case_count"] for v in result["phases"][phase].values()), 126)
            self.assertEqual(sum(len(v["service_times"]) for v in result["phases"][phase].values()), 126)
        self.assertFalse((self.output/"manifest.json").exists())
        self.assertFalse((self.baseline/"tests/rse_release_sampling.rs").exists())
        self.assertIn("CARGO_TARGET_DIR", self.build.call_args.kwargs["env"])

    def test_single_observation_contract_failure_stops_later_capture(self):
        for phase in ('calibration','A1','A2'):
            for field in ('backend_id','constructor_budget'):
                with self.subTest(phase=phase,field=field):
                    self.output=self.root/('single-row-'+phase+'-'+field);self.run.reset_mock()
                    def change(profile,mode,folder,rows):
                        if profile=='smoke' and folder==phase:
                            if field=='constructor_budget':rows[1]['placement'][field]=7
                            else:rows[1]['rows'][0][field]='changed'
                    self.mutate=change
                    with self.assertRaisesRegex(ValueError,'incompatible|observation placement'):self.qualify()
                    self.assertEqual(self.run.call_count,{'calibration':1,'A1':5,'A2':9}[phase])
                    self.assertTrue((self.output/phase/'smoke-samples.json').exists())
                    self.assert_no_success()

    def test_wide_distribution_retains_complete_first_phase_and_does_not_repeat(self):
        def change(profile, mode, phase, rows):
            if profile == "small_file" and phase == "A1":
                for row in rows:
                    if row["case"] == "encode:4:2:1024" and row["index"] >= 9:
                        row["rows"][0]["ns_per_iter"] *= 2; row["timed_ns"] *= 2
        self.mutate = change
        with self.assertRaisesRegex(evidence.EvidenceFailure, "distributions") as raised: self.qualify()
        self.assertEqual(set(raised.exception.details["phases"]["A1"]), set(evidence.FILES))
        self.assertEqual(self.run.call_count, 8)
        self.assertFalse((self.output/"A2").exists())
        self.assert_no_success()

    def test_repeat_drift_retains_all_126_checks_and_both_phases(self):
        def change(profile, mode, phase, rows):
            if profile == "small_file" and phase == "A2":
                for row in rows:
                    if row["case"] == "encode:4:2:1024":
                        row["rows"][0]["ns_per_iter"] *= 1.06; row["timed_ns"] *= 1.06
        self.mutate = change
        with self.assertRaisesRegex(evidence.EvidenceFailure, "drift") as raised: self.qualify()
        self.assertEqual(len(raised.exception.details["repeat_checks"]), 126)
        self.assertFalse(raised.exception.details["baseline_controls_passed"])
        self.assert_no_success()

    def test_exact_five_percent_boundary_is_inclusive(self):
        def change(profile, mode, phase, rows):
            if profile == "small_file" and phase == "A2":
                for row in rows:
                    if row["case"] == "encode:4:2:1024":
                        row["rows"][0]["ns_per_iter"] *= 1.05; row["timed_ns"] *= 1.05
        self.mutate = change
        result = self.qualify()
        self.assertTrue(result["baseline_controls_passed"])
        self.assertEqual(max(item["drift"] for item in result["repeat_checks"]), .05)

    def test_short_or_missing_samples_are_rejected_with_raw_rows_retained(self):
        for fault in ("short", "missing"):
            with self.subTest(fault=fault):
                self.output = self.root/("qualification-"+fault)
                def change(profile, mode, phase, rows):
                    if profile != "smoke" or phase != "A1": return
                    if fault == "short": rows[-1]["wall_ns"] = 1
                    else: rows.pop()
                self.mutate = change
                with self.assertRaises(ValueError): self.qualify()
                self.assertTrue((self.output/"A1/smoke-samples.json").exists())
                self.assert_no_success()

    def test_preparation_layout_and_rng_mismatch_are_not_silent(self):
        for field in ("fixture", "workload", "rng"):
            with self.subTest(field=field):
                output = self.output; self.output = output.with_name("qualification-"+field)
                def change(profile, mode, phase, rows):
                    if phase != "A1": return
                    if field == "fixture" and profile == "small_file":
                        for row in rows:
                            for position in row["fixture"]["source"]:
                                for name in ("offset_64", "offset_4096", "offset_page"): position[name] = 32
                    elif field == "workload" and profile == "smoke":
                        for row in rows: row["placement"]["constructor_budget"] = 7
                    elif field == "rng" and profile == "hotspot":
                        for row in rows:
                            for key in ("next_before", "next_after"): row["workload"]["rng"][key] = [x+1 for x in row["workload"]["rng"][key]]
                self.mutate = change
                with self.assertRaisesRegex(ValueError, "differs from calibration"): self.qualify()
                self.assert_no_success(); self.output = output

    def test_calibration_backend_and_io_modes_must_match_measurement(self):
        for field in ("backend", "features", "stream_io_mode"):
            with self.subTest(field=field):
                self.output = self.root/("qualification-"+field)
                def change(profile, mode, phase, rows):
                    if phase == "A1" and profile == "stream":
                        for row in rows: row["rows"][0][field] = "changed"
                self.mutate = change
                with self.assertRaises(ValueError): self.qualify()
                self.assert_no_success()

    def test_interrupted_worker_retains_journal_and_cleans_overlay(self):
        def fail(*args, **kwargs):
            self.emit(*args, **kwargs)
            raise subprocess.CalledProcessError(9, ["worker"])
        self.run.side_effect = fail
        with self.assertRaises(subprocess.CalledProcessError): self.qualify()
        self.assertTrue((self.output/"calibration/smoke-samples.partial.json").exists())
        self.assert_no_success()

    def test_context_change_keeps_completed_rows(self):
        def change(*args): self.context.return_value = {"changed": True}
        self.mutate = change
        with self.assertRaisesRegex(ValueError, "environment changed"): self.qualify()
        self.assertTrue((self.output/"calibration/smoke-samples.json").exists())
        self.assert_no_success()

    def test_cleanup_failure_cannot_write_success(self):
        close = evidence.overlay
        from contextlib import contextmanager
        @contextmanager
        def fail(*args):
            with close(*args): yield
            raise ValueError("cleanup failed")
        with mock.patch.object(evidence, "overlay", fail):
            with self.assertRaisesRegex(ValueError, "cleanup failed"): self.qualify()
        self.assert_no_success()

    def test_existing_output_is_never_overwritten(self):
        self.output.mkdir(); marker = self.output/"keep"; marker.write_text("original")
        with self.assertRaisesRegex(ValueError, "new qualification"): self.qualify()
        self.build.assert_not_called(); self.run.assert_not_called()
        self.assertEqual(marker.read_text(), "original")


if __name__ == "__main__": unittest.main()
