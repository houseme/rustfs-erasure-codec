# Cross-Platform Benchmark Runbook

This runbook documents the full EC performance benchmark workflow for x86_64 and aarch64 platforms.

## 1. Prerequisites

| Dependency | Minimum Version | Notes |
|---|---|---|
| Rust | 1.85+ | Requires `edition = 2024` support |
| cargo | Matches rustc | |
| bc | Any | Used for ratio calculations in the script |
| git | 2.x | For revision recording |

aarch64 additional requirements:
- NEON support (all Apple Silicon / ARMv8+ devices satisfy this)
- ARMv9 hardware required for SVE backend testing

## 2. Quick Start

### 2.1 Full Benchmark Suite (Recommended)

```bash
bash scripts/run-full-benchmark.sh
```

Default behavior:
- Extended profile (4x2 + 10x4, shard 1K→1M)
- 5 iterations per case
- 15-second cooldown intervals
- Auto-detects architecture and collects hardware info
- Outputs to `benchmarks/<arch>/`

### 2.2 Small Files Only

```bash
bash scripts/run-full-benchmark.sh --phase small
```

### 2.3 Large Files Only (Isolated)

```bash
bash scripts/run-full-benchmark.sh --phase large
```

### 2.4 Custom Parameters

```bash
bash scripts/run-full-benchmark.sh \
  --profile fast \
  --iterations 10 \
  --cooldown 30 \
  --features "std simd-accel"
```

## 3. Platform-Specific Notes

### 3.1 x86_64 (Linux)

Auto-detected SIMD backends:
- `rust-gfni-avx512` — Requires GFNI + AVX-512 (Intel Ice Lake+ / AMD Zen 4+)
- `rust-avx2` — Requires AVX2 (Haswell+)
- `rust-ssse3` — Requires SSSE3 (Core 2+)
- `scalar-rust` — Fallback when no SIMD is available

Force a specific backend:

```bash
RSE_BACKEND_OVERRIDE=rust-avx2 bash scripts/run-full-benchmark.sh
```

### 3.2 aarch64 (Linux / macOS)

Auto-detected SIMD backends:
- `rust-neon` — Requires NEON (all ARMv8+)
- `scalar-rust` — Fallback

Apple Silicon notes:

```bash
# On macOS, install gnu-time for better timing resolution
brew install gnu-time

# Ensure release build is used
bash scripts/run-full-benchmark.sh --features "std simd-accel"
```

aarch64 Linux (e.g., Graviton, Kunpeng):

```bash
# Verify NEON availability
grep -i neon /proc/cpuinfo

# Run
bash scripts/run-full-benchmark.sh
```

### 3.3 Backend Consistency Verification

Run the backend consistency sweep to confirm all available backends behave identically:

```bash
# x86_64
bash scripts/run_x86_backend_smoke_matrix.sh

# aarch64
bash scripts/run_aarch64_backend_smoke_matrix.sh
```

## 4. Output Structure

```
benchmarks/
├── x86_64-linux/
│   ├── 2026-06-17-x86_64-linux-extended-hwinfo.txt
│   ├── 2026-06-17-x86_64-linux-extended.csv
│   ├── 2026-06-17-x86_64-linux-extended.json
│   └── 2026-06-17-x86_64-linux-extended-large-isolated.csv
├── aarch64-linux/
│   ├── 2026-06-18-aarch64-linux-extended-hwinfo.txt
│   ├── 2026-06-18-aarch64-linux-extended.csv
│   └── 2026-06-18-aarch64-linux-extended.json
└── aarch64-darwin/
    ├── 2026-06-18-aarch64-darwin-extended-hwinfo.txt
    ├── 2026-06-18-aarch64-darwin-extended.csv
    └── 2026-06-18-aarch64-darwin-extended.json
```

### hwinfo.txt Contents

The following information is collected automatically:

- CPU model, frequency, core count
- L1/L2/L3 cache sizes
- SIMD instruction set flags
- Total memory
- Kernel version
- OS version
- Rust/cargo versions
- C compiler version
- System load
- Benchmark configuration parameters

## 5. Cooldown Strategy

The script includes a built-in cooldown mechanism:

| Phase | Default Cooldown | Notes |
|---|---|---|
| Before start | 15s | Ensures CPU exits boost thermal state |
| Small files → large files | 15s | Inter-phase cooldown |
| `--cooldown` flag | Custom | 30-60s recommended for high-load environments |

Verifying cooldown is sufficient:

```bash
# Check if CPU frequency is near base frequency
grep 'cpu MHz' /proc/cpuinfo | head -1

# aarch64 Linux
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq

# macOS
sysctl -n hw.cpufrequency
```

## 6. Result Comparison

### 6.1 Same-Platform A/B Comparison

```bash
# Old baseline
OLD=benchmarks/x86_64-linux/2026-06-17-x86_64-linux-extended.csv

# New results
NEW=benchmarks/x86_64-linux/2026-06-18-x86_64-linux-extended.csv

# Using check_benchmark_regression.py
cargo xtask check-benchmark \
  --baseline "$OLD" \
  --current "$NEW" \
  --metric ns_per_iter \
  --threshold reconstruct_opt=0.10
```

### 6.2 Cross-Platform Comparison

Cross-platform comparisons require care:
- Absolute values are not directly comparable (different CPU performance levels)
- Focus on **ratios** (opt/plain ratio) and **trends**
- Performance ratios for the same operation should remain stable across platforms

### 6.3 Key Metrics

| Metric | Purpose | Priority |
|---|---|---|
| `ns_per_iter` | Small-file latency comparison | High |
| `throughput_mb_s` | Large-file throughput comparison | High |
| opt/plain ratio | Optimization effectiveness verification | High |
| encode/verify ratio | Regression detection baseline | Medium |

## 7. FAQ

### Q: What if `simd-accel` feature is not available on aarch64?

NEON support on aarch64 is automatically enabled through the `std` feature. No additional feature flag is needed:

```bash
bash scripts/run-full-benchmark.sh --features std
```

### Q: How do I verify which backend is actually in use?

Check the `backend` and `backend_id` columns in the CSV output, or:

```bash
RSE_BACKEND_OVERRIDE=auto cargo test --release --features "std simd-accel" \
  --test benchmark_smoke -- --ignored --nocapture 2>&1 | grep -i backend
```

### Q: What if large-file testing causes OOM?

The 10x4_1M case requires ~80MB of memory (14 shards × 1MB). If OOM occurs:

```bash
# Test only 4x2 large files
RSE_BENCH_CASE_FILTER='4x2_512k,4x2_1m' bash scripts/run-full-benchmark.sh --phase large
```

### Q: How to use in CI environments?

CI environments should use the fast profile with fewer iterations:

```bash
bash scripts/run-full-benchmark.sh --profile fast --iterations 3 --cooldown 5
```

## 8. Documentation Template

After completing a cross-platform benchmark run, record results using the following template:

```markdown
## <Platform> Benchmark Results — <Date>

### Hardware
- CPU:
- Cores:
- Frequency:
- Memory:
- SIMD:

### Software
- OS:
- Kernel:
- Rust:
- Features:

### Results Summary
| Case | reconstruct | reconstruct_opt | ratio |
|---|---|---|---|
| 4x2_1K | | | |
| 4x2_1M | | | |
| 10x4_1K | | | |
| 10x4_1M | | | |

### Conclusion
- opt/plain ratio stable:
- Any regressions:
- Notes:
```
