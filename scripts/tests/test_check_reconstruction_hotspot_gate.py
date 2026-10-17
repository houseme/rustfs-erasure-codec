import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_reconstruction_hotspot_gate.py"


def row(**changes):
    result = {"scenario": "recover", "candidate_mb_s": 100, "baseline_mb_s": 80, "speedup": 1.25}
    result.update(changes)
    return result


class HotspotGateTests(unittest.TestCase):
    def run_gate(self, baseline, current, *arguments):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "a.json").write_text(json.dumps(baseline))
            (root / "b.json").write_text(json.dumps(current))
            return subprocess.run(
                [sys.executable, str(SCRIPT), "--baseline", str(root / "a.json"),
                 "--current", str(root / "b.json"), *arguments],
                capture_output=True, text=True, check=False,
            )

    def test_default_compares_all_baseline_scenarios(self):
        result = self.run_gate([row()], [row()])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("recover", result.stdout)
        result = self.run_gate([row(), row(scenario="other")],
                               [row(), row(scenario="other", candidate_mb_s=50)],
                               "--require-scenario", "recover")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("other", result.stderr)

    def test_empty_malformed_and_duplicate_artifacts_fail(self):
        for bad in ([], {}, [None], [row(scenario="")], [row(), row()]):
            for baseline, current in ((bad, [row()]), ([row()], bad)):
                with self.subTest(bad=bad):
                    self.assertNotEqual(self.run_gate(baseline, current).returncode, 0)

    def test_dropped_and_required_scenarios_fail(self):
        self.assertNotEqual(self.run_gate([row(), row(scenario="other")], [row()]).returncode, 0)
        self.assertNotEqual(self.run_gate([row()], [row()], "--require-scenario", "absent").returncode, 0)

    def test_invalid_metrics_fail_on_both_sides(self):
        for value in (0, -1, float("nan"), float("inf"), -float("inf"), True, "nan", "bad", None):
            for baseline, current in (([row(candidate_mb_s=value)], [row()]),
                                      ([row()], [row(candidate_mb_s=value)])):
                with self.subTest(value=value):
                    self.assertNotEqual(self.run_gate(baseline, current).returncode, 0)
        missing = row()
        del missing["candidate_mb_s"]
        self.assertNotEqual(self.run_gate([missing], [row()]).returncode, 0)
        self.assertNotEqual(self.run_gate([row()], [row(), row(scenario="extra", candidate_mb_s=0)]).returncode, 0)

    def test_thresholds_and_speedup_floors_are_enforced(self):
        self.assertNotEqual(self.run_gate([row()], [row(candidate_mb_s=50)]).returncode, 0)
        self.assertEqual(self.run_gate([row()], [row(candidate_mb_s=90)],
                                      "--scenario-max-regression", "recover=0.2").returncode, 0)
        self.assertNotEqual(self.run_gate([row()], [row()], "--min-speedup", "recover=2").returncode, 0)
        self.assertNotEqual(self.run_gate([row()], [row(speedup="nan")], "--min-speedup", "recover=1").returncode, 0)

    def test_invalid_and_duplicate_limits_fail(self):
        for arguments in (("--max-regression", "nan"), ("--max-regression", "inf"),
                          ("--max-regression", "-1"), ("--max-regression", "1.1"),
                          ("--scenario-max-regression", "recover=nan"),
                          ("--scenario-max-regression", "recover=-1"),
                          ("--min-speedup", "recover=inf"), ("--min-speedup", "=1"),
                          ("--min-speedup", "recover=1", "--min-speedup", "recover=2")):
            with self.subTest(arguments=arguments):
                self.assertNotEqual(self.run_gate([row()], [row()], *arguments).returncode, 0)

    def test_available_metadata_and_workload_must_match(self):
        for field, before, after in (("backend", "rust-avx2", "scalar-rust"),
                                     ("features", "std", "none"), ("cpu_model", "CPU A", "CPU B"),
                                     ("data_shards", 4, 10), ("required_pattern", "data", "parity")):
            with self.subTest(field=field):
                self.assertNotEqual(self.run_gate([row(**{field: before})], [row(**{field: after})]).returncode, 0)
        self.assertNotEqual(self.run_gate([row(backend="rust-avx2")], [row()]).returncode, 0)
        result = self.run_gate([row(cpu_model="CPU A")], [row(cpu="CPU A")])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(self.run_gate([row()], [row(cpu="A", cpu_model="B")]).returncode, 0)
        self.assertNotEqual(self.run_gate([row(data_shards=1)], [row(data_shards=True)]).returncode, 0)
        self.assertNotEqual(self.run_gate([row()], [row(speedup="nan")]).returncode, 0)


if __name__ == "__main__":
    unittest.main()
