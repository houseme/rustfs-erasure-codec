# 8.0.3 Release Checklist

This patch release ships RustFS compatibility hardening, klauspost/reedsolomon selective-recovery alignment, SIMD hot-loop cleanup, and ABBA benchmark governance. It does not change the public API shape or encoded data format.

## Version Freeze

- `Cargo.toml` package and workspace dependency versions are `8.0.3`.
- `Cargo.lock` root package version is `8.0.3`.
- `README.md` and `README_CN.md` installation examples use `8.0.3`.
- `CHANGELOG.md` contains the dated `8.0.3` entry.
- The release branch has a clean worktree before tagging.

## Required Validation

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo test --workspace --locked --features "simd-accel benchmark-metrics"
cargo clippy --workspace --all-targets --features "simd-accel benchmark-metrics" --locked -- -D warnings
git diff --check
bash scripts/check_backend_consistency.sh
bash scripts/run_aarch64_backend_smoke_matrix.sh
```

Backend benchmark governance:

```bash
RSE_ABBA_BACKEND=auto \
RSE_ABBA_SAMPLE_SIZE=20 \
RSE_ABBA_WARMUP_TIME=2 \
RSE_ABBA_MEASUREMENT_TIME=2 \
bash scripts/run_galois_backend_abba.sh <baseline-ref> <candidate-ref>
```

Treat performance conclusions as invalid when the A1/A2 baseline drift gate fails.

## Current Evidence

- PR #5 CI passed Linux, Windows, macOS ARM64, Linux ARM64, ppc64le VSX, ASan, cargo-audit, typos, and backend override regression jobs.
- Local aarch64 `auto` backend ABBA validation passed 3 consecutive drift-gated runs.
- x86_64 remote ABBA validation was intentionally inconclusive because repeated runs showed unstable A1/A2 baseline drift. Do not claim x86_64 performance improvement or regression from those measurements.

## Tag And Publish

After the release PR is merged and required validation is green:

```bash
git switch main
git pull --ff-only
git tag -a 8.0.3 -m "release: 8.0.3"
git push origin 8.0.3
```

Create the GitHub release and publish to crates.io only after the release-preflight workflow succeeds.
