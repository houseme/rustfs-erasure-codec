# Fused NEON encode review — 2026-10-16

Baseline: `642217a40a8798eb8756a83c84ac4b525546eabc`. Retained runtime: `0e2b5bad5503fb29528539c0d12bf2ef004b3601`. Both are on `houseme/erasure-codec-review-fixes`, which already contains main `60bb40c`.

## Review and implementation

The existing checked shard-view preparation and ISA/shape selection remain the single entry to generated kernels. This review traced the cumulative branch path through `encode_sep`, the bounded parallel route, `SelectedEncodeKernel::execute`, and the generated functions. The new diff affects the NEON emitter, its direct regression test, and measurement tooling. It adds no runtime dispatch variants.

The generator keeps one fused template for all six shapes. Each input vector is loaded once; its low/high nibbles feed every parity accumulator. Buffer pointers are captured before the vector loop after the dispatcher validates counts, matrix-row widths, and all actual byte lengths. The loop performs unaligned 16-byte loads/stores only while `offset + 16 <= (shard_len & !15)`. The pointers derive from live slice borrows, stay within their allocations, and never escape the call. Scalar tails retain ordinary indexed field arithmetic.

A register-budget grouping draft removed vector spills but duplicated input/nibble work across output groups. Its diagnostic paired run showed small-shard regressions, so that unpublished draft was replaced before push. The retained emitter preserves fusion and shares extraction work; it does not layer another fallback or per-layout exception onto dispatch. This tradeoff matters more than minimizing spill counts alone.

No unresolved blocker was found in the final changed runtime paths after the source, caller, unsafe-boundary, and regression review. Existing reconstruction planning, backend override rules, job limits, and encoded format remain covered by the workspace regressions.

## Generated-code evidence

The release disassembly was inspected with `otool -tvV`. For each generated function, a backward-branch loop containing `tbl` was identified. The table counts static instructions per 16-byte offset across all parity outputs. Stack counts include q/d register loads/stores addressed through sp/x29; they are not elapsed time or total stack size. The compiler is stable `rustc 1.99.1` on aarch64 macOS.

| Shape | Before loop instructions | After | Before vector stack accesses | After |
| --- | ---: | ---: | ---: | ---: |
| 4+2 | 54 | 41 | 1 | 0 |
| 6+3 | 117 | 92 | 13 | 14 |
| 8+3 | 152 | 125 | 20 | 23 |
| 8+4 | 193 | 160 | 30 | 33 |
| 10+4 | 247 | 201 | 44 | 44 |
| 12+4 | 306 | 254 | 67 | 67 |

The retained 10+4/12+4 loops still have vector stack traffic. Instruction counts decrease while fusion preserves input sharing. Spill-free code was not assumed to be faster.

## Correctness and compile checks

- Workspace tests with `simd-accel benchmark-metrics`, excluding the fuzz runner.
- `no_std + simd-accel` tests.
- Workspace all-target/all-feature Clippy with warnings denied, including fuzz targets.
- AddressSanitizer direct generated-kernel oracle and encoding/dispatch integrations on nightly.
- x86_64 macOS library/test compilation, Linux x86_64 library compilation, wasm32 wrapper compilation, formatting and diff checks.

The direct kernel oracle bypasses possible generic fallback and compares scalar GF arithmetic for all six supported shapes over 258 shape/length combinations. It covers empty checked invocations, every 16-byte tail residue, unaligned views, coefficients including 0/1/255, and untouched prefix/suffix guards. Existing integrations cover large tails, custom matrices, backend overrides, non-Sync field markers, malformed native views, and bounded parallel execution. x86 runtime, MSRV 1.98.1, real WASM integration, and downstream S3 throughput were not exercised in this follow-up.

## Timing evidence and limits

[All raw observations](../benchmarks/recovery-paths/2026-10-16-neon.json) retain four probes: rejected separate-process grouping draft, diagnostic paired grouping draft, paired fused implementation with distinct buffers, and the final paired fused implementation with shared buffer addresses. The rejected runtime source delta is embedded for reconstruction; its unpublished commit is not part of delivered history.

The separate-process run suffered large B1/B2 changes even in unchanged controls and cannot establish latency conclusions. A distinct-buffer paired run then observed a 4+2/1MiB slowdown. Sharing the identical buffer between implementations removed data-placement differences; that slowdown did not recur. Both observations remain archived rather than being discarded.

The final paired probe uses one process, serial encoding, adjacent A1/B1/B2/A2 phases per case, 20 warmups per phase, and three 150 ms samples in batches of eight. Both byte outputs are compared and verified before timing, poisoned parity is re-encoded, and final parity is verified again. All 18 cases passed both 5% drift gates. These are local warm observations; hardware resources were not reserved, and neither parallel/server throughput nor cross-machine speedups are established.

| Layout / shard bytes | Baseline us | Candidate us | Change |
| --- | ---: | ---: | ---: |
| 10x4 / 1024 | 0.677 | 0.677 | +0.1% |
| 10x4 / 32768 | 20.912 | 19.099 | -8.7% |
| 10x4 / 1048576 | 774.257 | 755.648 | -2.4% |
| 12x4 / 1024 | 0.789 | 0.776 | -1.6% |
| 12x4 / 32768 | 26.990 | 24.354 | -9.8% |
| 12x4 / 1048576 | 1001.142 | 990.215 | -1.1% |
| 8x3 / 1024 | 0.454 | 0.448 | -1.3% |
| 8x3 / 32768 | 11.621 | 11.200 | -3.6% |
| 8x3 / 1048576 | 444.815 | 431.256 | -3.0% |
| 8x4 / 1024 | 0.556 | 0.551 | -1.0% |
| 8x4 / 32768 | 15.055 | 14.278 | -5.2% |
| 8x4 / 1048576 | 549.868 | 537.074 | -2.3% |
| 6x3 / 1024 | 0.363 | 0.362 | -0.4% |
| 6x3 / 32768 | 8.282 | 8.109 | -2.1% |
| 6x3 / 1048576 | 274.118 | 266.132 | -2.9% |
| 4x2 / 1024 | 0.225 | 0.233 | +3.5% |
| 4x2 / 32768 | 3.970 | 3.933 | -0.9% |
| 4x2 / 1048576 | 122.714 | 121.981 | -0.6% |

The 32 KiB observations for 10+4, 12+4, and 8+4 are about 8.7%, 9.8%, and 5.2% lower latency respectively. The 4+2/1KiB case is about 8 ns (3.5%) higher; no universal optimum or zero-regression guarantee is claimed. Isolated Linux/cross-architecture validation remains tracked by [backlog #2853](https://github.com/rustfs/backlog/issues/2853).

## Reproducing the paired probe

[`scripts/paired_encode_probe.rs`](../scripts/paired_encode_probe.rs) is the exact measurement logic, formatted for reuse. It is a standalone ARM64 probe rather than an automatically discovered Cargo bench. Create a temporary Cargo project with `[workspace]`, edition 2024, and two dependency aliases named `base_codec` and `next_codec`, each enabling `simd-accel`. Point them to the two source snapshots.

Cargo rejects two local packages with identical name/version in one lockfile. In the temporary baseline copy only, change the package version and its workspace self-dependency version from 9.0.0 to 0.0.1. All baseline Rust and build-script bytes must remain identical to baseline `642217a`. Verify that every common dependency version/checksum in the temporary lockfile matches the committed baseline lockfile. No product release version is changed.

Copy the probe to the temporary project's `src/main.rs`, build release with the same compiler and flags, and run with `RSE_BACKEND_OVERRIDE` unset. This script asserts the NEON backend and records each phase's samples and iteration counts. Set `RAYON_NUM_THREADS=4` and `RS_PARALLEL_POLICY_MAX_JOBS=4` for the documented context. Keep other builds/benchmarks stopped during measurement.

The regular `recovery_paths` profile now contains 58 cases: serial encoding covers every generated shape at 1 KiB, 32 KiB, and 1 MiB. Its `encode.` filter selects 21 cases. Baseline and candidate standalone probes require independent target directories and generated-source inspection. The full 58-case profile was compiled; it is not claimed as a complete timed A/B validation in this follow-up.
