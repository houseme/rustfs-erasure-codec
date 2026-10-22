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
TOOLS="${OUT_DIR}/rse-tools"

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
# Freeze one executable before switching revisions. Old revisions do not need
# xtask sources or its dependencies, and no Python helper is copied or invoked.
TOOL_HOST="$(rustc -vV | sed -n 's/^host: //p')"
cargo build --locked --release --package xtask --target "${TOOL_HOST}" --target-dir target/rse-tools
cp "target/rse-tools/${TOOL_HOST}/release/xtask" "${TOOLS}"
"${TOOLS}" abba-metadata --output "${META_JSON}" \
  --baseline-ref "${BASELINE_REF}" --candidate-ref "${CANDIDATE_REF}" \
  --backend-override "${BACKEND}" --features "${FEATURES}" \
  --sample-size "${SAMPLE_SIZE}" --warmup-time "${WARMUP_TIME}" \
  --measurement-time "${MEASUREMENT_TIME}" --cooldown-seconds "${COOLDOWN_SECONDS}" \
  --drift-threshold "${DRIFT_THRESHOLD}" --cpuset "${CPUSET}"

run_stage() {
  local stage="$1"
  local ref="$2"
  local write_header="$3"
  local commit
  local stage_start
  git switch --quiet --detach "${ref}"
  commit="$(git rev-parse --short HEAD)"
  echo "==> ${stage} ${commit} backend=${BACKEND}"
  stage_start="$("${TOOLS}" epoch)"

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

  if [[ "${write_header}" == "yes" ]]; then
    "${TOOLS}" extract-criterion \
      --stage "${stage}" \
      --commit "${commit}" \
      --backend-override "${BACKEND}" \
      --csv "${CSV_PATH}" \
      --jsonl "${JSONL_PATH}" \
      --updated-after-epoch "${stage_start}" \
      --write-header
  else
    "${TOOLS}" extract-criterion \
      --stage "${stage}" \
      --commit "${commit}" \
      --backend-override "${BACKEND}" \
      --csv "${CSV_PATH}" \
      --jsonl "${JSONL_PATH}" \
      --updated-after-epoch "${stage_start}"
  fi
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

"${TOOLS}" analyze-abba \
  --csv "${CSV_PATH}" \
  --summary-json "${SUMMARY_JSON}" \
  --summary-md "${SUMMARY_MD}" \
  --drift-threshold "${DRIFT_THRESHOLD}"

echo "saved: ${OUT_DIR}"
