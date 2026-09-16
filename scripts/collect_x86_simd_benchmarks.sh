#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found"
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 not found"
  exit 1
fi

DATE_UTC="$(date -u +%F)"
CPU_SLUG="${1:-}"
if [[ -z "${CPU_SLUG}" ]]; then
  CPU_SLUG="$(lscpu | awk -F: '/Model name:/ {gsub(/^ +/, "", $2); print $2; exit}' | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\+/-/g; s/^-//; s/-$//')"
fi

OUT_DIR="benchmarks/x86_64-simd"
OUT_JSON="${OUT_DIR}/${DATE_UTC}-${CPU_SLUG}.json"
RUN_META="${OUT_DIR}/${DATE_UTC}-${CPU_SLUG}.run-meta.json"

BACKENDS=(
  auto
  scalar
  rust-ssse3
  rust-avx2
  rust-avx512
  rust-gfni-avx2
  rust-gfni-avx512
)

mkdir -p "${OUT_DIR}"
mkdir -p target/benchmark-smoke
mkdir -p target/criterion

git_revision() {
  git rev-parse --short HEAD 2>/dev/null || echo "unknown"
}

feature_set() {
  echo "std|simd-accel"
}

write_run_meta() {
  RUN_META_PATH="${RUN_META}" \
  DATE_UTC_VALUE="${DATE_UTC}" \
  CPU_SLUG_VALUE="${CPU_SLUG}" \
  GIT_REVISION_VALUE="$(git_revision)" \
  FEATURE_SET_VALUE="$(feature_set)" \
  BACKENDS_VALUE="${BACKENDS[*]}" \
  python3 - <<'PY'
import json
import os
import pathlib
import platform
import subprocess

out = pathlib.Path(os.environ["RUN_META_PATH"])

def capture(cmd):
    try:
        return subprocess.check_output(cmd, text=True).strip()
    except Exception:
        return ""

payload = {
    "date_utc": os.environ["DATE_UTC_VALUE"],
    "machine_slug": os.environ["CPU_SLUG_VALUE"],
    "git_revision": os.environ["GIT_REVISION_VALUE"],
    "feature_set": os.environ["FEATURE_SET_VALUE"],
    "backends": os.environ["BACKENDS_VALUE"].split(),
    "hostname": platform.node(),
    "arch": platform.machine(),
    "platform": platform.platform(),
    "uname_a": capture(["uname", "-a"]),
    "lscpu": capture(["lscpu"]),
}
out.write_text(json.dumps(payload, indent=2))
print(out)
PY
}

run_smoke() {
  local backend="$1"
  local out_csv="target/benchmark-smoke/smoke-results-release-${backend}.csv"
  echo "==> release smoke: ${backend}"
  RSE_BACKEND_OVERRIDE="${backend}" \
  RSE_STRICT_BACKEND_OVERRIDE=1 \
    cargo test --release --features 'std simd-accel' --test benchmark_smoke \
      benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
  cp target/benchmark-smoke/smoke-results.csv "${out_csv}"
}

run_bench() {
  local backend="$1"
  echo "==> criterion bench: ${backend}"
  RSE_BACKEND_OVERRIDE="${backend}" \
    RSE_STRICT_BACKEND_OVERRIDE=1 \
    cargo bench --bench galois_backend --features 'std simd-accel' -- \
      --sample-size 10 --warm-up-time 1 --measurement-time 1
}

for backend in "${BACKENDS[@]}"; do
  run_bench "${backend}"
  run_smoke "${backend}"
done

echo "==> writing ${OUT_JSON}"
python3 scripts/summarize_x86_simd_benchmarks.py \
  --root "${ROOT_DIR}" \
  --machine-json "${OUT_JSON}" \
  --machine-slug "${CPU_SLUG}" \
  --date "${DATE_UTC}"

echo "==> writing ${RUN_META}"
write_run_meta >/dev/null

echo "saved: ${OUT_JSON}"
echo "saved: ${RUN_META}"
