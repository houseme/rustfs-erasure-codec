#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <baseline-ref> <candidate-ref>" >&2
  exit 2
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BASELINE_REF="$1"
CANDIDATE_REF="$2"
BACKEND="${RSE_ABBA_BACKEND:-auto}"
FEATURES="${RSE_ABBA_FEATURES:-std simd-accel}"
SAMPLE_SIZE="${RSE_ABBA_SAMPLE_SIZE:-20}"
WARMUP_TIME="${RSE_ABBA_WARMUP_TIME:-2}"
MEASUREMENT_TIME="${RSE_ABBA_MEASUREMENT_TIME:-2}"
COOLDOWN_SECONDS="${RSE_ABBA_COOLDOWN_SECONDS:-20}"
DRIFT_THRESHOLD="${RSE_ABBA_DRIFT_THRESHOLD:-0.05}"
CPUSET="${RSE_ABBA_CPUSET:-}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BASELINE_SHORT="$(git rev-parse --short "${BASELINE_REF}")"
CANDIDATE_SHORT="$(git rev-parse --short "${CANDIDATE_REF}")"
OUT_DIR="${RSE_ABBA_OUT_DIR:-target/benchmark-smoke/galois-backend-abba-${STAMP}-${BACKEND}-${BASELINE_SHORT}-${CANDIDATE_SHORT}}"
CSV_PATH="${OUT_DIR}/estimates.csv"
JSONL_PATH="${OUT_DIR}/estimates.jsonl"
SUMMARY_JSON="${OUT_DIR}/summary.json"
SUMMARY_MD="${OUT_DIR}/summary.md"
META_JSON="${OUT_DIR}/run-meta.json"
EXTRACT_HELPER="${OUT_DIR}/extract_galois_backend_criterion.py"
ANALYZE_HELPER="${OUT_DIR}/analyze_galois_backend_abba.py"

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "working tree must be clean before ABBA switching" >&2
  exit 1
fi

ORIGINAL_BRANCH="$(git symbolic-ref --quiet --short HEAD || true)"
ORIGINAL_HEAD="$(git rev-parse HEAD)"
cleanup() {
  if [[ -n "${ORIGINAL_BRANCH}" ]]; then
    git switch --quiet "${ORIGINAL_BRANCH}" || git switch --quiet --detach "${ORIGINAL_HEAD}"
  else
    git switch --quiet --detach "${ORIGINAL_HEAD}"
  fi
}
trap cleanup EXIT

mkdir -p "${OUT_DIR}"
cp scripts/extract_galois_backend_criterion.py "${EXTRACT_HELPER}"
cp scripts/analyze_galois_backend_abba.py "${ANALYZE_HELPER}"

python3 - "${META_JSON}" \
  "${BASELINE_REF}" \
  "${CANDIDATE_REF}" \
  "${BACKEND}" \
  "${FEATURES}" \
  "${SAMPLE_SIZE}" \
  "${WARMUP_TIME}" \
  "${MEASUREMENT_TIME}" \
  "${COOLDOWN_SECONDS}" \
  "${DRIFT_THRESHOLD}" \
  "${CPUSET}" <<'PY'
import json
import os
import pathlib
import platform
import subprocess
import sys

(
    out,
    baseline,
    candidate,
    backend,
    features,
    sample_size,
    warmup_time,
    measurement_time,
    cooldown_seconds,
    drift_threshold,
    cpuset,
) = sys.argv[1:]

def capture(cmd):
    try:
        return subprocess.check_output(cmd, text=True, stderr=subprocess.STDOUT).strip()
    except Exception as exc:
        return f"<unavailable: {exc}>"

payload = {
    "baseline_ref": baseline,
    "candidate_ref": candidate,
    "backend_override": backend,
    "features": features,
    "sample_size": sample_size,
    "warmup_time": warmup_time,
    "measurement_time": measurement_time,
    "cooldown_seconds": cooldown_seconds,
    "drift_threshold": drift_threshold,
    "cpuset": cpuset,
    "hostname": platform.node(),
    "machine": platform.machine(),
    "platform": platform.platform(),
    "uname": capture(["uname", "-a"]),
    "lscpu": capture(["lscpu"]),
    "rustc": capture(["rustc", "--version", "--verbose"]),
    "cargo": capture(["cargo", "--version"]),
}
pathlib.Path(out).write_text(json.dumps(payload, indent=2, sort_keys=True))
PY

run_stage() {
  local stage="$1"
  local ref="$2"
  local write_header="$3"
  local commit
  local stage_start
  git switch --quiet --detach "${ref}"
  commit="$(git rev-parse --short HEAD)"
  echo "==> ${stage} ${commit} backend=${BACKEND}"
  stage_start="$(python3 -c 'import time; print(time.time())')"

  local -a command=(
    cargo bench --bench galois_backend --features "${FEATURES}" --
    --sample-size "${SAMPLE_SIZE}"
    --warm-up-time "${WARMUP_TIME}"
    --measurement-time "${MEASUREMENT_TIME}"
  )
  if [[ -n "${CPUSET}" ]]; then
    command=(taskset -c "${CPUSET}" "${command[@]}")
  fi

  RSE_BACKEND_OVERRIDE="${BACKEND}" \
  RSE_STRICT_BACKEND_OVERRIDE=1 \
    "${command[@]}" 2>&1 | tee "${OUT_DIR}/${stage}-${commit}.log"

  local header_args=()
  if [[ "${write_header}" == "yes" ]]; then
    header_args=(--write-header)
  fi
  python3 "${EXTRACT_HELPER}" \
    --stage "${stage}" \
    --commit "${commit}" \
    --backend-override "${BACKEND}" \
    --csv "${CSV_PATH}" \
    --jsonl "${JSONL_PATH}" \
    --updated-after-epoch "${stage_start}" \
    "${header_args[@]}"
}

cooldown() {
  if [[ "${COOLDOWN_SECONDS}" != "0" ]]; then
    echo "==> cooldown ${COOLDOWN_SECONDS}s"
    sleep "${COOLDOWN_SECONDS}"
  fi
}

run_stage A1 "${BASELINE_REF}" yes
cooldown
run_stage B1 "${CANDIDATE_REF}" no
cooldown
run_stage B2 "${CANDIDATE_REF}" no
cooldown
run_stage A2 "${BASELINE_REF}" no

python3 "${ANALYZE_HELPER}" \
  --csv "${CSV_PATH}" \
  --summary-json "${SUMMARY_JSON}" \
  --summary-md "${SUMMARY_MD}" \
  --drift-threshold "${DRIFT_THRESHOLD}"

echo "saved: ${OUT_DIR}"
