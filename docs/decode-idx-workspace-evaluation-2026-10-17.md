# DecodeIdx workspace evaluation — 2026-10-17

This completes the first P2 evaluation requested by [backlog #2571](https://github.com/rustfs/backlog/issues/2571), starting from `5647098`. The delivered change is a metadata-capacity correction in the existing API. No public DecodeIdx workspace API is added, and this evaluation does not complete [isolated throughput acceptance #2853](https://github.com/rustfs/backlog/issues/2853).

## Existing work and measured allocations

`decode_idx` already accumulates directly into caller-owned output buffers; it does not allocate whole-shard scratch. Its common 4+2 and 10+4 warmed paths already allocate nothing when inputs and outputs are preallocated. The older two-step benchmark clones inputs and allocates destinations, so it cannot isolate planning costs.

The independent [probe](../scripts/decode_idx_workspace_probe.rs) measures preallocated complete progressive recoveries and exact output bytes. Each test has two requested data or parity outputs. The inverse cache is warm, with `std + simd-accel`, metrics disabled and stable 1.99.1.

| Layout | Before, per batch | Delivered capacity fix, per batch |
| --- | ---: | ---: |
| 4+2 / 10+4 | 0 allocations | 0 allocations |
| 32+16 | 1 allocation / 384 requested bytes | 0 allocations |
| 64+16 | 2 allocations / 1,152 requested bytes | 1 allocation / 512 requested bytes |

For a 64-batch 64+16 recovery this reduces measured requests from 128 allocations / 73,728 bytes to 64 / 32,768. These are cumulative allocation requests on the measured thread, not retained or peak memory. Results for data and parity targets agree. All recovered bytes match the original codeword.

The cause was reserving the missing-index vector to total-shards even though a valid expected basis has exactly data-shards entries. Reserving parity-shards avoids a heap spill for 32+16 without changing matrix arithmetic, API shape or validation order. Invalid masks can still grow the vector before returning the existing error.

Two release regressions verify zero allocations across 32 progressive batches and preserve overcomplete/undercomplete expectation behavior, empty batches and absent outputs. Linux ARM64 execution and all-feature Clippy passed.

## Temporary workspace prototype and rejected timing evidence

A temporary prepared-row prototype stores the encoding format, expected basis, fixed output targets and coefficient rows. It retains no caller buffers or references. Repeated inputs retain the existing XOR behavior: supplying the same contribution twice cancels it; there is no implicit duplicate tracking.

The [machine-readable evidence](../benchmarks/decode-idx/2026-10-17-workspace-evaluation.json) retains all 24 allocation observations for each runtime version and the single 24-observation A1/B1/B2/A2 campaign. Repeated four-batch 4+2 allocation observations are retained explicitly. The timing campaign compares the original runtime with the temporary prototype, before the capacity fix. Twenty-three observations pass their per-row drift checks; one fails the baseline check at 9.87%. The entire timing campaign is rejected for candidate attribution. No failed observation was rerun, and no speedup is claimed.

Preparation was excluded from prototype timing, and the local host was unreserved. No timing campaign was run for the delivered capacity fix. The prototype's former 32+16 advantage includes allocations now removed by the simpler correction; it is not evidence of remaining workspace gains.

For disjoint input batches spanning one object, the original parity projection costs `O(requested_parity × data_shards²)` in total: each call projects only the columns present in that batch. Preparing every column upfront does not automatically reduce first-object arithmetic. A reusable plan could amortize projection and cache-lock costs across multiple objects with the same layout, but that lifecycle and preparation cost were not established here. Current repository callers are tests and benchmarks.

## Decision and compatibility boundaries

Keep the simple measured correction and the existing public API. A production workspace remains deferred until a real repeated-layout caller and a controlled lifecycle comparison establish its benefit. This is an evaluation decision, not an assertion that a prepared API was implemented.

A future plan should own the canonical format, expected source indices, inverse/matrix-derived rows and target mask, while preserving caller ownership and allowing different equal shard lengths between operations. It should avoid automatic TLS state, hidden duplicate suppression and copied shard data. Acceptance must include preparation, first-object latency, reused-object latency, full input/output validation, and both drift checks on reserved resources.

Existing semantics remain explicit: only Classic GF8 supports DecodeIdx; merge mode has no matrix planning; unexpected inputs are ignored for contributions but still length-checked. An overcomplete expected mask with active input/output returns `TooFewShardsPresent`, while an empty active batch or no outputs may return success earlier. The capacity correction preserves those contracts rather than silently redefining them.

To reproduce allocation comparisons, build the baseline and candidate libraries separately with identical release features and dependencies. Keep both rlibs in separate immutable locations, compile the identical probe with an explicit `--extern rustfs_erasure_codec=...` and matching `-L dependency=...`, then run `--allocations`. Do not select a latest rlib by filename or let Cargo overwrite the saved baseline. `--timing` is an exploratory sampler, not an isolation or performance acceptance gate.
