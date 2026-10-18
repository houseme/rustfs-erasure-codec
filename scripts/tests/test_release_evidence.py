"""Evidence contract tests use synthetic fixtures, never performance acceptance."""
import copy
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as evidence


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.manifest = self.root / "manifest.json"
        self.sources = {
            "baseline": {"commit": evidence.BASELINE_COMMIT, "tree": "a" * 40, "lock_sha256": "a" * 64},
            "candidate": {"commit": "b" * 40, "tree": "c" * 40, "lock_sha256": "b" * 64},
        }
        context = {"compiler": "rustc fixture", "cpu_model": "fixture CPU", "settings": evidence.BASE_ENV,
                   "isolation": {"scope": "exclusive-vm-cpu-partition", "reserved_cpu_count": 8, "physical_host_exclusivity_proven": False}}
        self.doc = {"schema_version": 1, "baseline_release": "9.0.0", "phase_order": list(evidence.PHASES),
                    "sources": self.sources, "harness_sha256": "d" * 64,
                    "contexts": {p: copy.deepcopy(context) for p in (*evidence.PHASES, "after")}, "files": {}}
        for phase in evidence.PHASES:
            (self.root / phase).mkdir()
            self.doc["files"][phase] = {}
            for profile in evidence.FILES:
                self.write_rows(phase, profile, self.fixture_rows(phase, profile))
        for name, value in (("source_info", self.sources["candidate"]), ("source_at", self.sources["baseline"]),
                            ("harness_digest", "d" * 64), ("command", "rustc fixture")):
            patch = mock.patch.object(evidence, name, return_value=value)
            patch.start()
            self.addCleanup(patch.stop)

    def fixture_rows(self, phase, profile):
        result = []
        revision = self.sources["baseline" if phase.startswith("A") else "candidate"]["commit"]
        for key in sorted(evidence.expected_cases(profile)):
            row = {"schema_version": 1, "artifact_kind": evidence.KINDS[profile], "benchmark_metrics_enabled": False}
            if profile == "hotspot":
                row.update(scenario=key, data_shards=10, parity_shards=4, shard_size=1048576, useful_shards=1,
                           missing_pattern="0", required_pattern="0", baseline_operation="reconstruct", candidate_operation="reconstruct_data",
                           baseline_mb_s=100.0, candidate_mb_s=100.0, speedup=1.0)
            else:
                operation, data, parity, size, *block = key.split(":")
                row.update(operation=operation, data_shards=int(data), parity_shards=int(parity), shard_size=int(size),
                           throughput_mb_s=100.0, ns_per_iter=100.0, git_revision=revision[:12],
                           features="std|simd-accel", target_triple="x86_64-linux-unknown", backend="fixture",
                           backend_id="fixture", backend_kind="fixture", backend_override="auto", override_honored=True,
                           profile="extended" if profile == "smoke" else "fast")
                if block:
                    row.update(stream_block_size=int(block[0]), stream_io_backend="fixture", stream_io_mode="auto", ns_per_block=100.0)
            result.append(row)
        return result

    def write_rows(self, phase, profile, rows):
        name = f"{phase}/{evidence.FILES[profile]}"
        path = self.root / name
        path.write_text(json.dumps(rows) + "\n")
        self.doc["files"][phase][profile] = {"path": name, "sha256": evidence.digest(path.read_bytes())}

    def verify(self):
        self.manifest.write_text(json.dumps(self.doc))
        return evidence.verify(self.manifest, self.root)

    def test_complete_campaign_runs_existing_regression_gates(self):
        _, _, report = self.verify()
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["case_counts"], {"smoke": 15, "small_file": 88, "hotspot": 8, "stream": 15})
        self.assertEqual(report["drift_checks"], 252)

    def test_wrong_source_lock_harness_baseline_and_self_comparison(self):
        original = copy.deepcopy(self.doc)
        mutations = (("baseline", "commit", "f" * 40), ("candidate", "commit", evidence.BASELINE_COMMIT),
                     ("candidate", "tree", "f" * 40), ("candidate", "lock_sha256", "f" * 64),
                     ("baseline", "tree", "c" * 40))
        for role, key, value in mutations:
            with self.subTest(role=role, key=key):
                self.doc = copy.deepcopy(original)
                self.doc["sources"][role][key] = value
                with self.assertRaises(ValueError): self.verify()
        self.doc = copy.deepcopy(original)
        self.doc["harness_sha256"] = "f" * 64
        with self.assertRaises(ValueError): self.verify()

    def test_rebase_with_identical_tree_is_accepted(self):
        current = dict(self.sources["candidate"], commit="e" * 40)
        with mock.patch.object(evidence, "source_info", return_value=current), mock.patch.object(evidence.subprocess, "run") as gate:
            report = self.verify()[2]
        self.assertEqual(report["candidate_commit"], "e" * 40)
        self.assertEqual(report["measured_candidate_commit"], "b" * 40)
        self.assertEqual(gate.call_count, 16)

    def test_empty_duplicate_missing_extra_wrong_revision_and_nonfinite(self):
        for mode in ("empty", "duplicate", "missing", "extra", "revision", "nonfinite", "zero", "features"):
            with self.subTest(mode=mode):
                rows = self.fixture_rows("A1", "small_file")
                if mode == "empty": rows = []
                elif mode == "duplicate": rows.append(rows[0])
                elif mode == "missing": rows.pop()
                elif mode == "extra": rows.append(dict(rows[0], shard_size=3))
                elif mode == "revision": rows[0]["git_revision"] = "badface"
                elif mode == "nonfinite": rows[0]["ns_per_iter"] = float("nan")
                elif mode == "zero": rows[0]["ns_per_iter"] = 0
                else: rows[0]["features"] = "std"
                self.write_rows("A1", "small_file", rows)
                with self.assertRaises(ValueError): self.verify()

    def test_every_phase_and_profile_required(self):
        original = copy.deepcopy(self.doc)
        for phase in evidence.PHASES:
            for profile in evidence.FILES:
                self.doc = copy.deepcopy(original)
                del self.doc["files"][phase][profile]
                with self.assertRaises(ValueError): self.verify()

    def test_tampered_bytes_path_and_symlinks(self):
        self.doc["files"]["A1"]["smoke"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "digest"): self.verify()
        for name in ("../escape", "/tmp/file", "A1/../smoke-results.json", "unknown/smoke-results.json"):
            with self.assertRaises(ValueError): evidence.safe_file(self.root, name)
        target = self.root / "A1" / evidence.FILES["smoke"]
        target.unlink()
        target.symlink_to(self.root / "B1" / evidence.FILES["smoke"])
        with self.assertRaises(ValueError): evidence.safe_file(self.root, "A1/" + evidence.FILES["smoke"])

    def test_environment_changes_including_after_reject(self):
        original = copy.deepcopy(self.doc)
        for phase in (*evidence.PHASES[1:], "after"):
            for key in ("compiler", "cpu_model", "settings", "isolation"):
                self.doc = copy.deepcopy(original)
                self.doc["contexts"][phase][key] = "changed"
                with self.assertRaises(ValueError): self.verify()

    def test_either_side_drift_withholds_comparison(self):
        for phase in ("A2", "B2"):
            original = self.fixture_rows(phase, "smoke")
            rows = copy.deepcopy(original)
            rows[0]["throughput_mb_s"] *= 1.06
            self.write_rows(phase, "smoke", rows)
            with mock.patch.object(evidence.subprocess, "run") as gate:
                with self.assertRaisesRegex(ValueError, "drift"): self.verify()
                gate.assert_not_called()
            self.write_rows(phase, "smoke", original)

    def test_stable_regression_rejected_by_existing_gate(self):
        for phase in ("B1", "B2"):
            rows = self.fixture_rows(phase, "smoke")
            rows[0]["throughput_mb_s"] = 50.0
            self.write_rows(phase, "smoke", rows)
        with self.assertRaises(subprocess.CalledProcessError): self.verify()

    def test_materialize_uses_verified_raw_files(self):
        self.manifest.write_text(json.dumps(self.doc))
        with mock.patch.object(evidence.subprocess, "run"):
            evidence.materialize(self.manifest, self.root, self.root / "inputs.env")
        lines = dict(line.split("=", 1) for line in (self.root / "inputs.env").read_text().splitlines())
        self.assertEqual(set(lines), set(evidence.VARIABLES.values()))
        for profile, filename in evidence.FILES.items():
            self.assertEqual(pathlib.Path(lines[evidence.VARIABLES[profile]]).read_bytes(), (self.root / "A1" / filename).read_bytes())
            self.assertEqual((self.root / "target/benchmark-smoke" / filename).read_bytes(), (self.root / "B2" / filename).read_bytes())

    def test_capture_refuses_to_reuse_any_existing_output(self):
        report = self.root / "report.json"
        report.write_text("old observation")
        with self.assertRaisesRegex(ValueError, "new evidence directory"):
            evidence.capture(self.root, self.root, self.root, self.root)
        self.assertEqual(report.read_text(), "old observation")


if __name__ == "__main__": unittest.main()
