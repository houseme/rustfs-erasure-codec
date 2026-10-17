"""Exercise shell control flow with command stubs; no compilation or timing claims."""
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "release-check.sh"
BASELINES = ("RSE_SMOKE_BASELINE", "RSE_SMALL_FILE_BASELINE", "RSE_RECONSTRUCTION_HOTSPOT_BASELINE", "RSE_STREAM_PATH_BASELINE")
SWITCHES = ("RUN_BACKEND_CONSISTENCY", "RUN_SMALL_FILE_GATE", "RUN_RECONSTRUCTION_HOTSPOT_GATE", "RUN_STREAM_PATH_GATE", "RUN_SIMD_ACCEL_TESTS", "RUN_X86_SIMD_OVERRIDE_MATRIX", "RUN_AARCH64_SIMD_OVERRIDE_MATRIX")


class ReleaseCheckTests(unittest.TestCase):
    def run_checks(self, profile, baselines=True, invalid=None, **changes):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            binary = root / "bin"
            binary.mkdir()
            trace = root / "trace"
            stub = '#!/bin/sh\nprintf "%s|profile=%s|simd=%s|x86=%s|arm=%s\\n" "$0 $*" "${RSE_SMOKE_PROFILE:-}" "${RUN_SIMD_ACCEL_TESTS:-}" "${RUN_X86_SIMD_OVERRIDE_MATRIX:-}" "${RUN_AARCH64_SIMD_OVERRIDE_MATRIX:-}" >> "$TRACE"\ncase "$*" in *"${FAIL_ON:-__never__}"*) exit 42 ;; esac\n'
            for command in ("cargo", "python3", "bash"):
                path = binary / command
                path.write_text(stub)
                path.chmod(0o755)
            environment = dict(os.environ)
            for name in BASELINES + SWITCHES + ("RSE_BENCHMARK_ALLOW_MISSING_CASES", "FAIL_ON"):
                environment.pop(name, None)
            environment.update(PATH=str(binary) + os.pathsep + environment["PATH"], TRACE=str(trace), VALIDATION_PROFILE=profile)
            if baselines:
                for name in BASELINES:
                    path = root / (name + ".json")
                    path.write_text("[]\n")
                    environment[name] = str(path)
            if invalid:
                name, kind = invalid
                path = root / "bad"
                if kind == "directory":
                    path.mkdir()
                elif kind == "empty":
                    path.touch()
                environment[name] = str(path)
            environment.update(changes)
            result = subprocess.run(["/bin/bash", str(SCRIPT)], cwd=root, env=environment, capture_output=True, text=True, check=False)
            return result, trace.read_text() if trace.exists() else ""

    def test_release_missing_inputs_fail_before_commands(self):
        result, trace = self.run_checks("release", baselines=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("requires RSE_SMOKE_BASELINE", result.stdout)
        self.assertEqual(trace, "")

    def test_every_release_baseline_must_be_a_readable_nonempty_file(self):
        for name in BASELINES:
            for kind in ("missing", "directory", "empty"):
                with self.subTest(name=name, kind=kind):
                    result, trace = self.run_checks("release", invalid=(name, kind))
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(name, result.stderr)
                    self.assertEqual(trace, "")

    def test_invalid_profile_and_partial_release_fail_before_commands(self):
        for profile, changes in (("typo", {}), ("release", {"RSE_BENCHMARK_ALLOW_MISSING_CASES": "1"})):
            result, trace = self.run_checks(profile, **changes)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(trace, "")

    def test_release_cannot_disable_required_gates(self):
        result, trace = self.run_checks("release", **dict.fromkeys(SWITCHES, "0"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("check_backend_consistency.sh", trace)
        self.assertIn("check_reconstruction_hotspot_gate.py", trace)
        self.assertIn("small-file-results.json", trace)
        self.assertIn("stream-path-results.json", trace)
        self.assertIn("smoke-results.json", trace)
        self.assertIn("profile=extended|simd=1|x86=1|arm=1", trace)
        smoke = next(line for line in trace.splitlines() if "benchmark_smoke_matrix_runs_and_exports_results" in line)
        self.assertIn("--release --features std simd-accel", smoke)
        golden = next(line for line in trace.splitlines() if "--test golden_vectors" in line)
        self.assertNotIn("--ignored", golden)

    def test_fast_profile_keeps_quick_optional_checks(self):
        result, trace = self.run_checks("fast", baselines=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("profile=quick", trace)
        self.assertNotIn("check_benchmark_regression.py", trace)

    def test_failed_gate_stops_the_pipeline(self):
        result, trace = self.run_checks("release", FAIL_ON="check_benchmark_regression.py")
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("check_backend_consistency.sh", trace)


if __name__ == "__main__":
    unittest.main()
