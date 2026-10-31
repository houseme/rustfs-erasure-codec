#!/usr/bin/env python3
"""Synthetic handshake probe; it never runs codec work or measures performance."""
import json
import os
from pathlib import Path
import sys

root = Path(os.environ["ROW_GATE_SYNTHETIC_ROOT"])
count = root / "invocations"
run = int(count.read_text()) + 1 if count.exists() else 1
count.write_text(str(run))
cases = [
    "retained_recovery_row/10x4/1048576/missing0",
    "retained_recovery_row/10x4/1048577/missing3",
    "retained_recovery_row/4x2/1048576/missing0",
]
for index, case in enumerate(cases):
    value = 100.0 if run == 1 else 106.0
    row = {"case_id": case, "jobs": 4, "available_parallelism": 5, "chunk_len": 95296}
    for stage in ("A1", "B1", "B2", "A2"):
        row[stage] = {"samples_ns": [value] * 3, "iterations": [10] * 3}
    print(json.dumps(row), flush=True)
    if sys.stdin.readline().strip() != "continue":
        sys.exit(2)
    if run == 2 and index == 0:
        (root / "invalid_ack").write_text("unexpected")
