"""Behavioral checks for the diagnostic harness's drift gates."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest

path = Path(__file__).with_name("run_streaming.py")
spec = importlib.util.spec_from_file_location("row_streaming_gate", path)
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


def fixture():
    return {
        "case_id": "retained_recovery_row/10x4/1048576/missing0",
        "jobs": 4,
        "available_parallelism": 5,
        "chunk_len": 95296,
        **{name: {"samples_ns": [100.0] * 3, "iterations": [10] * 3} for name in gate.STAGES},
    }


class GateTests(unittest.TestCase):
    def test_stable_baseline_passes(self):
        self.assertEqual(gate.evaluate(fixture(), baseline=True), (0.0, 0.0, 0.0))

    def test_a2_and_b2_drift_are_rejected(self):
        for stage in ("A2", "B2"):
            row = fixture()
            row[stage]["samples_ns"] = [106.0] * 3
            with self.assertRaises(gate.GateError):
                gate.evaluate(row, baseline=False)

    def test_same_source_bias_is_checked_only_for_baseline(self):
        row = fixture()
        for stage in ("B1", "B2"):
            row[stage]["samples_ns"] = [106.0] * 3
        with self.assertRaises(gate.GateError):
            gate.evaluate(row, baseline=True)
        self.assertAlmostEqual(gate.evaluate(row, baseline=False)[2], 6.0)

    def test_boundary_is_inclusive(self):
        row = fixture()
        row["A2"]["samples_ns"] = [105.0] * 3
        gate.evaluate(row, baseline=False)

    def test_cross_round_drift_is_checked(self):
        self.assertAlmostEqual(gate.check_cross_round(100.0, 105.0), 5.0)
        for second in (106.0, 94.0, 0.0, float("nan"), float("inf")):
            with self.subTest(second=second):
                with self.assertRaises(gate.GateError):
                    gate.check_cross_round(100.0, second)

    def test_phase_level_uses_both_medians(self):
        row = fixture()
        row["A2"]["samples_ns"] = [98.0, 100.0, 102.0]
        self.assertEqual(gate.phase_level(row, "A1", "A2"), 100.0)

    def test_recorded_cross_round_shift_is_rejected(self):
        root = Path(__file__).resolve().parent.parent
        cases = [
            (root / "2026-10-31-freeze-7587132" / "sched-diagnostic", "retained_recovery_row/4x2/1048576/missing0"),
            (Path(__file__).resolve().parent / "paired-aa-results", "retained_recovery_row/10x4/1048577/missing3"),
        ]
        for directory, case in cases:
            rounds = []
            for number in (1, 2):
                rows = [json.loads(line) for line in (directory / f"baseline-{number}.jsonl").read_text().splitlines()]
                rounds.append(next(row for row in rows if row["case_id"] == case))
            for first, second in (("A1", "A2"), ("B1", "B2")):
                with self.subTest(case=case, phase=first[0]):
                    with self.assertRaises(gate.GateError):
                        gate.check_cross_round(
                            gate.phase_level(rounds[0], first, second),
                            gate.phase_level(rounds[1], first, second),
                        )

    def test_malformed_values_fail_closed(self):
        variants = []
        row = fixture(); row["A1"]["samples_ns"] = [float("nan")] * 3; variants.append(row)
        row = fixture(); row["B1"]["samples_ns"] = [float("inf")] * 3; variants.append(row)
        row = fixture(); row["B2"]["samples_ns"] = [0.0] * 3; variants.append(row)
        row = fixture(); row["A1"]["samples_ns"] = [10**400] * 3; variants.append(row)
        row = fixture(); row["A1"]["iterations"] = [10, 0, 10]; variants.append(row)
        row = fixture(); row["jobs"] = True; variants.append(row)
        row = fixture(); row["A1"] = None; variants.append(row)
        row = fixture(); row["case_id"] = "unknown"; variants.append(row)
        for row in variants:
            with self.subTest(row=copy.deepcopy(row)):
                with self.assertRaises(gate.GateError):
                    gate.evaluate(row, baseline=True)

    def test_stream_stops_reading_after_failed_case(self):
        read_after_failure = []
        row = fixture()
        row["A2"]["samples_ns"] = [106.0] * 3

        def lines():
            yield json.dumps(row)
            read_after_failure.append(True)
            yield json.dumps(fixture())

        with self.assertRaises(gate.GateError):
            list(gate.validated_rows(lines(), baseline=False, resource_invalid=lambda: False))
        self.assertEqual(read_after_failure, [])

    def test_stream_rejects_resource_change_and_missing_cases(self):
        with self.assertRaises(gate.GateError):
            list(gate.validated_rows([json.dumps(fixture())], baseline=True, resource_invalid=lambda: True))
        with self.assertRaises(gate.GateError):
            list(gate.validated_rows([json.dumps(fixture())], baseline=True, resource_invalid=lambda: False))

    def test_resource_change_takes_precedence_over_missing_case_at_eof(self):
        with self.assertRaisesRegex(gate.GateError, "resource monitor"):
            list(gate.validated_rows([], baseline=True, resource_invalid=lambda: True))

    def test_sustained_off_target_load_increments_and_clears_streak(self):
        before = {f"cpu{i}": [0] * 8 for i in range(16)}
        idle = {f"cpu{i}": [0, 0, 0, 20, 0, 0, 0, 0] for i in range(16)}
        loaded = copy.deepcopy(idle)
        loaded["cpu2"] = [20, 0, 0, 0, 0, 0, 0, 0]
        ratio, streak = gate.off_target_busy(before, loaded, 0)
        self.assertAlmostEqual(ratio, 0.1)
        self.assertEqual(streak, 1)
        _, streak = gate.off_target_busy(before, loaded, streak)
        _, streak = gate.off_target_busy(before, loaded, streak)
        self.assertEqual(streak, 3)
        _, streak = gate.off_target_busy(before, idle, streak)
        self.assertEqual(streak, 0)

    def test_invalid_cpu_counters_fail_closed(self):
        before = {f"cpu{i}": [10] * 8 for i in range(16)}
        after = copy.deepcopy(before)
        after["cpu2"][0] = 9
        with self.assertRaises(gate.GateError):
            gate.off_target_busy(before, after, 0)
        del after["cpu2"]
        with self.assertRaises(gate.GateError):
            gate.off_target_busy(before, after, 0)

    def test_recorded_nonexclusive_window_is_rejected(self):
        monitor = Path(__file__).with_name("streaming-aa-results") / "monitor.jsonl"
        rows = [json.loads(line) for line in monitor.read_text().splitlines()]

        def counters(row):
            return {
                line.split()[0]: [int(value) for value in line.split()[1:9]]
                for line in row["cpu_all"]
            }

        streak = 0
        for previous, current in zip(rows, rows[1:]):
            _, streak = gate.off_target_busy(counters(previous), counters(current), streak)
            if streak >= 3:
                break
        self.assertGreaterEqual(streak, 3)

    def test_stop_process_terminates_only_the_supplied_child(self):
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        try:
            gate.stop_process(child)
            self.assertIsNotNone(child.poll())
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()

    def test_failed_case_is_not_acknowledged(self):
        row = fixture()
        row["A2"]["samples_ns"] = [106.0] * 3
        script = (
            "import sys; print(sys.argv[1], flush=True); "
            "ack=sys.stdin.readline(); print('NEXT_STARTED', flush=True)"
        )
        child = subprocess.Popen(
            [sys.executable, "-u", "-c", script, json.dumps(row)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        accepted = []
        try:
            with self.assertRaises(gate.GateError):
                gate.process_cases(
                    child, io.StringIO(), baseline=False,
                    resource_invalid=lambda: False,
                    accept=lambda *args: accepted.append(args),
                )
            gate.stop_process(child)
            self.assertEqual(child.stdout.read(), "")
            self.assertEqual(accepted, [])
        finally:
            gate.stop_process(child)
            child.stdin.close()
            child.stdout.close()

    def test_cross_round_failure_is_not_acknowledged(self):
        script = (
            "import sys; print(sys.argv[1], flush=True); "
            "ack=sys.stdin.readline(); print('NEXT_STARTED', flush=True)"
        )
        child = subprocess.Popen(
            [sys.executable, "-u", "-c", script, json.dumps(fixture())],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        try:
            with self.assertRaises(gate.GateError):
                gate.process_cases(
                    child, io.StringIO(), baseline=True,
                    resource_invalid=lambda: False,
                    accept=lambda *_: gate.check_cross_round(100.0, 106.0),
                )
            gate.stop_process(child)
            self.assertEqual(child.stdout.read(), "")
        finally:
            gate.stop_process(child)
            child.stdin.close()
            child.stdout.close()

    def test_valid_cases_are_acknowledged(self):
        rows = []
        for case in sorted(gate.EXPECTED_CASES):
            row = fixture()
            row["case_id"] = case
            rows.append(row)
        script = (
            "import json,sys\n"
            "for row in json.loads(sys.argv[1]):\n"
            " print(json.dumps(row), flush=True)\n"
            " if sys.stdin.readline().strip() != 'continue': sys.exit(3)\n"
        )
        child = subprocess.Popen(
            [sys.executable, "-u", "-c", script, json.dumps(rows)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        accepted = []
        try:
            gate.process_cases(
                child, io.StringIO(), baseline=True,
                resource_invalid=lambda: False,
                accept=lambda row, *_: accepted.append(row["case_id"]),
            )
            self.assertEqual(child.wait(), 0)
            self.assertEqual(set(accepted), gate.EXPECTED_CASES)
        finally:
            gate.stop_process(child)
            child.stdin.close()
            child.stdout.close()

    def test_resource_change_stops_before_acknowledging_next_case(self):
        rows = []
        for case in sorted(gate.EXPECTED_CASES):
            row = fixture()
            row["case_id"] = case
            rows.append(row)
        script = (
            "import json,sys\n"
            "for row in json.loads(sys.argv[1]):\n"
            " print(json.dumps(row), flush=True)\n"
            " if sys.stdin.readline().strip() != 'continue': sys.exit(3)\n"
        )
        child = subprocess.Popen(
            [sys.executable, "-u", "-c", script, json.dumps(rows)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        accepted = []

        def accept(row, *_):
            accepted.append(row["case_id"])

        try:
            with self.assertRaises(gate.GateError):
                gate.process_cases(
                    child, io.StringIO(), baseline=True,
                    resource_invalid=lambda: bool(accepted), accept=accept,
                )
            self.assertEqual(len(accepted), 1)
        finally:
            gate.stop_process(child)
            child.stdin.close()
            child.stdout.close()


if __name__ == "__main__":
    unittest.main()
