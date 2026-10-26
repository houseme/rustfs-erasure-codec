# Benchmark Methodology

## 1. Scope

This document standardizes how to run, collect, and compare benchmark results for this crate.

Targets:

- smoke regression (`tests/benchmark_smoke.rs`)
- small-file matrix (`tests/benchmark_small_files.rs`)
- stream path matrix (`tests/benchmark_stream_paths.rs`)
- throughput matrix (`benches/throughput_matrix.rs`)
- backend kernel benchmarks (`benches/galois_backend.rs`)

## 2. Baseline Commands

Smoke (fast regression):

```bash
cargo test --test benchmark_smoke benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
```

Smoke profiles:

- `RSE_SMOKE_PROFILE=quick`: single small 4+2 / 64 KiB case for default release-check use
- `RSE_SMOKE_PROFILE=fast`: quick profile plus 10+4 / 1 MiB
- `RSE_SMOKE_PROFILE=extended`: full smoke matrix with higher default iteration count

Example:

```bash
RSE_SMOKE_PROFILE=quick cargo test --test benchmark_smoke benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
```

Small-file matrix:

```bash
RSE_SMALL_FILE_PROFILE=fast \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture
```

Small-file profiles:

- `RSE_SMALL_FILE_PROFILE=quick`: `4+2` on `1 KiB / 4 KiB / 16 KiB / 64 KiB`
- `RSE_SMALL_FILE_PROFILE=fast`: `4+2` on `1 KiB / 4 KiB / 16 KiB / 64 KiB / 128 KiB / 256 KiB / 512 KiB`, plus `10+4` on `16 KiB / 64 KiB / 256 KiB / 512 KiB`
- `RSE_SMALL_FILE_PROFILE=extended`: full small-file baseline/update run through `1 MiB` for both `4+2` and `10+4`

Small-file helper script:

```bash
bash scripts/run_small_file_benchmark_matrix.sh
```

Stream path matrix:

```bash
RSE_STREAM_PROFILE=fast \
cargo test --release --features "std simd-accel" \
  --test benchmark_stream_paths \
  benchmark_stream_path_matrix_runs_and_exports_results -- --ignored --nocapture
```

Stream path profiles:

- `RSE_STREAM_PROFILE=quick`: `4+2 / 64 KiB` Memory backend smoke for `encode_stream` and `verify_stream`
- `RSE_STREAM_PROFILE=fast`: `4+2` and `10+4` Memory backend coverage for `encode_stream`, `verify_stream`, and `reconstruct_stream`
- `RSE_STREAM_PROFILE=extended`: broader Memory backend coverage through larger shard counts for stream path regression analysis

Stream path I/O modes:

- `RSE_STREAM_IO_MODE=auto`: use the current `StreamOptions` automatic I/O scheduling policy
- `RSE_STREAM_IO_MODE=serial`: force serial stream reads and writes
- `RSE_STREAM_IO_MODE=parallel`: force rayon-backed parallel stream reads and writes

Stream path artifacts archived in this repository:

- `benchmarks/stream-path/2026-06-28-cooldown/phase0-51e0b64-fast.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-baseline-f1ad373-fast-auto-selected-iter10.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-current-2a1aa88-fast-auto-selected-iter10.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-baseline-f1ad373-fast-serial-4x2_64k-iter20.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-current-2a1aa88-fast-serial-4x2_64k-iter20.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-baseline-f1ad373-extended-auto.csv`
- `benchmarks/stream-path/2026-06-28-cooldown/phase4-current-2a1aa88-extended-auto.csv`

Filtered stream path drill-down:

```bash
RSE_STREAM_PROFILE=fast \
RSE_STREAM_IO_MODE=serial \
RSE_STREAM_CASE_FILTER=10x4_64k \
RSE_STREAM_ITERATIONS=10 \
cargo test --release --features "std simd-accel" \
  --test benchmark_stream_paths \
  benchmark_stream_path_matrix_runs_and_exports_results -- --ignored --nocapture
```

Stream path cooldown A/B protocol:

1. Run baseline and current on the same machine, with the same feature set,
   backend mode, `RSE_STREAM_PROFILE`, `RSE_STREAM_IO_MODE`, filter, and
   iteration count.
2. Insert at least a 20 second cooldown between every benchmark invocation.
3. Treat full-matrix 3-iteration results as a screening pass only. If one case
   looks suspicious, rerun that case in isolation with `RSE_STREAM_ITERATIONS=10`
   or higher before treating it as a regression.
4. Prefer `ns_per_block` for stream regression gates because it normalizes
   multi-block cases better than whole-iteration latency.
5. Archive the exact CSV/JSON artifacts before running the next mode, because
   `target/benchmark-smoke/stream-path-results.*` is overwritten by each stream
   benchmark invocation.

Repeated reconstruct planning reuse:

- for repeated `Option<Vec<u8>>` reconstruct workloads with a stable missing
  pattern, prefer `prepare_reconstruct_opt_workspace(...)` once and
  `reconstruct_opt_with_workspace(...)` inside the hot loop
- this avoids rebuilding the option-vec reconstruct plan on every call and is
  the preferred measurement shape when evaluating repeated serial reconstruct
  workloads such as `10x4_64k`

Throughput matrix:

```bash
cargo bench --locked --bench throughput_matrix --features "std simd-accel"
```

Criterion calls its benchmark callback repeatedly during calibration and each
sample. Keep retained source storage outside that callback, or initialize it
once through an outer `Option`; putting construction outside `b.iter` alone does
not make it benchmark-lived. Update views borrow the original immutable data and
are prepared once; verification results are explicitly consumed.

`throughput_matrix_update` includes output cloning, view construction and drop
on each iteration. `throughput_matrix_update_in_place` retains its output and
views, alternating valid forward/reverse transitions across all callback
boundaries, including odd iteration counts. Both have one/two-change cases and
sparse/dense inputs (`_dense` suffix). Keep the existing sparse IDs and compare
only like-for-like caller lifetimes and patterns. Rates use logical complete
stripe bytes for historical compatibility, not total memory traffic.

```bash
cargo bench --locked --bench throughput_matrix --features "std simd-accel" -- throughput_matrix_update
```

Short `--warm-up-time` / `--measurement-time` overrides are useful for functional
and constructor-lifetime checks only. Criterion's independently calibrated counts
and a shared development host do not establish formal repeatability or release
acceptance. Prebuild all binaries before a measurement schedule; do not build or
switch source between its phases. Use the complete Linux release-evidence
collector for publication qualification and retain every failed control.

The optional `throughput-profile-report` now uses schema 2, adds the full
`benchmark` name, and contains only executed benchmarks. Its identity is
`(operation, benchmark)`, which distinguishes change counts, patterns and caller
lifetimes; `case` alone describes geometry. These counter records are separate
from Criterion timing samples and from formal release evidence.

Backend kernels:

```bash
cargo bench --bench galois_backend --features std
```

Backend kernel A/B/B/A gate (nonempty, complete campaigns with both drift checks):

```bash
RSE_ABBA_BACKEND=auto \
RSE_ABBA_SAMPLE_SIZE=20 \
RSE_ABBA_WARMUP_TIME=2 \
RSE_ABBA_MEASUREMENT_TIME=2 \
RSE_ABBA_COOLDOWN_SECONDS=20 \
bash scripts/run_galois_backend_abba.sh <baseline-ref> <candidate-ref>
```

The script switches refs in `A1 / B1 / B2 / A2` order, extracts raw Criterion
mean estimates immediately after every stage, and writes `estimates.csv`,
`summary.json`, `summary.md`, logs, and run metadata under
`target/benchmark-smoke/galois-backend-abba-*`. Treat the performance conclusion
as invalid when the A1→A2 baseline drift gate fails; in that case report only
the validation status and rerun under better resource isolation.

SIMD smoke (when available):

```bash
cargo test --features "std simd-accel" --test benchmark_smoke benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
```
Use `--ignored` here as well because the artifact-producing smoke benchmark is intentionally not part of the default `cargo test` surface:

```bash
cargo test --features "std simd-accel" --test benchmark_smoke benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
```


The analyzer rejects empty/incomplete campaigns, duplicate case/stage estimates,
invalid numeric values, inconsistent resolved revisions and conflicting backend
provenance. Both A1/A2 and B1/B2 must pass the configured drift limit. A failure
in any case withholds candidate attribution for the entire campaign, writes an
explicit failed report and returns nonzero. It does not add a speedup/regression
threshold. Passing these structural checks still does not establish resource
isolation. For isolated acceptance, prepare both binaries before measurement,
reserve and verify the execution resources, and retain every phase's evidence;
the convenience runner's Cargo/ref switching is not itself an isolation proof.

### Classic plan lifecycle diagnostic

The standalone `reconstruct_plans` target shares the shape resource catalog and
keeps fixture owners outside repeated Criterion callbacks. It separates full
workspace preparation, no-output preparation and no-output calls for GF8/tower
GF16, including wide and malformed layouts. Returned owned plans are destroyed
inside `iter`; this is a lifecycle window, not a drop-excluded timer.

```bash
cargo +stable bench --locked --features simd-accel --bench reconstruct_plans -- --test
```

This functional command checks all 348 IDs without a statistical timing campaign.
See [the source-bound follow-up](reconstruct-plan-benchmark-2026-10-26.md) for
preflight oracles, paired-source overlays, lifecycle boundaries and the remaining
controlled-environment gates. These diagnostics do not replace release evidence.

## 3. Determinism Rules

- Use fixed seeds from `benches/common/mod.rs` (`BASE_SEED` + `derived_seed`).
- Do not mix old and new build artifacts when comparing results.
- For A/B comparisons, run both sides from clean builds.

## 4. Output Artifacts

Smoke outputs:

- `target/benchmark-smoke/smoke-results.json`
- `target/benchmark-smoke/smoke-results.csv`

Small-file outputs:

- `target/benchmark-smoke/small-file-results.json`
- `target/benchmark-smoke/small-file-results.csv`

Stream path outputs:

- `target/benchmark-smoke/stream-path-results.json`
- `target/benchmark-smoke/stream-path-results.csv`

Cache analysis outputs (tests):

- `target/benchmark-smoke/reconstruction-cache-stats.json`
- `target/benchmark-smoke/reconstruction-cache-patterns.csv`
- `target/benchmark-smoke/reconstruction-hotspot-results.json`
- `target/benchmark-smoke/reconstruction-hotspot-results.csv`

Artifact history rule:

- artifact overwrite/append behavior is harness-specific
- stream path JSON/CSV artifacts are single-run snapshots overwritten by each
  stream benchmark invocation; copy them before running the next mode
- for append-style artifacts, repeated executions append new records to the
  existing file
- when comparing one run to another, either filter by the intended records or
  archive/copy the file before the next run

These artifact-producing reconstruction benchmarks are marked `#[ignore]` on purpose.
Run them explicitly so normal `cargo test` stays bounded and does not mix
correctness validation with long-running CPU-saturating workload measurement.

Explicit commands:

```bash
cargo test --features std benchmark_parallel_helpers_quantify_gain -- --ignored --nocapture
cargo test --features std benchmark_reconstruction_cache_patterns -- --ignored --nocapture
cargo test --features std benchmark_reconstruction_cache_layers -- --ignored --nocapture
cargo test --release --features "std simd-accel" benchmark_reconstruction_hotspots -- --ignored --nocapture
```

If you are debugging a hang-like symptom or want a lighter local sampling pass,
lower the inner loop count:

```bash
RSE_TEST_BENCH_ITERATIONS=1 \
  cargo test --features std benchmark_parallel_helpers_quantify_gain -- --ignored --nocapture
```

Regression gate inputs / outputs:

- baseline artifact: user-provided `smoke-results.json` via `RSE_SMOKE_BASELINE`
- current artifact: `target/benchmark-smoke/smoke-results.json`
- consistency sweep: `scripts/check_backend_consistency.sh`

Small-file gate inputs / outputs:

- baseline artifact: archived `small-file-results.json` or `small-file-results.csv`
- current artifact: `target/benchmark-smoke/small-file-results.json`
- recommended metric for small shards: `ns_per_iter`

Throughput profiling output (optional):

- `target/benchmark-smoke/throughput-profile-report.json` (when `RSE_WRITE_PROFILE_REPORT=1`)

## 4.1 Unified Artifact Schema

Every benchmark or profiling artifact should now expose a stable core envelope so
results from different phases can be compared without guessing field meaning.

Required core fields when applicable:

- `schema_version`
- `artifact_kind`
- `benchmark_metrics_enabled`
- `git_revision`
- `target_triple`
- `features`
- `backend`
- `backend_id`
- `backend_kind`
- `backend_override`
- `operation`
- `data_shards`
- `parity_shards`
- `shard_size`

Artifact-specific fields remain allowed, but the core envelope should stay stable.

Current artifact mapping:

- `smoke-results`: regression-oriented throughput snapshot
- `small-file-results`: small-shard latency/throughput snapshot
- `stream-path-results`: stream encode/verify/reconstruct latency and throughput snapshot
- `parallel-helper-results`: serial vs optimized helper comparison
- `reconstruction-hotspot-results`: reconstruct workload comparison
- `reconstruction-cache-stats`: cache observability snapshot
- `reconstruction-cache-patterns`: cache workload comparison
- `throughput-profile-report`: runtime/profile counter export

Current schema version:

- `schema_version = 1`

Notes:

- JSON artifacts should always include `schema_version` and `artifact_kind`.
- CSV artifacts should keep a fixed leading column order that starts with
  `schema_version,artifact_kind`.
- Scripts may ignore extra fields, but should not assume field order beyond the
  documented CSV header.

## 5. Comparison Protocol

1. Record commit hash and feature set.
2. Keep backend selection explicit:
   - default runtime dispatch, or
   - force backend via `RSE_BACKEND_OVERRIDE`.
3. For each operation (`encode`, `verify`, `reconstruct`, `reconstruct_data`), compare:
   - throughput (`throughput_mb_s`)
   - latency (`ns_per_iter`) when available
4. For small-file decisions, prioritize `ns_per_iter` over `throughput_mb_s`, especially for `1 KiB` to `64 KiB`.
5. Use median of repeated runs for decisions; avoid single-run conclusions.
6. If only `1 KiB` or `4 KiB` cases look bad while neighboring points are stable, rerun that case in isolation with a higher iteration count before treating it as a real regression.
7. If a full `extended` run shows broad small-file regressions but targeted filtered reruns disagree, treat the filtered reruns as the more trustworthy signal for code-change decisions.

Example drill-down:

```bash
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=10x4_1k \
RSE_SMALL_FILE_ITERATIONS=40 \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture
```

## 6. Cache Metrics Interpretation

For reconstruction cache stats:

- `requests`: total decode-matrix lookup attempts
- `hits`: cache hits
- `misses`: cache misses
- `inserts`: inserted decode-matrix entries
- `evictions`: entries evicted by LRU capacity pressure

Derived analysis fields:

- `hit_rate = hits / requests`
- `reuse_ratio = hits / inserts`
- `miss_cost_per_request = misses / requests`

## 7. Recommended Release Checklist

Use the fixed script:

```bash
./scripts/release-check.sh
```

This script runs test gates for:

- unit/integration correctness
- self-test entry
- benchmark smoke
- smoke regression gate (when `RSE_SMOKE_BASELINE` is set)
- small-file regression gate (when `RUN_SMALL_FILE_GATE=1` and `RSE_SMALL_FILE_BASELINE` is set)
- stream path regression gate (when `RUN_STREAM_PATH_GATE=1` and `RSE_STREAM_PATH_BASELINE` is set)
- backend/ISA consistency sweep (when `RUN_BACKEND_CONSISTENCY=1`)
- `no_std` build path
- `std` and optional `simd-accel` paths

If you want a lower-noise local pass that avoids test-level parallelism and
keeps smoke work on the smallest profile, use:

```bash
./scripts/serial-test-check.sh
```

## 8. Smoke Regression Gate

The independent reconstruction hotspot gate also compares every baseline
scenario, even without `--require-scenario`. Required scenarios and overrides
add requirements rather than selecting a smaller comparison. It rejects
empty/duplicate/dropped scenarios, nonpositive or non-finite metrics, invalid
limits, and conflicting workload or available provenance fields through the
same normalization helpers as the general gate. Legacy metadata absent on both
sides remains unavailable evidence; this does not establish CPU/build isolation
or replace a complete drift-gated campaign.


The gate requires every baseline case to remain in the current artifact, rejects
empty/duplicate results and invalid metrics, and checks available target,
backend, feature, instrumentation, and stream I/O metadata. For a deliberately
smaller profile, pass `--allow-missing-cases`; `release-check.sh` forwards this
only outside strict release mode when `RSE_BENCHMARK_ALLOW_MISSING_CASES=1`. Omitted cases remain listed in
`missing_cases`, at least one comparable case is required, and explicit
`--require-case` checks still apply to both artifacts. Legacy provenance fields
absent from both sides are reported in `unchecked_provenance`; a field missing
on just one side requires `--allow-missing-provenance`. That option never permits
different values present on both sides.

Use `--strict-provenance` for comparisons that require complete recorded
provenance. Every row in both artifacts, including unmatched cases, must provide
these canonical fields: `target_triple`, `features`,
`benchmark_metrics_enabled`, `backend`, `cpu_model`, `compiler`, `build_profile`,
`rayon_threads`, `available_parallelism`, and `max_parallel_jobs`. Stream cases
also require `stream_io_backend` and `stream_io_mode`. The legacy
`--allow-missing-provenance` option cannot waive strict required fields.

The gate resolves these aliases before comparing JSON or CSV records:

| Canonical field | Accepted aliases |
|---|---|
| `backend` | `backend_name` |
| `cpu_model` | `cpu` |
| `compiler` | `compiler_version`, `rustc`, `rustc_version` |
| `build_profile` | `cargo_profile` |
| `rayon_threads` | `rayon_num_threads` |
| `max_parallel_jobs` | `max_jobs` |

Aliases in one row must agree. CPU whitespace and the optional `rustc ` compiler
prefix are normalized; compiler version, hash, and date are otherwise retained.
Do not substitute an MSRV declaration for the compiler that produced the binary.
`profile` denotes the workload profile (such as `fast` or `extended`), is
compared separately, and cannot satisfy `build_profile`. Thread counts must be
positive integers; only configured `max_parallel_jobs` permits zero, meaning
automatic selection. Zero and an explicit positive cap are different settings.
`rayon_threads` is the actual worker pool size, and `available_parallelism` is
the process-visible parallelism budget, not an alias for the configured cap.

Missing/null metadata and placeholders such as `unknown` or `nan` do not satisfy
strict requirements. CPU, compiler, and profile strings cannot be blank. For
`features`, the explicit value `none` represents an empty feature set; a blank
field does not satisfy strict mode. Known compiler, workload/build profile,
thread-budget, `effective_max_jobs`, `target_cpu`, or `rustflags` differences
fail in either mode, even with legacy overrides. Commit revisions, iteration
counts, and fixture seeds may differ. The JSON summary records
`strict_provenance`; compatibility mode still permits unavailable metadata on
both sides and lists it in `unchecked_provenance`.

Equal metadata, including CPU model names, does not establish that runs used the
same physical machine, exclusive resources, or passed a baseline-drift check.
Strict provenance complements resource isolation and repeated controlled runs;
it is not a complete performance acceptance test. Older artifacts often lack
the required build/CPU/thread fields and therefore need newly recorded evidence
before they can pass strict mode.

Compare a fresh smoke run against a checked-in or archived baseline:

```bash
RSE_SMOKE_BASELINE=/abs/path/to/smoke-results.json \
cargo xtask check-benchmark \
  --baseline "$RSE_SMOKE_BASELINE" \
  --current target/benchmark-smoke/smoke-results.json
```

Default allowed regressions:

- `encode`: 10%
- `verify`: 12%
- `reconstruct`: 15%
- `reconstruct_data`: 15%

Thresholds can be overridden per operation:

```bash
cargo xtask check-benchmark \
  --baseline old.json \
  --current new.json \
  --threshold reconstruct=0.18 \
  --threshold reconstruct_data=0.18
```

The same tool also supports small-file latency checks:

```bash
cargo xtask check-benchmark \
  --baseline old-small-file.json \
  --current target/benchmark-smoke/small-file-results.json \
  --metric ns_per_iter \
  --threshold verify_with_buffer=0.12 \
  --require-case encode:4:2:1024 \
  --require-case verify_with_buffer:4:2:4096 \
  --require-case reconstruct_data:10:4:65536
```

`release-check.sh` can also run the small-file gate:

```bash
VALIDATION_PROFILE=extended \
RUN_SMALL_FILE_GATE=1 \
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_BASELINE=/abs/path/to/small-file-results.json \
./scripts/release-check.sh
```

Recommended defaults for small-file checks:

- `RSE_SMALL_FILE_METRIC=ns_per_iter`
- `RSE_SMALL_FILE_PROFILE=fast` for day-to-day validation
- `RSE_SMALL_FILE_PROFILE=extended` when refreshing an archived baseline

## 8.1 Stream Path Regression Gate

Generate the current stream path artifact:

```bash
RSE_STREAM_PROFILE=fast \
RSE_STREAM_IO_MODE=auto \
cargo test --release --features "std simd-accel" \
  --test benchmark_stream_paths \
  benchmark_stream_path_matrix_runs_and_exports_results -- --ignored --nocapture
```

Compare it against a saved stream baseline using per-block latency:

```bash
cargo xtask check-benchmark \
  --baseline /path/to/stream-path-results.json \
  --current target/benchmark-smoke/stream-path-results.json \
  --metric ns_per_block \
  --threshold encode_stream=0.15 \
  --threshold verify_stream=0.15 \
  --threshold reconstruct_stream=0.18 \
  --require-case encode_stream:4:2:65536:65536 \
  --require-case verify_stream:4:2:65536:65536 \
  --require-case reconstruct_stream:10:4:1048576:1048576
```

Or drive it through the release workflow:

```bash
RUN_STREAM_PATH_GATE=1 \
RSE_STREAM_PROFILE=fast \
RSE_STREAM_IO_MODE=auto \
RSE_STREAM_PATH_BASELINE=/path/to/stream-path-results.json \
./scripts/release-check.sh
```

Recommended defaults:

- `RSE_STREAM_PATH_METRIC=ns_per_block`
- `RSE_STREAM_PROFILE=fast` for day-to-day validation
- `RSE_STREAM_PROFILE=extended` only when refreshing an archived stream baseline
- `RSE_STREAM_IO_MODE=auto` for release gates; use `serial` and `parallel` as drill-down modes

## 8.2 Baseline Update Governance

Updating a benchmark baseline is allowed only when the new baseline reflects an
intentional and verified steady-state change.

Allowed cases:

1. An intentional algorithm or scheduling change has landed and the previous
   baseline no longer represents the intended default behavior.
2. Backend priority or dispatch behavior changed intentionally and the new
   default path is the one being validated.
3. A benchmark schema migration occurred, and the old artifact can no longer be
   compared mechanically without lossy translation.

Disallowed cases:

1. A single noisy run looks slower or faster than expected.
2. Different machines, CPUs, or feature sets are being compared as if they were
   the same baseline lineage.
3. The current run used a different backend override, different workload shape,
   or different build cleanliness than the archived baseline without explicit
   justification.

Minimum evidence before refreshing a baseline:

1. Same-machine comparison
2. Matching feature set
3. Matching or explicitly documented backend selection
4. Clean-build comparison when performance-sensitive conclusions are involved
5. Repeated runs with median-oriented interpretation
6. Short rationale describing why the old baseline is no longer authoritative

Recommended baseline refresh note template:

- reason:
- old baseline:
- new baseline:
- machine / cpu:
- feature set:
- backend mode:
- repeated-run summary:
- expected long-term effect:

## 9. Backend Consistency Sweep

When SIMD is available, run the reusable backend override sweep:

```bash
RUN_BACKEND_CONSISTENCY=1 ./scripts/release-check.sh
```

Or invoke it directly:

```bash
bash scripts/check_backend_consistency.sh
```

Host-specific override smoke matrix helpers are also available:

```bash
bash scripts/run_x86_backend_smoke_matrix.sh
bash scripts/run_aarch64_backend_smoke_matrix.sh
```

On Apple Silicon or other `aarch64` hosts, the ARM64 sweep validates
`auto`, `scalar`, and `rust-neon`.

## 9.1 Reconstruction Hotspot Gate

The phase-5 hotspot workloads can now be promoted from ad hoc benchmarks to a
repeatable regression gate.

Generate the current hotspot artifact:

```bash
cargo test --release --features "std simd-accel" benchmark_reconstruction_hotspots -- --ignored --nocapture
```

Compare it against a saved baseline:

```bash
cargo xtask check-hotspot \
  --baseline /path/to/reconstruction-hotspot-results.json \
  --current target/benchmark-smoke/reconstruction-hotspot-results.json \
  --require-scenario reconstruct_data_missing_1_data \
  --require-scenario reconstruct_some_required_1_of_2_missing_data
```

Use `--min-speedup scenario=value` only for scenarios that have already been
proven to require a positive optimization margin on the target machine. Do not
assume every hotspot candidate is universally faster across all architectures.

Or drive it through the release workflow:

```bash
RUN_RECONSTRUCTION_HOTSPOT_GATE=1 \
RSE_RECONSTRUCTION_HOTSPOT_BASELINE=/path/to/reconstruction-hotspot-results.json \
./scripts/release-check.sh
```

## 10. ISA Integration Template

Use this template whenever adding a new SIMD or ISA-specific backend.

Checklist:

1. Add a named backend entry with stable `name`, `id`, and `kind`.
2. Connect runtime dispatch without breaking scalar fallback.
3. Add or update `RSE_BACKEND_OVERRIDE` support for the new backend name.
4. Add scalar correctness comparison tests.
5. Include the backend in reusable consistency sweep scripts when the platform
   can expose it.
6. Validate smoke benchmark output with the new backend selected explicitly.
7. Validate kernel benchmark output where applicable.
8. Document whether the backend is experimental, candidate-default, or
   fallback-only.

Minimum deliverables:

- correctness tests
- backend override coverage
- consistency sweep coverage
- smoke or workload benchmark evidence
- dispatch and fallback notes

## 11. Matrix Mode Integration Template

Use this template whenever extending `MatrixMode` or adding a new matrix
construction strategy.

Checklist:

1. State the target workload or compatibility need.
2. Preserve current default behavior unless the change is explicitly intended to
   alter defaults.
3. Add encode correctness coverage.
4. Add reconstruction correctness coverage.
5. Check interaction with `reconstruct_data` and `reconstruct_some`.
6. Evaluate benchmark impact on at least one representative workload.
7. Update API and governance docs together with the implementation.

Minimum deliverables:

- constructor / option-path coverage
- correctness tests for encode and reconstruct flows
- documented benchmark impact
- rollback story if the mode is not yet ready for general default use

## 12. Metrics Feature Gate

Heavy benchmark and reconstruction metrics are now governed by the
`benchmark-metrics` feature.

Current behavior:

- default features keep `benchmark-metrics` enabled
- disabling default features or explicitly removing `benchmark-metrics` should
  preserve correctness behavior while allowing metric fields to degrade safely
  to zero-valued snapshots

Guidelines:

1. Do not make correctness depend on metrics collection.
2. Benchmark artifacts should record whether metrics were enabled through
   `benchmark_metrics_enabled`.
3. When metrics are disabled, scripts should treat zero-valued counters as
   “metrics unavailable in this build” unless the workload naturally produces
   zeros.

## 13. Native AVX2 Same-Machine Benchmark Runbook

Use this workflow when deciding whether `rust-avx2` is ready to outrank
`simd-c` on a real x86_64 AVX2 machine.

### 13.1 Preconditions

1. Run on a native x86_64 host that actually exposes AVX2.
2. Keep machine, governor, and feature set stable across the whole run.
3. Prefer a clean worktree or at least a clearly recorded commit / diff scope.
4. Avoid mixing incremental artifacts from unrelated benchmark sessions.

### 13.2 Required commands

Primary collection flow:

```bash
bash scripts/collect_x86_simd_benchmarks.sh
```

This flow collects:

- release smoke results per backend override
- `galois_backend` criterion results
- machine summary JSON under `benchmarks/x86_64-simd/`

### 13.3 Minimum evidence package

The benchmark package is not considered decision-ready unless it includes:

1. release smoke comparisons for:
   - `auto`
   - `simd-c`
   - `rust-avx2`
   - `scalar`
2. kernel benchmark comparisons for:
   - `galois_mul_slice`
   - `galois_mul_slice_xor`
3. override-honored checks with no silent mismatch for the compared backends
4. archived machine JSON in `benchmarks/x86_64-simd/`

### 13.4 Decision template

When judging whether `rust-avx2` may become the default x86_64 priority, record:

- machine json:
- commit or diff scope:
- compared backends:
- smoke winner by operation:
- kernel winner by operation:
- recommended priority from summary script:
- diverges from current runtime priority:
- repeated-run stability conclusion:
- adoption decision:
- fallback plan:

### 13.5 Stability rule

Do not promote `rust-avx2` based on one archived run alone.

Recommended minimum:

1. At least two same-machine archived runs
2. No correctness or override mismatch anomalies
3. No contradictory conclusion caused solely by noisy single-run outliers
4. If recommendation changes across runs, prefer the backend that remains
   competitive across smoke and kernel results rather than blindly following one
   aggregate score

### 13.6 Promotion decision classes

Use one of the following final labels:

- `candidate-only`: correctness is good, but performance evidence is not yet stable
- `candidate-default`: evidence is strong enough to test a default-priority change
- `fallback-only`: backend remains useful for explicit override or niche hosts, but
  should not be promoted

### 13.7 Ready-To-Paste Result Record

Use the following record when the AVX2 run finishes:

```text
machine json:
run meta:
commit or diff scope:
compared backends:
smoke winner by operation:
kernel winner by operation:
recommended priority from summary script:
diverges from current runtime priority:
override mismatch count:
repeated-run stability conclusion:
adoption decision:
fallback plan:
follow-up action:
```

Recommended follow-up action values:

- `promote-rust-avx2-candidate`
- `keep-current-default-and-retest`
- `treat-as-fallback-only`

### 13.8 Exit Criteria

Treat a same-machine AVX2 run as decision-ready only when all of the following
are true:

1. at least one archived machine JSON and matching `run-meta.json` exist
2. release smoke evidence is present for the compared backends
3. kernel benchmark evidence is present for the compared backends
4. `override_mismatch_count` is zero
5. a final adoption label has been chosen
6. the result has been written back to the task board and phase 4 document

## 14. Archived X86 Summary Interpretation

The archived JSON under `benchmarks/x86_64-simd/` should be interpreted as
evidence artifacts, not as automatic truth.

Important fields:

- `rankings_10x4_1m`
- `criterion_rankings`
- `recommended_default_priority`
- `release_smoke_override_mismatches`

Interpretation guidance:

1. `recommended_default_priority` is a benchmark-driven suggestion, not an
   unconditional promotion decision.
2. `diverges_from_current_runtime_priority_x86=true` means the archived result
   disagrees with the current runtime order and must be reviewed manually.
3. If different archived runs recommend different orders, treat the result as
   unstable until the reason is explained or repeated runs converge.

## 15. ARM64 Profiling And Feature-Detect Contract

This section standardizes how aarch64 backend evolution should expose feature
detect signals and profiling fields so a future SVE backend can plug into the
same observability story without redefining the rules.

### 15.1 Feature-Detect Rules

1. aarch64 capability detection should flow through `Aarch64FeatureSet`.
2. New aarch64 backend candidates should add explicit fields there before
   dispatch logic starts branching on ad hoc conditionals elsewhere.
3. Placeholder capability fields are allowed when they do not change current
   backend priority or runtime behavior.
4. A new backend must not change existing dispatch order until correctness,
   override behavior, and benchmark evidence are in place.

### 15.2 Override Rules

For each aarch64 backend candidate:

1. `RSE_BACKEND_OVERRIDE` must map to a stable backend name.
2. Strict override mode must either honor the request or fail loudly.
3. Backend metadata should remain consistent across:
   - `active_backend_name()`
   - `active_backend_id()`
   - `active_backend_kind()`
   - smoke result metadata fields

### 15.3 Profiling Rules

1. Profiling should distinguish implementation work from tail fallback work.
2. Runtime profile fields should remain comparable across ARM64 experiments.
3. Backend-local profiling is allowed, but it should not replace the shared
   runtime/profile counters already exported by the benchmark flow.
4. If a future SVE backend adds counters, it should follow the same naming style
   as the current NEON profiling surface.

### 15.4 Minimum Validation For A Future SVE Backend

Before a future `rust-sve` backend is considered usable, it should add:

1. dispatch priority tests
2. scalar correctness comparison
3. override metadata validation
4. smoke metadata verification
5. workload and kernel benchmark evidence

## 16. Codec Construction, Encoding, And Recovery Probe

`benches/recovery_paths.rs` provides a small stable-Rust probe without additional
dependencies. It keeps deterministic fixtures and measures these cases:

- GF8 and GF16 Leopard recovery: `10+4` with 1 KiB, 32 KiB, and 128 KiB + 64-byte
  shards, plus `32+16` with 32 KiB shards. The 32 KiB `10+4` cases separately
  remove one and two data shards; `32+16` removes two.
- Classic stream recovery: 1 MiB shards, `10+4` with 4 KiB and 64 KiB blocks, and
  `32+16` with 4 KiB blocks. Each case removes data shard 0 and parity shard 0
  and uses serial in-memory stream I/O.
- Classic parallel encode: `4+2` and `10+4` with 256 KiB shards, and `10+4` with
  1 MiB shards. Enable the SIMD feature appropriate to the comparison host.
- Codec construction: Classic, Leopard GF8, and Leopard GF16 at `10+4`, `32+16`,
  and `128+64`. These `*.construct` cases create and drop a fresh codec each
  iteration, with shared field tables already warm; `shard_size: 0` means no
  payload is encoded or recovered in this case.
- Classic parity-only recovery: `10+4` with 1 KiB shards and `32+16` with 4 KiB
  shards, each recovering parity shard 0 with inversion caching enabled and
  disabled. The operation labels end in `.cached` and `.uncached` respectively.
- Generic selective parity recovery: `32+16` with 64 KiB shards, missing data
  shard 0 and parity shard 0, requesting only that parity shard through
  `reconstruct_some` with reused `ShardSlot` storage. The unrequested data
  buffer must remain missing and unchanged.
- Ordinary generic Classic data recovery (`classic.reconstruct_data_slots` and
  `classic.reconstruct_data_vec`): `4+2` with 16 KiB and `10+4` with 32 KiB shards,
  each missing one/two data shards. Slots retain backing storage; Vec adapters
  take/drop and reconstruct payload buffers inside timing. Both use the ordinary
  `reconstruct_data` entry point exercised by the pinned ECStore consumer.
- Classic optimized data recovery (`classic.reconstruct_data_opt`): `10+4`
  with 32 KiB and 1 MiB shards, separately missing one and two data shards.
  This API requires `Option<Vec<u8>>`; each iteration takes and drops the
  missing data buffers, then reconstructs replacement buffers. That adapter
  storage turnover is included in the latency and allocation measurements.
- Classic serial encode (`encode.serial`): all generated shapes (`10+4`, `12+4`,
  `8+3`, `8+4`, `6+3`, `4+2`) with 1 KiB, 32 KiB, and 1 MiB shards. These cases
  call `encode`, without the parallel or automatic entrypoint. Each result also
  passes an untimed independent `verify` arithmetic check.
- Classic all-present recovery: `10+4`, `40+8`, and `128+64`, with 65-byte shards
  and no erasures. `classic.reconstruct_all_present` uses the generic API with
  `ShardSlot`; `classic.reconstruct_opt_all_present` uses `Option<Vec<u8>>`.
  These timed closures only call their API, with no buffer resets or replacement.
  The untimed preflight also checks that one shorter present shard is rejected,
  so the fast return cannot skip input validation.

For example, on an ARM64 host:

```bash
RSE_RECOVERY_SAMPLE_MS=100 \
cargo bench --bench recovery_paths --features "std simd-neon" \
  > target/recovery-paths.jsonl

RSE_RECOVERY_CASE_FILTER=gf16.reconstruct \
RSE_RECOVERY_SAMPLE_MS=500 \
cargo bench --bench recovery_paths --features "std simd-neon" \
  > target/recovery-gf16.jsonl
```

`RSE_RECOVERY_SAMPLE_MS` is a positive per-sample duration in milliseconds
(default 100). `RSE_RECOVERY_CASE_FILTER` optionally selects a substring of the
reported `case_id`; a filter matching no cases fails. Each case performs 20
warmup iterations, counts allocations for one further operation, then collects
three `std::time::Instant` latency samples in batches of eight operations.
Except for construction probes, the codec and erasure pattern are reused.
Caller buffers are retained for the ShardSlot, stream, and encode cases;
the generic Vec and `reconstruct_data_opt` cases replace their missing
`Option<Vec<u8>>` buffers each time.
The take/drop cost is included in latency, and replacement allocations are
included in the allocation counts, so these cases should not be treated as
allocation-equivalent to ShardSlot recovery. Stream buffers retain their
capacity when marked missing.
Initialization and fixture allocation are excluded from recovery/encode timing.
An untimed preflight poisons retained missing/parity output bytes or actually
removes Option buffers before invoking the operation, so a successful no-op
cannot pass using previously correct output.
Recovery checks both bytes and presence before and after measurement; selective
recovery also checks that the unrequested missing shard remains unchanged.
All-present cases instead check unchanged content and presence, alongside the
malformed-length preflight described above.
The timed recovery/encode loops retain their original operations and do not
include the preflight poisoning or correctness checks.

Each JSONL row records shard dimensions, missing data/parity counts, stream
block size, allocation calls, `allocated_bytes`, all three `samples_ns`, their
iteration counts, and `median_ns_per_iter`. Allocation calls include `alloc`,
`alloc_zeroed`, and `realloc`; bytes are cumulative requested sizes, including
the full requested size of each reallocation, **not peak or retained memory**.
Counting is process-wide for that one warmed operation, including worker
threads, and disabled for the latency samples. Rows also include architecture,
OS, active backend, requested `backend_override`, feature flags, metrics mode,
and thread counts. Before measurements, the probe records `git_revision` and
`git_dirty` from its source checkout; untracked files count as dirty, and
unavailable Git metadata is represented as `"unknown"` / `null`. `target_triple`
uses the existing benchmark `ARCH-OS-ENV` convention, with `unknown` when Cargo's
target environment is unavailable; it is not necessarily a canonical Rust
target triple. Record the compiler alongside the output, preserve the tested
diff for dirty runs, and compare matching `case_id` values under the same
configuration and resource isolation.

This JSONL schema includes distinct missing-shard patterns; it is not direct
input to `check_benchmark_regression.py`, whose case key omits that dimension.
These short samples help identify allocation changes and local regressions.
They do not establish cold-start cost, broad throughput claims, cross-machine
speedups, or application-level performance; use repeated controlled runs for
those conclusions.

The 2026-10-16 review and rejected-latency A/B/B/A evidence are recorded in
[the refactor review](erasure-codec-review-refactor-2026-10-16.md).

The profile now has 66 cases; `RSE_RECOVERY_CASE_FILTER=encode.` selects 21.
For adjacent same-process comparisons that share buffer addresses, see the
[NEON follow-up review and standalone probe](erasure-codec-neon-review-2026-10-16.md).
