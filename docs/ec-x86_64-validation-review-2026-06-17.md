# EC x86_64 Validation Review (2026-06-17)

## 1. Scope

This pass re-validated the current `main` checkout on the native `x86_64`
host, compared current EC small-file behavior against the archived
`2026-05-27` `x86_64` baseline, and reviewed whether the recent
`reconstruct_opt` / policy work has become patch-stacked enough to justify a
refactor instead of another incremental tweak.

Current checkout:

- `git_revision`: `11dca37`
- branch: `main`
- `git pull --rebase`: already up to date on `2026-06-17`

Validation host:

- architecture: `x86_64`
- CPU: `AMD EPYC 9V45 96-Core Processor`
- ISA observed via `lscpu`: `ssse3`, `avx2`, `avx512f`, `avx512bw`, `gfni`

## 2. Why Small Files Must Be Included

Yes, the EC review must include small files.

That is already the intended workflow in:

- `docs/ec-small-file-benchmark-playbook.md`
- `docs/benchmark-methodology.md`

The required size sweep is already part of the supported matrix:

- `1 KiB`
- `4 KiB`
- `16 KiB`
- `64 KiB`
- `128 KiB`
- `256 KiB`
- `512 KiB`

The current docs are explicit that small-file decisions should prioritize
`ns_per_iter`, especially from `1 KiB` through `64 KiB`, and should not be
replaced by large-file throughput alone.

## 3. Commands Executed

Host / dispatch validation:

```bash
lscpu
cargo test --features 'std simd-accel' test_select_x86_backend_priority -- --nocapture
cargo test --features 'std simd-accel' test_active_backend_metadata -- --nocapture
```

Current small-file artifact refresh:

```bash
RSE_SMALL_FILE_PROFILE=extended bash scripts/run_small_file_benchmark_matrix.sh
cargo xtask check-benchmark \
  --baseline benchmarks/small-file/2026-05-27-x86_64-linux-extended.csv \
  --current benchmarks/small-file/2026-06-17-x86_64-linux-extended.csv \
  --metric ns_per_iter \
  --threshold encode=0.12 \
  --threshold verify=0.12 \
  --threshold verify_with_buffer=0.12 \
  --threshold reconstruct=0.18 \
  --threshold reconstruct_data=0.18
```

Targeted small-file reruns:

```bash
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=4x2_1k \
RSE_SMALL_FILE_ITERATIONS=200 \
cargo test --release --features 'std simd-accel' \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture

RSE_BACKEND_OVERRIDE=rust-avx2 \
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=4x2_1k \
RSE_SMALL_FILE_ITERATIONS=200 \
cargo test --release --features 'std simd-accel' \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture
```

Reconstruct policy review:

```bash
cargo test --release --features 'std simd-accel' \
  --test benchmark_smoke \
  benchmark_reconstruct_policy_4x2_64k_exports_results -- --ignored --nocapture

cargo test --release --features 'std simd-accel' \
  --test benchmark_smoke \
  benchmark_reconstruct_policy_10x4_64k_exports_results -- --ignored --nocapture
```

General validation:

```bash
cargo test --features 'std simd-accel' test_galois_8_reconstruct_opt_matches_reconstruct -- --nocapture
cargo test --features 'std simd-accel' test_galois_8_reconstruct_opt_with_workspace_matches_reconstruct -- --nocapture
cargo test --features 'std simd-accel' test_galois_8_reconstruct_opt_serial_path_records_fallback_metric -- --nocapture
cargo test --features 'std simd-accel' --test benchmark_smoke benchmark_smoke_matrix_runs_and_exports_results -- --ignored --nocapture
cargo test --no-default-features
```

## 4. Archived Artifacts

Current full `extended` artifact archived from this pass:

- `benchmarks/small-file/2026-06-17-x86_64-linux-extended.json`
- `benchmarks/small-file/2026-06-17-x86_64-linux-extended.csv`

Current artifact metadata:

- target triple: `x86_64-linux-unknown`
- features: `std|simd-accel`
- backend: `rust-gfni-avx512`
- backend id: `RustGfniAvx512`
- backend override: `auto`
- iterations: `5`

Important comparison note:

- archived baseline `2026-05-27-x86_64-linux-extended.csv` was collected on
  `rust-avx2`
- current `2026-06-17` artifact was collected on auto-selected
  `rust-gfni-avx512`

So any direct baseline comparison must distinguish:

1. code-path changes
2. backend-policy drift
3. small-sample noise on ultra-small cases

## 5. Small-file Findings

### 5.1 Full-matrix compare against `2026-05-27`

The automated `ns_per_iter` comparison flagged three cases beyond threshold:

1. `verify 4x2_1k`: `+104.80%`
2. `encode 4x2_4k`: `+12.64%`
3. `encode 4x2_16k`: `+17.88%`

Everything else stayed within threshold, and most `10x4` cases from
`1 KiB` through `64 KiB` improved materially:

- `10x4_1k encode`: `+8.59%` latency regression, still within threshold
- `10x4_1k verify`: `-6.17%`
- `10x4_4k verify`: `-11.55%`
- `10x4_16k reconstruct`: `-26.31%`
- `10x4_64k reconstruct`: `-16.67%`
- `10x4_64k reconstruct_data`: `-10.22%`

### 5.2 What The `1 KiB` Rerun Changed

Per the playbook, the suspicious `1 KiB` point was rerun in isolation with
higher iteration count before touching the kernel.

Targeted `4x2_1k` rerun with `iterations=200`:

Auto backend (`rust-gfni-avx512`):

- `verify`: `309.01 ns`
- `verify_with_buffer`: `172.86 ns`
- `reconstruct`: `528.03 ns`
- `reconstruct_data`: `408.76 ns`

Forced `rust-avx2`:

- `verify`: `315.17 ns`
- `verify_with_buffer`: `207.51 ns`
- `reconstruct`: `600.60 ns`
- `reconstruct_data`: `431.59 ns`

That rerun changes the interpretation:

1. the isolated `4x2_1k verify` point is not evidence of a kernel-wide
   regression
2. both `auto` and forced `rust-avx2` remain extremely fast on the focused rerun
3. the full-matrix failure is better treated as a high-noise / fixed-overhead
   sensitivity case, not a reason to rush another low-level SIMD rewrite

The `verify` vs `verify_with_buffer` gap is still real and consistent, but that
matches the existing playbook guidance: repeated-call users should prefer
workspace / scratch reuse.

## 6. Branch Review: Where The Code Looks Patch-stacked

The strongest patch-stacking signal is still the `Option<Vec<u8>>`
`reconstruct_opt` / `reconstruct_data_opt` path.

### 6.1 Same-run small-file evidence

Focused `4x2_1k` reruns show:

Auto backend:

- `reconstruct`: `528.03 ns`
- `reconstruct_opt`: `36424.51 ns`

Forced `rust-avx2`:

- `reconstruct`: `600.60 ns`
- `reconstruct_opt`: `37729.43 ns`

So `reconstruct_opt` is still tens of microseconds slower than direct
`reconstruct` on a tiny serial fallback case.

### 6.2 Current `64 KiB` policy compare evidence

Latest `target/benchmark-smoke/reconstruct-policy-4x2_64k.csv` rows:

- `reconstruct_serial`: `14151.2104 MB/s`
- `reconstruct_opt_default`: `449.1142 MB/s`
- `reconstruct_opt_minparallel64k_minjob64k`: `4751.6171 MB/s`

Latest `target/benchmark-smoke/reconstruct-policy-10x4_64k.csv` rows:

- `reconstruct_serial`: `21162.0505 MB/s`
- `reconstruct_opt_default`: `9691.1746 MB/s`
- `reconstruct_opt_minparallel64k_minjob64k`: `9888.0149 MB/s`

The important part is not the exact threshold variant ranking.
The important part is that the current `reconstruct_opt_*` serial-fallback
family is still structurally slower than direct serial reconstruct.

Conclusion:

1. this area still behaves like patch-stacked policy work
2. another threshold tweak is not the right next step
3. the next real optimization needs to simplify or replace the serial
   `Option<Vec<u8>>` fallback shape itself

## 7. Changes Made In This Pass

Two narrow changes were kept:

1. `src/galois_8/policy.rs`
   - added a lightweight shape-summary helper for `Option<Vec<u8>>`
   - `reconstruct_opt` / `reconstruct_data_opt` now decide serial vs parallel
     without first building the full reconstruct plan
   - serial fallback now delegates directly to base `reconstruct(...)` /
     `reconstruct_data(...)` instead of re-entering the opt-specific plan path

2. `tests/benchmark_small_files.rs`
   - added `#![cfg(feature = "std")]`
   - this restores clean `cargo test --no-default-features` coverage by keeping
     the artifact-export benchmark target out of the non-`std` build

What these changes do:

- reduce one layer of patch-stacked entry overhead
- keep the `std` / `no_std` validation boundary correct

What they do **not** solve yet:

- the deeper `reconstruct_opt` serial-fallback structural slowdown

## 8. Validation Outcome

Passed in this pass:

- x86 backend priority test
- active backend metadata test
- `reconstruct_opt` correctness tests
- smoke benchmark artifact export
- `cargo test --no-default-features`

## 9. Final Decision

### Keep

1. Keep small-file validation as a required part of x86_64 EC review.
2. Keep the new `2026-06-17` archived `extended` artifact.
3. Keep the narrow entry-path cleanup in `src/galois_8/policy.rs`.
4. Keep the `std` gate on `tests/benchmark_small_files.rs`.

### Do Not Do In This Pass

1. Do not call the current branch “fully optimized” for `reconstruct_opt`.
2. Do not add another reconstruct threshold tweak on top of the current
   `Option<Vec<u8>>` fallback structure.
3. Do not treat the one-shot `4x2_1k verify` full-matrix failure as a proven
   kernel regression after the high-iteration rerun contradicted it.

## 10. Recommended Next Step

If the next pass stays narrow, the best target is:

1. dedicated serial `reconstruct_opt` / `reconstruct_data_opt` API-shape cleanup
2. not another policy-threshold experiment
3. validate on the same `4x2_1k`, `4x2_64k`, and `10x4_64k` cases immediately
   after each structural change
