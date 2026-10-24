# Reusing survivor views after mixed Classic recovery

Runtime `7c94573` extends `873ce47` / runtime `7bf62d5`. The
[complete resource result](../benchmarks/release-9.1.0/2026-10-25/mixed-recovery-survivor-views.json)
retains all 135 comparisons, including every generic and adaptive bulk case.
The [previous target-index review](borrowed-recovery-targets-review-2026-10-25.md)
keeps its original 54-case source binding.

## Problem and change

Ordinary mixed recovery first restores missing data from the selected surviving
shards, then encodes missing parity from all data. The old executor built another
full reference table for that parity stage. The selected-input table already has
exactly the required length and capacity. Surviving systematic data forms its
sorted prefix; surviving parity references are needed only until data recovery
finishes. Merge recovered references backwards into this existing table, keeping
unread data references intact. The generic and concrete Option-vector executors
share this safe merge. No payload is moved or copied.

The merge starts after all data outputs succeed. Generic destinations retain
per-output initialization/computation, so a later invalid destination leaves the
earlier completed output valid. Destination owners outlive the merged references.
Composed selective parity, data-only and parity-only paths retain their original
execution. Field arithmetic, coefficients, inverse/cache keys, kernel dispatch,
policy decisions, error priority, output selection and public formats are unchanged.
No new unsafe code, API, allocator setting or shape-specific heuristic is added.

## Complete resource and compatibility checks

The new public-API regression genuinely fails on unmodified `873ce47`: every one
of the 135 cases requests two wide slice-reference tables. Runtime `7c94573`
requests one. Total caller-thread allocation requests decrease by exactly one in
every case; payload allocation in bulk recovery is still included. GF8/GF16,
three matrix modes, 33+33 / 40+40 / 64+64, five distinct data-erasure patterns and
one missing parity shard are covered. There are 90 preallocated generic ShardSlot
cases and 45 GF8 adaptive Option-vector cases. Inverse/backend warmup is outside
the tracked call. All recovered bytes and present flags are exact.

A separate unit check covers all 2,044 GF8/GF16 small erasure masks, checking
reference identity, systematic order and unchanged backing pointer/capacity.
The previous 54 target-borrow controls retain four requests per call.

Fresh stable validation on this runtime passes:

- 398 library, 166 integration and ten doc parent tests (574); six filtered child
  executions remain separate, with 43 existing ignored tests.
- Codec-only no_std: 269; Rust tooling: 317. Strict host codec/tool Clippy,
  formatting, spelling and diff checks pass.
- Published 9.0.0 local-default-backend compatibility: 200 non-fast configurations,
  27 custom / 162 cross recoveries, 12 wide GF16 configurations, and 270 selective /
  2,640 cross recoveries. Exact parity and bidirectional recovery pass.
- Fresh release WASM build/types and 14 Node tests: 12 actual module cases,
  one result-code contract and one facade unit, zero skipped.
- x86 Darwin all-target and x86 Linux library strict Clippy are compile-only.
  Actual core/WASM package inventories remain 71/6.

All validation input blobs match the committed runtime. Independent replay of
all original allocation logs matches the focused and full-test records exactly,
without running codec work. The 9/4/3 generated observer families are compared in
full: the only changed generated member is reconstruct.rs, and its complete delta
exactly equals the production delta. Current-source snapshots are updated after
that comparison; sampling templates, loops, counts and acceptance are unchanged.

## Release boundary

These are deterministic resource and correctness results, with zero timing
observations. They do not establish end-to-end throughput, RSS, physical isolation
or a unique explanation of older performance refusals. Previous sources and
failures remain intact. Dedicated Linux validation for this runtime, accepted
complete published-9.0.0 ABBA/paired-layout evidence, final source/tool/lock freeze
and strict positive release preflight remain outstanding. No tag, registry upload
or automatic merge is performed.
