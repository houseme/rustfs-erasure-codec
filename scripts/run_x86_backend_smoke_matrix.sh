#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

DATE_UTC="${1:-$(date -u +%F)}"
CPU_SLUG="${2:-$(lscpu | awk -F: '/Model name:/ {gsub(/^ +/,"",$2); print $2; exit}' | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\+/-/g; s/^-//; s/-$//')}"

# ── CPU feature detection ──────────────────────────────────────────
CPUFLAGS=$(grep -m1 '^flags' /proc/cpuinfo 2>/dev/null || echo "")

has_flag() {
  echo " ${CPUFLAGS}" | grep -qi " $1 "
}

BACKENDS=(auto scalar)

if has_flag ssse3; then
  BACKENDS+=(rust-ssse3)
fi
if has_flag avx2; then
  BACKENDS+=(rust-avx2)
fi
if has_flag avx512f && has_flag avx512bw; then
  BACKENDS+=(rust-avx512)
fi
if has_flag gfni && has_flag avx2; then
  BACKENDS+=(rust-gfni-avx2)
fi
if has_flag gfni && has_flag avx512f && has_flag avx512bw; then
  BACKENDS+=(rust-gfni-avx512)
fi

echo "==> Detected CPU flags: $(echo "${CPUFLAGS}" | tr ' ' '\n' | grep -iE 'sse|avx|gfni|neon' | tr '\n' ' ')"
echo "==> Backends to test: ${BACKENDS[*]}"

mkdir -p target/benchmark-smoke

for backend in "${BACKENDS[@]}"; do
  echo "==> smoke: ${backend}"
  RSE_BACKEND_OVERRIDE="${backend}" \
  RSE_STRICT_BACKEND_OVERRIDE=1 \
    cargo test --release --features 'std simd-accel' --test benchmark_smoke \
      benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
  cp target/benchmark-smoke/smoke-results.csv \
    "target/benchmark-smoke/smoke-results-release-${backend}.csv"
done

OUT_JSON="benchmarks/x86_64-simd/${DATE_UTC}-${CPU_SLUG}.json"
python3 scripts/summarize_x86_simd_benchmarks.py \
  --root "${ROOT_DIR}" \
  --machine-json "${OUT_JSON}" \
  --machine-slug "${CPU_SLUG}" \
  --date "${DATE_UTC}"

echo "saved: ${OUT_JSON}"
