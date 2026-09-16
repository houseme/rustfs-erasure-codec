#!/usr/bin/env bash
# run-full-benchmark.sh — Cross-platform full benchmark suite with cooldown
#
# Usage:
#   bash scripts/run-full-benchmark.sh                    # full extended profile
#   bash scripts/run-full-benchmark.sh --profile fast     # fast profile
#   bash scripts/run-full-benchmark.sh --phase small      # small files only
#   bash scripts/run-full-benchmark.sh --phase large      # large files only
#   bash scripts/run-full-benchmark.sh --cooldown 30      # 30s cooldown between phases
#   bash scripts/run-full-benchmark.sh --iterations 10    # 10 iterations per case
#
# Environment overrides:
#   RSE_SMALL_FILE_PROFILE    — quick | fast | extended (default: extended)
#   RSE_SMALL_FILE_ITERATIONS — iteration count (default: 5)
#   RSE_BENCH_FEATURES        — cargo features (default: "std simd-accel")
#   RSE_BENCH_COOLDOWN        — seconds between phases (default: 15)
#   RSE_BENCH_CASE_FILTER     — comma-separated case labels
#
# Output:
#   benchmarks/<arch>/<date>-<arch>-<profile>.csv
#   benchmarks/<arch>/<date>-<arch>-<profile>.json
#   benchmarks/<arch>/<date>-<arch>-hwinfo.txt

set -euo pipefail

# ── Argument parsing ──────────────────────────────────────────────

PROFILE="${RSE_SMALL_FILE_PROFILE:-extended}"
ITERATIONS="${RSE_SMALL_FILE_ITERATIONS:-5}"
FEATURES="${RSE_BENCH_FEATURES:-std simd-accel}"
COOLDOWN="${RSE_BENCH_COOLDOWN:-15}"
PHASE="all"
CASE_FILTER="${RSE_BENCH_CASE_FILTER:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)    PROFILE="$2"; shift 2 ;;
    --iterations) ITERATIONS="$2"; shift 2 ;;
    --cooldown)   COOLDOWN="$2"; shift 2 ;;
    --phase)      PHASE="$2"; shift 2 ;;
    --filter)     CASE_FILTER="$2"; shift 2 ;;
    --features)   FEATURES="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^$/{ s/^# \?//; p }' "$0"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ── Environment detection ─────────────────────────────────────────

ARCH=$(uname -m)
DATE=$(date -u '+%Y-%m-%d')
TIMESTAMP=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
GIT_REV=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")

# Normalize arch for filenames
case "$ARCH" in
  x86_64)  ARCH_TAG="x86_64-linux" ;;
  aarch64) ARCH_TAG="aarch64-linux" ;;
  arm64)   ARCH_TAG="aarch64-darwin" ;;
  *)       ARCH_TAG="$ARCH" ;;
esac

OUTDIR="benchmarks/${ARCH_TAG}"
mkdir -p "$OUTDIR"

BASENAME="${DATE}-${ARCH_TAG}-${PROFILE}"
HWINFO_FILE="${OUTDIR}/${BASENAME}-hwinfo.txt"
CSV_FILE="${OUTDIR}/${BASENAME}.csv"
JSON_FILE="${OUTDIR}/${BASENAME}.json"

# ── Hardware info collection ──────────────────────────────────────

collect_hwinfo() {
  {
    echo "=== Hardware Environment ==="
    echo "Timestamp: ${TIMESTAMP}"
    echo "Git revision: ${GIT_REV}"
    echo "Architecture: ${ARCH}"
    echo ""

    if command -v lscpu &>/dev/null; then
      echo "=== CPU ==="
      lscpu | grep -E 'Model name|Socket|Core|Thread|CPU MHz|CPU max|Cache|Architecture|Byte Order'
      echo ""
    elif [[ -f /proc/cpuinfo ]]; then
      echo "=== CPU ==="
      grep -m1 'model name' /proc/cpuinfo
      grep -m1 'cpu MHz' /proc/cpuinfo
      echo ""
    elif command -v sysctl &>/dev/null; then
      echo "=== CPU (macOS) ==="
      sysctl -n machdep.cpu.brand_string 2>/dev/null || true
      sysctl -n hw.ncpu 2>/dev/null | xargs -I{} echo "CPU count: {}"
      echo ""
    fi

    echo "=== SIMD Flags ==="
    if [[ -f /proc/cpuinfo ]]; then
      grep -m1 'flags\|Features' /proc/cpuinfo | tr ' ' '\n' | grep -iE 'avx|sse|gfni|neon|sve|aes|pmull' | sort || echo "N/A"
    elif command -v sysctl &>/dev/null; then
      sysctl -n hw.optional.arm64 2>/dev/null && echo "NEON: yes" || true
      sysctl -n hw.optional.neon 2>/dev/null && echo "NEON: yes" || true
    fi
    echo ""

    echo "=== Memory ==="
    free -h 2>/dev/null | head -2 || vm_stat 2>/dev/null | head -5 || echo "N/A"
    echo ""

    echo "=== Kernel ==="
    uname -r
    echo ""

    echo "=== OS ==="
    cat /etc/os-release 2>/dev/null | grep -E '^PRETTY_NAME|^NAME' | head -2 || sw_vers 2>/dev/null || echo "N/A"
    echo ""

    echo "=== Rust ==="
    rustc --version
    cargo --version
    echo ""

    echo "=== System Load ==="
    uptime
    echo ""

    echo "=== Benchmark Config ==="
    echo "Profile: ${PROFILE}"
    echo "Iterations: ${ITERATIONS}"
    echo "Features: ${FEATURES}"
    echo "Cooldown: ${COOLDOWN}s"
    echo "Phase: ${PHASE}"
    echo "Case filter: ${CASE_FILTER:-<all>}"
  } > "$HWINFO_FILE"

  echo "✓ Hardware info saved: ${HWINFO_FILE}"
}

# ── Cooldown helper ───────────────────────────────────────────────

cooldown() {
  local label="$1"
  local seconds="$2"
  echo ""
  echo "⏳ Cooldown: ${label} (${seconds}s)..."
  echo "   CPU freq before: $(grep -m1 'cpu MHz' /proc/cpuinfo 2>/dev/null || sysctl -n hw.cpufrequency 2>/dev/null || echo 'N/A')"
  sleep "$seconds"
  echo "   CPU freq after:  $(grep -m1 'cpu MHz' /proc/cpuinfo 2>/dev/null || sysctl -n hw.cpufrequency 2>/dev/null || echo 'N/A')"
  echo ""
}

# ── Small-file benchmark ──────────────────────────────────────────

run_small_files() {
  echo "━━━ Phase 1: Small-File Benchmark ━━━"
  echo "    profile: ${PROFILE}"
  echo "    iterations: ${ITERATIONS}"
  echo "    features: ${FEATURES}"

  local env_vars=(
    "RSE_SMALL_FILE_PROFILE=${PROFILE}"
    "RSE_SMALL_FILE_ITERATIONS=${ITERATIONS}"
  )
  if [[ -n "$CASE_FILTER" ]]; then
    env_vars+=("RSE_SMALL_FILE_CASE_FILTER=${CASE_FILTER}")
  fi

  env "${env_vars[@]}" \
    cargo test --release \
    --features "$FEATURES" \
    --test benchmark_small_files \
    benchmark_small_file_matrix_runs_and_exports_results \
    -- --ignored --nocapture

  echo "✓ Small-file benchmark complete"
}

# ── Large-file benchmark ──────────────────────────────────────────

run_large_files() {
  echo "━━━ Phase 2: Large-File Benchmark (isolated) ━━━"

  local large_cases="4x2_512k,4x2_1m,10x4_512k,10x4_1m"
  if [[ -n "$CASE_FILTER" ]]; then
    large_cases="$CASE_FILTER"
  fi

  RSE_SMALL_FILE_PROFILE=extended \
  RSE_SMALL_FILE_ITERATIONS="${ITERATIONS}" \
  RSE_SMALL_FILE_CASE_FILTER="${large_cases}" \
    cargo test --release \
    --features "$FEATURES" \
    --test benchmark_small_files \
    benchmark_small_file_matrix_runs_and_exports_results \
    -- --ignored --nocapture

  echo "✓ Large-file benchmark complete"
}

# ── Collect artifacts ─────────────────────────────────────────────

collect_artifacts() {
  local src_csv="target/benchmark-smoke/small-file-results.csv"
  local src_json="target/benchmark-smoke/small-file-results.json"

  if [[ -f "$src_csv" ]]; then
    cp "$src_csv" "$CSV_FILE"
    echo "✓ CSV saved: ${CSV_FILE}"
  fi

  if [[ -f "$src_json" ]]; then
    cp "$src_json" "$JSON_FILE"
    echo "✓ JSON saved: ${JSON_FILE}"
  fi
}

# ── Summary ───────────────────────────────────────────────────────

print_summary() {
  echo ""
  echo "━━━ Benchmark Summary ━━━"
  echo "  Architecture: ${ARCH_TAG}"
  echo "  Git revision: ${GIT_REV}"
  echo "  Profile:      ${PROFILE}"
  echo "  Iterations:   ${ITERATIONS}"
  echo "  Timestamp:    ${TIMESTAMP}"
  echo ""
  echo "  Artifacts:"
  echo "    ${HWINFO_FILE}"
  echo "    ${CSV_FILE}"
  echo "    ${JSON_FILE}"
  echo ""

  if [[ -f "$CSV_FILE" ]]; then
    local total_cases
    total_cases=$(tail -n +2 "$CSV_FILE" | wc -l | tr -d ' ')
    echo "  Total data points: ${total_cases}"
    echo ""
    echo "  reconstruct_opt vs reconstruct (sample):"
    printf "  %-12s %-28s %12s %12s\n" "Case" "Operation" "opt ns" "plain ns"
    echo "  ─────────────────────────────────────────────────────────────────"
    # Show first and last 4x2 case as sample
    for label in "4x2_1k" "4x2_1m"; do
      local opt_ns plain_ns
      opt_ns=$(grep "${label}," "$CSV_FILE" | grep ',reconstruct_opt,' | head -1 | cut -d',' -f22)
      plain_ns=$(grep "${label}," "$CSV_FILE" | grep ',reconstruct,' | grep -v '_opt\|_data\|_shard\|_some' | head -1 | cut -d',' -f22)
      if [[ -n "$opt_ns" && -n "$plain_ns" ]]; then
        local ratio
        ratio=$(echo "scale=2; ${opt_ns}/${plain_ns}" | bc)
        printf "  %-12s %-28s %12s %12s  (%s×)\n" "$label" "reconstruct_opt" "$opt_ns" "$plain_ns" "$ratio"
      fi
    done
  fi
}

# ── Main ──────────────────────────────────────────────────────────

main() {
  echo "━━━ EC Small-File Benchmark Suite ━━━"
  echo "  arch: ${ARCH_TAG}"
  echo "  git:  ${GIT_REV}"
  echo "  date: ${DATE}"
  echo ""

  collect_hwinfo

  case "$PHASE" in
    small)
      cooldown "pre-benchmark" "$COOLDOWN"
      run_small_files
      collect_artifacts
      ;;
    large)
      cooldown "pre-benchmark" "$COOLDOWN"
      run_large_files
      collect_artifacts
      ;;
    all)
      cooldown "pre-small-file-phase" "$COOLDOWN"
      run_small_files
      collect_artifacts

      local small_csv="$CSV_FILE"
      local small_json="$JSON_FILE"

      cooldown "pre-large-file-phase" "$COOLDOWN"
      run_large_files

      local large_csv="${OUTDIR}/${DATE}-${ARCH_TAG}-${PROFILE}-large-isolated.csv"
      local large_json="${OUTDIR}/${DATE}-${ARCH_TAG}-${PROFILE}-large-isolated.json"
      cp target/benchmark-smoke/small-file-results.csv "$large_csv"
      cp target/benchmark-smoke/small-file-results.json "$large_json"
      echo "✓ Large-file artifacts saved: ${large_csv}"
      ;;
    *)
      echo "Unknown phase: ${PHASE} (use: small, large, all)"
      exit 1
      ;;
  esac

  print_summary
}

main "$@"
