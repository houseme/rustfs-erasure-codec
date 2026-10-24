# EC Small-File Baseline Update (2026-05-27)

## Summary

This update refreshes the small-file validation workflow and records a new
`x86_64-linux` extended baseline generated from current `HEAD`.

Key outcomes:

- added a dedicated small-file benchmark playbook
- aligned benchmark methodology docs with the real `quick` / `fast` / `extended` matrix
- extended regression tooling to support `ns_per_iter`
- added `verify_with_buffer` awareness to the regression checker
- added an optional small-file gate to `scripts/release-check.sh`
- archived a fresh `x86_64-linux` extended baseline

## New Baseline

Archived artifacts:

- `benchmarks/small-file/2026-05-27-x86_64-linux-extended.json`
- `benchmarks/small-file/2026-05-27-x86_64-linux-extended.csv`

Baseline metadata:

- commit: `9a1906e`
- target triple: `x86_64-linux-unknown`
- features: `std|simd-accel`
- backend: `rust-avx2`
- profile: `extended`
- iterations: `5`

## Why This Refresh Was Needed

The previously archived `aarch64` extended artifact was collected from
`git_revision=9d8c412` and did not include `verify_with_buffer`.

Current `HEAD` already benchmarks:

- `encode`
- `verify`
- `verify_with_buffer`
- `reconstruct`
- `reconstruct_data`

Without a refreshed baseline on current code, small-file regression checks would
have compared mismatched artifact shapes.

## Validation Notes

Verified locally:

- `cargo test --release --features "std simd-accel" --test benchmark_small_files benchmark_small_file_matrix_runs_and_exports_results -- --nocapture`
- `cargo xtask check-benchmark --baseline benchmarks/small-file/2026-05-27-x86_64-linux-extended.json --current target/benchmark-smoke/small-file-results.json --metric ns_per_iter ...`

The small-file regression checker passed with zero failures on the refreshed
baseline.

## Operational Guidance

Day-to-day validation:

```bash
RSE_SMALL_FILE_PROFILE=fast \
bash scripts/run_small_file_benchmark_matrix.sh
```

Extended baseline refresh:

```bash
RSE_SMALL_FILE_PROFILE=extended \
bash scripts/run_small_file_benchmark_matrix.sh
```

Release-style small-file gate:

```bash
VALIDATION_PROFILE=extended \
RUN_SMALL_FILE_GATE=1 \
RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_BASELINE=/abs/path/to/small-file-results.json \
./scripts/release-check.sh
```

## Cross-Architecture Context

See:

- `docs/ec-small-file-cross-arch-comparison-2026-05-27.md`

That comparison pairs the archived Apple Silicon extended run against the new
`x86_64-linux` extended baseline and uses `ns_per_iter` as the primary metric.
