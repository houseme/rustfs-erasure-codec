# EC Klauspost Alignment Document Index

## Purpose

This index turns the Klauspost alignment work into independent execution documents.
Use it as the entry point for planning, implementation, review, and validation.

Current main/9.1.0 review: [2026-10-18 review](erasure-codec-main-review-2026-10-18.md) and [release checklist](release-9.1.0-checklist.md).

Current Rust declaration review: [2026-10-17 compatibility validation](erasure-codec-rust-compatibility-2026-10-17.md).

Current PR/release readiness: [2026-10-17 release review](erasure-codec-release-readiness-2026-10-17.md).

Current recovery metadata follow-up: [2026-10-17 recovery validation](erasure-codec-recovery-metadata-validation-2026-10-17.md).

Earlier native/WASM follow-up: [2026-10-17 implementation and validation](erasure-codec-followup-validation-2026-10-17.md).

Earlier completion evidence and open acceptance boundaries: [2026-10-17 audit](erasure-codec-completion-audit-2026-10-17.md).

## Read Order

1. Read the overall design:
   - `docs/ec-klauspost-alignment-design.md`
2. Read the MinIO compatibility boundary:
   - `docs/ec-minio-compatibility-checklist.md`
3. Use the task board to pick the next implementation slice:
   - `docs/ec-klauspost-alignment-task-board.md`
4. Open the matching task document below and execute it as a standalone work item.

## Task Documents

### Classic-path safe improvements

- `docs/task-08-classic-aligned-allocation.md`
  - Add aligned allocation helpers without changing output semantics.
- `docs/task-09-real-matrix-modes.md`
  - Make `MatrixMode` behavior real and document compatibility impact.

### Incremental workflow APIs

- `docs/task-10-classic-parity-update-api.md`
  - Add sparse-update parity maintenance with byte-identical output.
- `docs/task-13-progressive-decode-idx.md`
  - Add incremental decode/reconstruct support for classic GF(2^8).

### Reconstruction planner and hotspot cleanup

- `docs/task-11-required-only-reconstruct-copy-elision.md`
  - Remove avoidable copies in required-only recovery paths.
- `docs/task-12-reconstruct-plan-unification.md`
  - Introduce a shared reconstruction planner and unify output planning.

### Alternative codec family and explicit compatibility separation

- `docs/task-14-leopard-codec-family-boundary.md`
  - Define how Leopard GF8/GF16 should be introduced without polluting the classic path.

## Recommended First Execution Order

1. `task-08-classic-aligned-allocation.md`
2. `task-09-real-matrix-modes.md`
3. `task-10-classic-parity-update-api.md`
4. `task-11-required-only-reconstruct-copy-elision.md`
5. `task-12-reconstruct-plan-unification.md`
6. `task-13-progressive-decode-idx.md`
7. `task-14-leopard-codec-family-boundary.md`

## Review Rule

For any task that touches encoding behavior, re-check:

- `docs/ec-minio-compatibility-checklist.md`

If a task changes matrix semantics, codec family, shard order, or padding behavior, it must be treated as a
compatibility-sensitive change even if benchmark results look good.
