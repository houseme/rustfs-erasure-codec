# Classic recovery metadata construction and validation — 2026-10-17

This continues the [first follow-up](erasure-codec-followup-validation-2026-10-17.md). The final frozen source is `0a2d5ee74c62a83059cea857fb6645477d433020`, including in-place scoped-plan construction `24dc15f` and borrowed row/destination metadata `0a2d5ee`. Main `60bb40c` remains included. [#2853](https://github.com/rustfs/backlog/issues/2853) tracks performance acceptance independently of the completed WASM integration and DecodeIdx assessment.

## Implementation and review

Classic recovery now fills its fresh scoped shape and plan directly in the execution frame. Owned workspace preparation uses the same shape/plan fillers and retains its existing owned result. Target index lists are extended in place. This changes construction lifetime and removes large owned Result return boundaries from repeated generic recovery; it adds no alternate algorithm, shape-specific branch, unsafe code, public API or new dependency.

The complete-layout return remains before plan construction. Mask/count/length/quorum/inverse validation and selected-output ordering remain; every present length is checked before inverse planning or destination initialization. Fresh internal plans cannot escape on failure. The error-order, singular-matrix, late fixed-buffer, exhaustive small-mask, GF16 and wide all-present zero-allocation regressions pass. Encoding matrices, shard bytes, cache policy and SIMD activation limits are unchanged. In the locked SmallVec 1.16.3 implementation, FromIterator itself delegates to a fresh vector plus extend; the new fillers retain that reservation strategy rather than adding a different capacity policy.

The investigation was [registered before implementation](https://github.com/rustfs/backlog/issues/2853#issuecomment-6234159156). Earlier symbolized `c68c257` consumer artifacts reserve 7,416 bytes of local stack in the relevant monomorph and contain eleven static memcpy call sites, including large metadata transfers. Those are static sites, not a count of operations on every request, a whole-stack peak, or proof that copying alone caused the prior 1.50% regression. Different profiles/contexts are not subtracted as causal evidence.

Canonical coefficient rows are now borrowed iterators. The generic serial executor zips them directly with borrowed destination references, avoiding temporary row-view arrays and moves of destination containers. Bulk and stream adapters collect only where contiguous slices are needed; exactly one parity slice is active, so chained slice-map size hints retain the original reservation behavior. Stream rows are still collected once during preparation, not per block.

The [row-view investigation](https://github.com/rustfs/backlog/issues/2853#issuecomment-6234552573) was registered before implementation. An end-to-end Classic 4+40 regression poisons forty retained parity destinations and checks every recovered byte and presence flag. Applied to archived `5d24068` with only the final test overlay, it reproduces four allocation requests; final `0a2d5ee` passes a three-request budget. This is a resource result, not a latency claim. An independent read-only review found no blocker in output-slice lifetimes, late errors, mixed/direct parity row order, fresh-plan invariants, wide reservation hints or generic/no_std bounds.

## Benchmark coverage

`5d24068` extends the common codec harness from 58 to 66 cases. Eight new cells exercise the ordinary generic `reconstruct_data` entry point used by the pinned ECStore consumer: 4+2/16 KiB and 10+4/32 KiB, each with one/two missing data shards and separate retained `ShardSlot` and `Option<Vec>` adapters. Retained destinations are poisoned before the complete preflight oracle; Option destinations are actually missing. Each new recovery cell checks complete bytes and presence after measurement. Vec replacement costs remain timed; retained storage is measured separately.

All 66 release smoke cases passed with a 1 ms sample window. The retained-slot cells allocate zero; Vec cells request the missing payload buffers. These are correctness/allocation smoke observations, without timing attribution.

The new complete codec campaign is a prespecified changed experiment: the same 66-case harness on both sides, three 1,000 ms samples per case instead of the earlier three 300 ms samples. It retains the original 58 cases. Every campaign remains A1/B1/B2/A2, with dual 5% drift gates and global null comparisons on failure. The earlier failed 58-case campaign is retained and never relabeled or replaced.

## Correctness

[Machine-readable evidence](../benchmarks/recovery-paths/2026-10-17-recovery-metadata-correctness.json) records the source archive, parent test counts and actual features. Only the last parent summary per executable is counted; repeated child summaries are excluded. [Per-executable count records](../benchmarks/recovery-paths/2026-10-17-recovery-metadata-test-counts.json) preserve the audit without adding nested-child results.

| Execution | Result |
| --- | --- |
| Local macOS ARM64, Rust 1.99.1, std + SIMD + metrics | 488 passed, 43 ignored. |
| Same host, core no_std + SIMD | 297 passed, 20 ignored. |
| Native Linux x86_64, Rust 1.99.1, std + SIMD + metrics | 491 passed, 43 ignored. |
| ARM64 Linux Docker, std + SIMD + metrics | 488 passed, 43 ignored. |
| Same ARM64 VM, genuine core no_std + SIMD | 297 passed, 20 ignored; actual library features exclude std/default/parking_lot/rayon. |
| Pinned RustFS release compatibility fixtures | Four passed; exact final source, lock/graph and formal artifact verified. |
| Focused Classic recovery suite | 12 passed. |
| Latest generated WASM | Locked release build, 14 Node tests and eight system Chrome tests passed. Node contains twelve real-WASM cases and two supplemental unit cases. |
| Static checks | All-feature workspace Clippy with warnings denied, formatting and diff checks passed. |

The Docker image and target are ARM64 Linux, with four CPUs/eight GiB, offline/no network, read-only source and an executable private TMPDIR. All 587 archived source files retain identical before/after digests; task containers were removed. This is VM functional execution, not dedicated ARM throughput. The declared Rust 1.98.1 minimum was not reinstalled for this change.

## Intermediate in-place-only results

The intermediate `5d24068` source passed native 490, local/ARM std 487 / no_std 297 and four pinned release fixtures. Both four-cell campaigns passed their eight drift checks, retained all 160 samples each and restored resources. [Main-baseline raw data](../benchmarks/recovery-paths/2026-10-17-inplace-intermediate-vs-main-vm/report.md) and [previous-runtime data](../benchmarks/recovery-paths/2026-10-17-inplace-intermediate-vs-previous-vm/report.md) retain exact raw file bytes and copy proofs.

| Bitrot / mode | `8f49bd8` to `5d24068` throughput | `9fcae45` to `5d24068` throughput |
| --- | ---: | ---: |
| Enabled / healthy | -0.06% | -0.34% |
| Enabled / first data missing | -0.53% | +0.56% |
| Disabled / healthy (diagnostic) | -0.13% | **-0.58%** |
| Disabled / first data missing (diagnostic) | -1.15% | +1.60% |

These columns are independent campaigns and are not pooled. In the main-baseline missing cells, A1/B1 median 95% confidence intervals partly overlap; not all phase pairs separate. In the `9fcae45` comparison, integrity-enabled missing/healthy intervals overlap; diagnostic missing has both candidate intervals below both baseline intervals, while diagnostic healthy has both above. The healthy diagnostic regression is retained. The intermediate 66-case campaign was built and dry-run successfully but deferred before any measurement while final `0a2d5ee` was prepared; it is not reported as an executed campaign.

## Controlled performance results

Results are inserted only after complete campaigns, provenance/cleanup checks and independent raw-sample recomputation. The codec and downstream builds use their own identical-within-comparison release profiles; their results are not pooled across profiles or experiments. The consumer remains RustFS `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`, with the same complete plaintext oracle/observed sink and five explicit I/O controls as the first controlled follow-up.

The final [main-baseline downstream campaign](../benchmarks/recovery-paths/2026-10-17-borrowed-metadata-vs-main-vm/report.md) passed all eight drift checks, full-content oracles, isolation, provenance and restoration checks. Root independently recomputed all sixteen phase medians from the 160 normalized raw samples and checked the recorded confidence bounds; all 64 original Criterion JSON files retain exact byte-copy proofs. The maximum observed bookend drift was 1.64%.

Integrity-enabled missing reads have baseline/candidate average phase medians of 890.28/892.48 µs, a +2.19 µs latency point estimate and -0.25% throughput point estimate. Integrity-enabled healthy reads are +0.26%; diagnostic healthy/missing are +0.37%/-0.77%. In both missing groups phase confidence intervals overlap, and no cell has both candidate intervals completely separated from both baseline intervals. These small point estimates do not demonstrate a stable speedup or regression. They are not subtracted from separate earlier experiments to claim a causal improvement.

The independent [previous-runtime campaign](../benchmarks/recovery-paths/2026-10-17-borrowed-metadata-vs-previous-vm/report.md), `9fcae45` to `0a2d5ee`, also passed all eight drift checks and restoration/provenance/oracle checks. Root recomputed all sixteen medians and 160 samples, and verified every raw-copy hash; maximum drift was 1.37%.

| Bitrot / mode | Main `8f49bd8` to final `0a2d5ee` | Previous `9fcae45` to final `0a2d5ee` |
| --- | ---: | ---: |
| Enabled / healthy | +0.26% | +0.58% |
| Enabled / first data missing | -0.25% | +1.14% |
| Disabled / healthy (diagnostic) | +0.37% | +0.46% |
| Disabled / first data missing (diagnostic) | -0.77% | +1.64% |

These are throughput point estimates from two independent campaigns. In the previous-runtime comparison, integrity-enabled missing reads changed from 903.01 to 892.79 µs; B1 and A1 median confidence intervals still overlap narrowly. Diagnostic missing B1/B2 intervals also overlap A2 narrowly. The integrity-enabled healthy cell has both candidate median intervals below both baseline intervals, while diagnostic healthy intervals overlap. The missing-read point estimates do not prove a stable speedup, and the table does not establish cross-machine or S3 throughput. No cross-campaign pooling or subtraction is used.

The complete [66-case native VM campaign](../benchmarks/recovery-paths/2026-10-17-borrowed-metadata-native-vm/report.md) produced all 264 rows and 792 latency samples, with every content oracle passing. Root independently checked all sample medians, 132 drift values, allocation bookends, workload/backend/build metadata invariants and the four exact raw-file copy hashes. All four phases passed VM cpuset/TID checks, with 197 complete thread samples each.

Seven cases fail at least one 5% drift gate:

| Case | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `classic.reconstruct_data_opt/10x4/32768/missing2+0/block0` | 1.72% | 7.11% |
| `classic.reconstruct_parity.cached/10x4/1024/missing0+1/block0` | 8.04% | 4.08% |
| `classic.reconstruct_some_parity/32x16/65536/missing1+1/block0` | 4.23% | 9.57% |
| `encode.serial/4x2/1024/missing0+0/block0` | 3.76% | 6.47% |
| `encode.serial/4x2/1048576/missing0+0/block0` | 2.25% | 6.90% |
| `encode.serial/8x3/1048576/missing0+0/block0` | 0.88% | 6.95% |
| `encode.serial/8x4/1048576/missing0+0/block0` | 5.93% | 0.26% |

The maximum baseline/candidate drift is 8.04%/9.57%. Whole-campaign latency **and allocation attribution are rejected**, every candidate comparison ratio remains `null`, and no cell was retried. Allocation bookends repeat in all cases, but this does not override the global gate. No passing subset is presented as a speedup or regression. The [independent audit](../benchmarks/recovery-paths/2026-10-17-borrowed-metadata-native-vm/root-independent-audit.json) preserves this decision. This is a new, prespecified 66-case/1,000 ms campaign; the historical failed 58-case/300 ms experiment remains intact.

All four phases report zero CPU steal; frequency and thermal observations were unavailable. VM partition checks therefore do not establish physical-host exclusivity or identify the cause of drift. The two separately accepted downstream profiles do not replace this rejected codec campaign.

[Final resource audit](../benchmarks/recovery-paths/2026-10-17-borrowed-metadata-native-vm/resource-audit.json) confirms no task build/benchmark/perf processes or reservations remain, and original controllers, parent CPU sets and launcher affinity are restored. Six protected old/new artifacts remain unchanged. The intermediate `5d24068` full campaign remains unstarted; measured campaigns were executed once each.

## Delivery and remaining scope

Final delivery-SHA CI is recorded in the linked backlog issues after actual push. Correctness, CI, VM resource isolation and throughput acceptance remain distinct. No dedicated ARM or S3 cluster throughput claim follows from these component results. #2853 and its parent #2708 remain open for stable complete codec acceptance and dedicated ARM throughput; the completed WASM integration and DecodeIdx assessment are not reopened by a timing rejection.
