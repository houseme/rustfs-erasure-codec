# EC Small-File Benchmark Playbook

## 1. Purpose

This playbook defines how to validate EC behavior for latency-sensitive small-file workloads.

It exists to answer three practical questions:

1. Are we still covering realistic small shard sizes such as `1 KiB` through `512 KiB`?
2. Did a code change regress small-file latency even if large-file throughput still looks healthy?
3. Should we optimize implementation details, or do we only need to refresh baselines and docs?

## 2. Why Small Files Need Dedicated Coverage

Small-file EC behavior is not well represented by throughput-heavy `1 MiB+` cases alone.

For `1 KiB`, `4 KiB`, `16 KiB`, and nearby sizes, fixed costs dominate more often:

- setup and dispatch overhead
- temporary buffer allocation or reuse behavior
- reconstruction path bookkeeping
- cache warmup effects
- backend selection overhead

Because of that, small-file validation should prioritize `ns_per_iter` first and
use `throughput_mb_s` as a secondary view.

## 3. Source Of Truth

Current implementation lives in:

- `tests/benchmark_small_files.rs`
- `scripts/run_small_file_benchmark_matrix.sh`
- `docs/benchmark-methodology.md`

If this playbook conflicts with old archived CSV or JSON files, prefer the current
test implementation and regenerate artifacts on the current commit.

## 4. Profiles And Covered Cases

The small-file matrix currently supports three profiles via `RSE_SMALL_FILE_PROFILE`.

`quick`

- `4+2`
- `1 KiB`
- `4 KiB`
- `16 KiB`
- `64 KiB`

`fast`

- `4+2`: `1 KiB`, `4 KiB`, `16 KiB`, `64 KiB`, `128 KiB`, `256 KiB`, `512 KiB`
- `10+4`: `16 KiB`, `64 KiB`, `256 KiB`, `512 KiB`

`extended`

- `4+2`: `1 KiB`, `4 KiB`, `16 KiB`, `64 KiB`, `128 KiB`, `256 KiB`, `512 KiB`, `1 MiB`
- `10+4`: `1 KiB`, `4 KiB`, `16 KiB`, `64 KiB`, `128 KiB`, `256 KiB`, `512 KiB`, `1 MiB`

This means the requested sizes `1 KiB / 4 KiB / 16 KiB / 64 KiB / 128 KiB / 256 KiB / 512 KiB`
are already part of the supported validation path.

## 5. Operations To Compare

The current small-file benchmark exports:

- `encode`
- `verify`
- `verify_with_buffer`
- `reconstruct`
- `reconstruct_data`

Interpretation guidance:

- `verify_with_buffer` is the preferred repeated-call path when callers can reuse scratch memory.
- If `verify_with_buffer` materially outperforms plain `verify` for small files, that is usually an API usage signal, not automatically a kernel bug.
- `reconstruct` and `reconstruct_data` are expected to look weaker than `encode`/`verify` at ultra-small sizes because fixed control overhead is a larger share of total work.

## 6. Standard Commands

Fast local validation:

```bash
RSE_SMALL_FILE_PROFILE=fast \
bash scripts/run_small_file_benchmark_matrix.sh
```

Full baseline refresh:

```bash
RSE_SMALL_FILE_PROFILE=extended \
bash scripts/run_small_file_benchmark_matrix.sh
```

Extended release-style gate:

```bash
VALIDATION_PROFILE=extended \
RUN_SMALL_FILE_GATE=1 \
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_BASELINE=/abs/path/to/small-file-results.json \
./scripts/release-check.sh
```

Single-case drill-down:

```bash
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=4x2_1k,4x2_4k,10x4_1k \
RSE_SMALL_FILE_ITERATIONS=12 \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture
```

## 7. Output Artifacts

The benchmark writes:

- `target/benchmark-smoke/small-file-results.json`
- `target/benchmark-smoke/small-file-results.csv`

When archiving a meaningful result, store a copy under:

- `benchmarks/small-file/`

Recommended naming:

- `YYYY-MM-DD-<target>-<profile>.json`
- `YYYY-MM-DD-<target>-<profile>.csv`

## 8. Comparison Rules

Before drawing conclusions:

1. Confirm baseline and current artifacts were generated from comparable commits or from an intentional before/after pair.
2. Confirm the same feature set and backend selection policy were used.
3. Confirm whether the artifacts include `verify_with_buffer`; older archived data may not.
4. Prefer comparing the same profile level, especially for `fast` vs `fast` or `extended` vs `extended`.
5. Remember that the matrix entrypoint is marked `#[ignore]`; direct `cargo test` commands must include `--ignored`.

For small-file regressions:

1. Use `ns_per_iter` as the primary metric.
2. Treat `throughput_mb_s` as supporting evidence only.
3. Focus first on these cases:
   - `4+2`: `1 KiB`, `4 KiB`, `16 KiB`, `64 KiB`
   - `10+4`: `1 KiB`, `4 KiB`, `16 KiB`, `64 KiB` in `extended`
4. If only `1 MiB` improves while `1 KiB` to `64 KiB` regresses, do not call that a small-file win.
5. If an apparent regression is isolated to `1 KiB` or `4 KiB`, rerun the case with a higher iteration count before touching the kernel.

Suggested rerun:

```bash
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=10x4_1k \
RSE_SMALL_FILE_ITERATIONS=40 \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture
```

On the `AMD EPYC 9V45` `x86_64` validation host, one-shot full `extended` runs
can overstate regressions for suspicious points. If the first full-matrix pass
looks bad, prefer a filtered rerun with higher iterations before touching the
kernel.

## 9. Automated Regression Checks

Throughput-oriented comparison:

```bash
cargo xtask check-benchmark \
  --baseline /abs/path/to/old-small-file-results.json \
  --current target/benchmark-smoke/small-file-results.json \
  --require-case encode:4:2:1024 \
  --require-case verify:4:2:4096
```

Latency-oriented small-file comparison:

```bash
cargo xtask check-benchmark \
  --baseline /abs/path/to/old-small-file-results.json \
  --current target/benchmark-smoke/small-file-results.json \
  --metric ns_per_iter \
  --threshold encode=0.12 \
  --threshold verify=0.12 \
  --threshold verify_with_buffer=0.12 \
  --threshold reconstruct=0.18 \
  --threshold reconstruct_data=0.18 \
  --require-case encode:4:2:1024 \
  --require-case verify_with_buffer:4:2:4096 \
  --require-case reconstruct:4:2:16384 \
  --require-case reconstruct_data:10:4:65536
```

For `ns_per_iter`, a regression means latency increased.
For `throughput_mb_s`, a regression means throughput dropped.

The release check uses latency mode by default for the small-file gate.

## 10. When Optimization Is Actually Needed

Treat a result as optimization-worthy when one or more of these are true:

- the current commit regresses `ns_per_iter` beyond threshold across multiple small-file sizes
- `verify_with_buffer` also regresses, which weakens the “just reuse workspace” explanation
- only small-file cases regress while medium or large cases stay flat
- the regression is reproducible across repeated runs with stable backend selection

Do not rush into kernel changes when:

- the data came from an older commit that predates current benchmark coverage
- the only issue is that docs or archived baseline files are stale
- the observed gap is between `verify` and `verify_with_buffer`, because that may simply reflect expected scratch-buffer reuse benefits

## 11. Recommended Next Step After Any Benchmark Change

1. Run `fast` locally while iterating.
2. Run `extended` before refreshing an archived baseline.
3. Archive the JSON and CSV together.
4. Record commit hash, target triple, features, and backend in the associated note or task update.
5. If the benchmark shape changed, update this playbook and `docs/benchmark-methodology.md` in the same change.
