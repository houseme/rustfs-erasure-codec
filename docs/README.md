# Documentation Index

This directory contains development documentation, design documents, benchmark results, and task tracking for the rustfs-erasure-codec project.

> Documents from the 2026 rewrite (by houseme) are primarily in **Chinese**, while older documents and some newer ones are in **English**.

## Quick References

| Document | Description |
|---|---|
| [ec-formal-delivery-summary-2026-05-28.md](ec-formal-delivery-summary-2026-05-28.md) | Formal delivery handoff summary |
| [ec-improvement-master-plan.md](ec-improvement-master-plan.md) | Master improvement plan (Chinese) |
| [benchmark-methodology.md](benchmark-methodology.md) | How to run and interpret benchmarks |
| [global-module-review-2026-10-26.md](global-module-review-2026-10-26.md) | Current module review, stream metadata consolidation, and validation limits |
| [caller-workspace-follow-up-2026-10-26.md](caller-workspace-follow-up-2026-10-26.md) | Generic parallel workspace reuse, resource evidence, and follow-up status |
| [global-review-9.1.0-2026-10-22.md](global-review-9.1.0-2026-10-22.md) | Archived earlier architecture review and bounded incremental-update resources |
| [drift-analysis-9.1.0-2026-10-22.md](drift-analysis-9.1.0-2026-10-22.md) | Current drift investigation, benchmark lifetime fixes and sparse incremental-update validation |
| [README-performance-index.md](README-performance-index.md) | Performance document index |
| [release-7.0.1-checklist.md](release-7.0.1-checklist.md) | 7.0.1 release checklist |
| [release-7.0.2-checklist.md](release-7.0.2-checklist.md) | 7.0.2 release checklist |

---

## A. Architecture & SIMD Design

### aarch64

| File | Description |
|---|---|
| [aarch64-simd-design.md](aarch64-simd-design.md) | aarch64 SIMD architecture design |
| [aarch64-source-code-analysis.md](aarch64-source-code-analysis.md) | Source code analysis for aarch64 |
| [aarch64-code-review-2026-05-26.md](aarch64-code-review-2026-05-26.md) | Code review results |
| [aarch64-simd-release-checklist.md](aarch64-simd-release-checklist.md) | Release checklist for aarch64 SIMD |

### x86_64

| File | Description |
|---|---|
| [x86_64-simd-runtime-dispatch-execution-guide.md](x86_64-simd-runtime-dispatch-execution-guide.md) | Runtime dispatch execution guide |
| [x86_64-simd-benchmark-summary-2026-05-26.md](x86_64-simd-benchmark-summary-2026-05-26.md) | Benchmark summary |
| [x86_64-simd-benchmark-summary-2026-05-26-amd-epyc-9v45.md](x86_64-simd-benchmark-summary-2026-05-26-amd-epyc-9v45.md) | Benchmark summary (AMD EPYC 9V45) |
| [x86_64-simd-gfni-design.md](x86_64-simd-gfni-design.md) | GFNI backend design |
| [x86_64-simd-release-checklist.md](x86_64-simd-release-checklist.md) | Release checklist |
| [x86_64-simd-verification-results.md](x86_64-simd-verification-results.md) | Verification results |
| [x86_64-simd-benchmark-ledger.md](x86_64-simd-benchmark-ledger.md) | Benchmark ledger |
| [x86_64-simd-ledger-entry-2026-05-26-amd-epyc-9v45.md](x86_64-simd-ledger-entry-2026-05-26-amd-epyc-9v45.md) | Ledger entry for AMD EPYC 9V45 |
| [x86_64-simd-final-delivery-summary.md](x86_64-simd-final-delivery-summary.md) | Final delivery summary |
| [x86_64-simd-second-gfni-machine-checklist.md](x86_64-simd-second-gfni-machine-checklist.md) | Second GFNI machine checklist |
| [x86_64-simd-second-gfni-machine-template.md](x86_64-simd-second-gfni-machine-template.md) | Second GFNI machine template |

---

## B. Benchmark Methodology & Results

### Methodology

| File | Description |
|---|---|
| [benchmark-methodology.md](benchmark-methodology.md) | Benchmark methodology and interpretation guide |
| [README-performance-index.md](README-performance-index.md) | Performance document index |
| [release-7.0.1-checklist.md](release-7.0.1-checklist.md) | 7.0.1 release checklist |
| [release-7.0.2-checklist.md](release-7.0.2-checklist.md) | 7.0.2 release checklist |

### Small-File Benchmarks

| File | Description |
|---|---|
| [ec-small-file-benchmark-playbook.md](ec-small-file-benchmark-playbook.md) | Small-file benchmark playbook |
| [ec-small-file-benchmark-results-2026-05-27.md](ec-small-file-benchmark-results-2026-05-27.md) | Small-file benchmark results |
| [ec-small-file-benchmark-results-2026-05-27-extended.md](ec-small-file-benchmark-results-2026-05-27-extended.md) | Extended small-file results |
| [ec-small-file-baseline-update-2026-05-27.md](ec-small-file-baseline-update-2026-05-27.md) | Baseline update |
| [ec-small-file-cross-arch-comparison-2026-05-27.md](ec-small-file-cross-arch-comparison-2026-05-27.md) | Cross-architecture comparison |

### Operation-Specific Results

| File | Description |
|---|---|
| [ec-decode-idx-benchmark-results-2026-05-28.md](ec-decode-idx-benchmark-results-2026-05-28.md) | `decode_idx` benchmark results |
| [ec-leopard-setup-benchmark-results-2026-05-28.md](ec-leopard-setup-benchmark-results-2026-05-28.md) | Leopard setup benchmark results |
| [ec-update-benchmark-results-2026-05-28.md](ec-update-benchmark-results-2026-05-28.md) | `update` benchmark results |
| [main-vs-origin-main-performance-2026-05-28.md](main-vs-origin-main-performance-2026-05-28.md) | main vs origin/main performance comparison |

---

## C. EC Improvement Roadmap

### Master Plan & Playbook

| File | Description |
|---|---|
| [ec-improvement-master-plan.md](ec-improvement-master-plan.md) | Master improvement plan (Chinese) |
| [ec-implementation-playbook.md](ec-implementation-playbook.md) | Implementation playbook |
| [ec-minio-compatibility-checklist.md](ec-minio-compatibility-checklist.md) | MinIO compatibility checklist |

### Klaus Post Alignment

| File | Description |
|---|---|
| [ec-klauspost-alignment-design.md](ec-klauspost-alignment-design.md) | Klaus Post alignment design |
| [ec-klauspost-alignment-index.md](ec-klauspost-alignment-index.md) | Klaus Post alignment index |
| [ec-klauspost-alignment-task-board.md](ec-klauspost-alignment-task-board.md) | Klaus Post alignment task board |

### Phase Documents

| File | Description |
|---|---|
| [ec-phase-1-baseline-and-regression.md](ec-phase-1-baseline-and-regression.md) | Phase 1: Baseline and regression |
| [ec-phase-2-api-and-config.md](ec-phase-2-api-and-config.md) | Phase 2: API and configuration |
| [ec-phase-3-parallel-boundary-design.md](ec-phase-3-parallel-boundary-design.md) | Phase 3: Parallel boundary design |
| [ec-phase-3-parallel-scheduler.md](ec-phase-3-parallel-scheduler.md) | Phase 3: Parallel scheduler |
| [ec-phase-4-simd-runtime-dispatch.md](ec-phase-4-simd-runtime-dispatch.md) | Phase 4: SIMD runtime dispatch |
| [ec-phase-5-reconstruction-and-cache.md](ec-phase-5-reconstruction-and-cache.md) | Phase 5: Reconstruction and cache |
| [ec-phase-6-selftest-release-governance.md](ec-phase-6-selftest-release-governance.md) | Phase 6: Self-test and release governance |
| [ec-phase-1-2-first-implementation-checklist.md](ec-phase-1-2-first-implementation-checklist.md) | Phase 1-2 first implementation checklist |
| [ec-phase-summary-2026-05-28.md](ec-phase-summary-2026-05-28.md) | Phase summary |

### Task Tracking

| File | Description |
|---|---|
| [ec-improvement-task-board.md](ec-improvement-task-board.md) | Improvement task board (Chinese) |
| [ec-unfinished-task-board.md](ec-unfinished-task-board.md) | Unfinished task board |
| [ec-formal-delivery-summary-2026-05-28.md](ec-formal-delivery-summary-2026-05-28.md) | Formal delivery summary |

---

## D. Leopard GF8 Codec

### Optimization & Planning

| File | Description |
|---|---|
| [leopard-gf8-optimization-roadmap-2026-05-30.md](leopard-gf8-optimization-roadmap-2026-05-30.md) | Optimization roadmap |
| [leopard-gf8-optimization-summary-2026-05-30.md](leopard-gf8-optimization-summary-2026-05-30.md) | Optimization summary |
| [leopard-gf8-dit4-adaptive-plan-2026-05-30.md](leopard-gf8-dit4-adaptive-plan-2026-05-30.md) | DIT-4 adaptive plan |
| [leopard-gf8-dit4-strategy-plan-2026-05-30.md](leopard-gf8-dit4-strategy-plan-2026-05-30.md) | DIT-4 strategy plan |
| [leopard-gf8-x86_64-verification-plan-2026-05-30.md](leopard-gf8-x86_64-verification-plan-2026-05-30.md) | x86_64 verification plan |
| [leopard-gf8-x86_64-execution-prompt.md](leopard-gf8-x86_64-execution-prompt.md) | x86_64 execution prompt |

### Backtest & Benchmark Results

| File | Description |
|---|---|
| [leopard-gf8-4m-backtest-2026-05-30.md](leopard-gf8-4m-backtest-2026-05-30.md) | 4M backtest |
| [leopard-gf8-adaptive-backtest-2026-05-30.md](leopard-gf8-adaptive-backtest-2026-05-30.md) | Adaptive backtest |
| [leopard-gf8-adaptive-backtest-round2-2026-05-30.md](leopard-gf8-adaptive-backtest-round2-2026-05-30.md) | Adaptive backtest round 2 |
| [leopard-gf8-backtest-final-2026-05-30.md](leopard-gf8-backtest-final-2026-05-30.md) | Final backtest |
| [leopard-gf8-neon-backtest-2026-05-31.md](leopard-gf8-neon-backtest-2026-05-31.md) | NEON backtest (aarch64) |
| [leopard-gf8-small-file-benchmark-2026-05-30.md](leopard-gf8-small-file-benchmark-2026-05-30.md) | Small-file benchmark |
| [leopard-gf8-x86_64-baseline-2026-05-30.md](leopard-gf8-x86_64-baseline-2026-05-30.md) | x86_64 baseline |
| [leopard-gf8-x86_64-comprehensive-benchmark-2026-05-30.md](leopard-gf8-x86_64-comprehensive-benchmark-2026-05-30.md) | x86_64 comprehensive benchmark |
| [leopard-gf8-x86_64-full-backtest-2026-05-30.md](leopard-gf8-x86_64-full-backtest-2026-05-30.md) | x86_64 full backtest |
| [leopard-gf8-x86_64-simd-results-2026-05-30.md](leopard-gf8-x86_64-simd-results-2026-05-30.md) | x86_64 SIMD results |

### Code Review & Safety

| File | Description |
|---|---|
| [leopard-gf8-code-review-2026-05-30.md](leopard-gf8-code-review-2026-05-30.md) | Code review |
| [leopard-gf8-unsafe-audit-2026-05-30.md](leopard-gf8-unsafe-audit-2026-05-30.md) | Unsafe code audit |
| [leopard-gf8-unsafe-perf-compare-2026-05-30.md](leopard-gf8-unsafe-perf-compare-2026-05-30.md) | Unsafe performance comparison |
| [leopard-gf8-refactor-perf-2026-05-30.md](leopard-gf8-refactor-perf-2026-05-30.md) | Refactor performance |

---

## E. Implementation Tasks (task-XX)

### Core Infrastructure (task-00 ~ task-08)

| File | Description |
|---|---|
| [task-00-platform-isa-split.md](task-00-platform-isa-split.md) | Platform ISA split |
| [task-01-backend-metadata-and-runtime-dispatch.md](task-01-backend-metadata-and-runtime-dispatch.md) | Backend metadata and runtime dispatch |
| [task-02-x86-avx2-modularization-and-stabilization.md](task-02-x86-avx2-modularization-and-stabilization.md) | x86 AVX2 modularization |
| [task-03-x86-ssse3-backend.md](task-03-x86-ssse3-backend.md) | x86 SSSE3 backend |
| [task-04-simd-c-legacy-and-build-rs-governance.md](task-04-simd-c-legacy-and-build-rs-governance.md) | SIMD-C legacy and build.rs governance |
| [task-05-x86-avx512-backend.md](task-05-x86-avx512-backend.md) | x86 AVX512 backend |
| [task-06-x86-gfni-backend.md](task-06-x86-gfni-backend.md) | x86 GFNI backend |
| [task-07-cross-backend-tests-and-bench-gates.md](task-07-cross-backend-tests-and-bench-gates.md) | Cross-backend tests and bench gates |
| [task-08-classic-aligned-allocation.md](task-08-classic-aligned-allocation.md) | Classic aligned allocation |

### API & Matrix Modes (task-09 ~ task-13)

| File | Description |
|---|---|
| [task-09-real-matrix-modes.md](task-09-real-matrix-modes.md) | Real matrix modes |
| [task-10-classic-parity-update-api.md](task-10-classic-parity-update-api.md) | Classic parity update API |
| [task-11-required-only-reconstruct-copy-elision.md](task-11-required-only-reconstruct-copy-elision.md) | Required-only reconstruct copy elision |
| [task-12-reconstruct-plan-unification.md](task-12-reconstruct-plan-unification.md) | Reconstruct plan unification |
| [task-13-progressive-decode-idx.md](task-13-progressive-decode-idx.md) | Progressive `decode_idx` |

### Leopard Codec (task-14 ~ task-30)

| File | Description |
|---|---|
| [task-14-leopard-codec-family-boundary.md](task-14-leopard-codec-family-boundary.md) | Codec family boundary |
| [task-15-leopard-gf8-flat-work-migration.md](task-15-leopard-gf8-flat-work-migration.md) | Flat work migration |
| [task-16-leopard-gf8-stage-plan-reuse.md](task-16-leopard-gf8-stage-plan-reuse.md) | Stage plan reuse |
| [task-16-minimal-intrusive-layout-refactor-plan.md](task-16-minimal-intrusive-layout-refactor-plan.md) | Minimal intrusive layout refactor |
| [task-17-leopard-gf8-copy-traffic-reduction.md](task-17-leopard-gf8-copy-traffic-reduction.md) | Copy traffic reduction |
| [task-18-leopard-gf8-group-traversal-partitioning.md](task-18-leopard-gf8-group-traversal-partitioning.md) | Group traversal partitioning |
| [task-19-leopard-gf8-group-schedule-metadata.md](task-19-leopard-gf8-group-schedule-metadata.md) | Group schedule metadata |
| [task-20-leopard-gf8-later-group-bookkeeping.md](task-20-leopard-gf8-later-group-bookkeeping.md) | Later group bookkeeping |
| [task-21-leopard-gf8-direction-reset.md](task-21-leopard-gf8-direction-reset.md) | Direction reset |
| [task-22-leopard-gf8-later-group-accumulation.md](task-22-leopard-gf8-later-group-accumulation.md) | Later group accumulation |
| [task-23-leopard-gf8-chunk-work-sizing.md](task-23-leopard-gf8-chunk-work-sizing.md) | Chunk work sizing |
| [task-24-leopard-gf8-threshold-work-slices.md](task-24-leopard-gf8-threshold-work-slices.md) | Threshold work slices |
| [task-25-leopard-gf8-work-slices-budget.md](task-25-leopard-gf8-work-slices-budget.md) | Work slices budget |
| [task-26-leopard-gf8-route-options.md](task-26-leopard-gf8-route-options.md) | Route options |
| [task-27-leopard-gf8-benchmark-decision.md](task-27-leopard-gf8-benchmark-decision.md) | Benchmark decision |
| [task-28-leopard-gf8-96x48-collapse.md](task-28-leopard-gf8-96x48-collapse.md) | 96x48 collapse |
| [task-29-leopard-gf8-remainder-topology-threshold.md](task-29-leopard-gf8-remainder-topology-threshold.md) | Remainder topology threshold |
| [task-30-leopard-gf8-remainder-path-followup.md](task-30-leopard-gf8-remainder-path-followup.md) | Remainder path followup |

---

## F. Code Review & Comparison

| File | Description |
|---|---|
| [main-vs-master-code-review-2026-05-27.md](main-vs-master-code-review-2026-05-27.md) | main vs master code review |
| [optimization-backtest-2026-05-30.md](optimization-backtest-2026-05-30.md) | Optimization backtest |

---

## G. Data Files

| File | Description |
|---|---|
| [leopard-gf8-adaptive-backtest-raw-2026-05-30.json](leopard-gf8-adaptive-backtest-raw-2026-05-30.json) | Adaptive backtest raw data |
| [leopard-gf8-adaptive-backtest-round2-raw-2026-05-30.json](leopard-gf8-adaptive-backtest-round2-raw-2026-05-30.json) | Adaptive backtest round 2 raw data |
