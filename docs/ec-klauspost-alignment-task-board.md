# EC Klauspost Alignment Task Board

## Status Legend

- `todo`: not started
- `doing`: active implementation
- `done`: implemented and validated
- `defer`: intentionally postponed

## Track 1: Classic Path Safe Improvements

### T1. Add aligned allocation helpers

- Status: `done`
- Task doc:
  - `docs/task-08-classic-aligned-allocation.md`
- Target files:
  - `src/core.rs`
  - `src/lib.rs`
  - `src/tests/mod.rs`
  - `README.md`
- Deliverables:
  - public aligned allocation API
  - tests for shard count, length, and zero-init behavior
  - docs that explain alignment benefit and compatibility neutrality
- Validation:
  - `cargo test`
  - `cargo bench --bench galois_backend --features simd-accel`

### T2. Implement real matrix modes

- Status: `done`
- Task doc:
  - `docs/task-09-real-matrix-modes.md`
- Target files:
  - `src/core.rs`
  - `src/matrix.rs`
  - `src/tests/mod.rs`
  - `README.md`
- Deliverables:
  - true `Cauchy`
  - true `JerasureLike`
  - explicit `Custom` semantics
  - compatibility notes per mode
- Validation:
  - golden-vector tests for default mode
  - dedicated tests showing non-default modes differ when expected

### T3. Update public docs for compatibility classes

- Status: `done`
- Target files:
  - `README.md`
  - `docs/ec-minio-compatibility-checklist.md`
- Deliverables:
  - classic-compatible vs alternative-codec wording
  - explicit warning that matrix changes alter output compatibility
- Validation:
  - doc review

## Track 2: Incremental Workflow APIs

### T4. Add parity update API

- Status: `done`
- Task doc:
  - `docs/task-10-classic-parity-update-api.md`
- Target files:
  - `src/core.rs`
  - `src/lib.rs`
  - `src/tests/mod.rs`
  - `benches/throughput_matrix.rs`
- Deliverables:
  - `update` API on classic GF(2^8) path
  - parity delta implementation
  - tests proving equivalence to full `encode`
- Validation:
  - targeted update tests
  - compare updated parity against full re-encode
  - add benchmark case for sparse updates

### T5. Add progressive decode API

- Status: `done`
- Task doc:
  - `docs/task-13-progressive-decode-idx.md`
- Target files:
  - `src/core.rs`
  - `src/lib.rs`
  - `src/tests/mod.rs`
- Deliverables:
  - `decode_idx` or equivalent naming
  - additive merge mode
  - docs for required zeroed destination buffers
- Validation:
  - multi-step reconstruct equivalence tests
  - error-shape tests for inconsistent inputs

## Track 3: Reconstruction Planner Cleanup

### T6. Introduce reconstruct planning helper

- Status: `done`
- Task doc:
  - `docs/task-12-reconstruct-plan-unification.md`
- Target files:
  - `src/core.rs`
- Deliverables:
  - internal plan struct for valid/invalid/missing output derivation
  - shared planner for serial and parallel paths
- Validation:
  - existing reconstruct tests
  - smoke benchmark pass

### T7. Remove unnecessary copying in required-only reconstruct

- Status: `done`
- Task doc:
  - `docs/task-11-required-only-reconstruct-copy-elision.md`
- Target files:
  - `src/core.rs`
  - `tests/benchmark_small_files.rs`
- Deliverables:
  - borrowed-input path for required-only reconstruct
  - retained no-partial-mutation guarantees on error
- Validation:
  - targeted reconstruct benchmarks
  - correctness equivalence tests

### T8. Evaluate one-pass unified output reconstruction

- Status: `defer`
- Target files:
  - `src/core.rs`
- Deliverables:
  - one-pass output planning for missing data and parity
  - preserve one/two-output optimized fast paths where they still win
- Validation:
  - A/B benchmark on `reconstruct`, `reconstruct_data`, `reconstruct_some`
  - regression gate check

## Track 4: Optional Alternative Codec Family

### T9. Design explicit codec-family selection

- Status: `done`
- Task doc:
  - `docs/task-14-leopard-codec-family-boundary.md`
- Target files:
  - `src/core.rs`
  - `src/lib.rs`
  - docs only in first pass
- Deliverables:
  - API sketch for classic vs Leopard families
  - compatibility notes in public docs
- Validation:
  - design review before code

### T10. Prototype Leopard GF8

- Status: `done`
- Target files:
  - new modules under `src/`
  - dedicated benchmark docs under `docs/`
- Deliverables:
  - opt-in Leopard GF8 path
  - isolated benchmark matrix
- Validation:
  - separate from classic-path regression gate

### T11. Evaluate Leopard GF16

- Status: `done`
- Deliverables:
  - feasibility report
  - explicit statement on compatibility tradeoffs

## Benchmark and Validation Checklist

Every change that touches hot paths should be validated with the smallest relevant command set:

1. `cargo test`
2. `cargo test --test benchmark_smoke --features "simd-accel benchmark-metrics" -- --nocapture`
3. `cargo bench --bench throughput_matrix --features "simd-accel benchmark-metrics"`
4. `cargo bench --bench galois_backend --features simd-accel`
5. `scripts/check_backend_consistency.sh` when backend-sensitive behavior changes
6. `scripts/run_small_file_benchmark_matrix.sh` when reconstruct or allocation behavior changes

## Recommended Implementation Order

1. Track backlog issue `rustfs/backlog#2571` for klauspost v1.14.2 hardening and validation status.
2. Keep P0 hardening complete: Leopard shard combination validation, Leopard family rejection in `decode_idx`, and regression tests.
3. Keep P1 compatibility complete: `reconstruct_some` / `reconstruct_some_opt` DataShards-length required mask support and current Clippy compatibility.
4. Defer T8/P2 one-pass reconstruct, stream reader/writer shape, and allocator/workspace experiments until benchmark evidence justifies the extra API or complexity.

## Merge Guidance

- Land Track 1 and Track 2 as small reviewable slices.
- Keep Track 3 benchmark-backed and isolated from API expansion.
- Keep Track 4 behind explicit options and separate benchmark reporting.
