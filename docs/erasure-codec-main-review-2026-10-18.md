# Main review and 9.1.0 preparation — 2026-10-18

Reviewed base: `26c10518f1fc8b4032a3f64886987bd6b9b66482`, including the PR #7
rebase merge. The [finding was recorded before implementation](https://github.com/rustfs/backlog/issues/2708#issuecomment-6236582226).
The reviewed scope includes effective format/clone identity, Classic planning and
serial/bulk/stream execution, generic field views, generated encode dispatch,
Leopard work-cache limits, error publication and release tooling.

## Structural conclusion

The current code is organized around one effective `EncodingFormat`, one Classic
shape/plan implementation and one checked native view constructor. Owned
workspaces and streams share that planning contract. Complete-layout calls skip
planning. Field capability checks replace element-size guesses. Retention is
bounded by the documented family/worker limits; runtime metrics and benchmark
instrumentation remain distinct.

Generic fallible outputs must finish one destination before proceeding to the
next, whereas concrete `Option<Vec>` outputs support fused kernels and bounded
parallel work. These execution adapters have different contracts; combining them
would either add bounds or weaken publication behavior. GF8/GF16 layouts and the
measured NEON activation boundary also remain explicit. Their presence is not
evidence that another runtime rewrite would improve performance. No other
blocking correctness issue was found in this reviewed scope.

## Verified performance defect and refactor

Generic serial recovery unconditionally retained every recovered-data slice,
even when no parity stage used it. This performed unnecessary work and spilled
an extra view array beyond 32 outputs. Data and parity loops also repeated the
same output initialization, length and error handling.

A single checked row executor now handles both stages. One consumer decision per
stage either consumes outputs immediately (data-only/composed parity) or retains
them for ordinary dependent parity. There is no per-output mode test, new unsafe
code, new public trait bound, alternative matrix or expanded SIMD route.

With only the final regression tests overlaid on the original main source,
**release builds** reproduced 6 data-only and 7 selective allocation requests;
the new maximum budgets are 5 and 6. The full mixed-recovery budget remains 7.
The same results were reproduced in debug. Tests poison retained destinations,
check every expected byte/presence flag, and preserve seven unrequested missing
data shards during selective parity. The existing late-buffer-error, validation
priority, singular matrix, exhaustive small-mask and GF16 cases also pass.
These are allocation observations, not peak-memory or throughput measurements.

## Compatibility and validation

[Machine-readable summary](../benchmarks/recovery-paths/2026-10-18-release91-review.json).
Local stable Rust 1.99.1/macOS ARM64: workspace 498 passed/43 ignored (core491 plus
WASM Rust7), independently selected no_std+SIMD 297/20, full-target/full-feature
Clippy and wasm32 checks pass. Fifteen focused Classic regressions pass. Counts
use only each executable's last parent summary; nested summaries are excluded.
Python gates: 79 passed.

The [standalone shared probe](../scripts/release-compatibility/README.md) links
the **actual published 9.0.0 crate** and the path candidate 9.1.0. Two hundred
cases compare every parity element and independently roundtrip one missing data
shard: Classic GF8/tower GF16, Leopard GF8/GF16, five layouts, four matrix options
and partial/vector-aligned lengths. Both versions enable simd-accel. Every case
passes by typed byte-vector equality; no digest-only equality or timing inference is used. This probe fixes
fast_one_parity=false. Existing cross-API tests cover the corrected fast mode;
legacy inconsistent fast/incremental/custom-field behavior is not treated as a
wire-format promise. Default parity and algorithm-family semantics are unchanged.
New Field hooks have defaults, public error variants are unchanged, and new
format/reader-writer APIs are additive. No MSRV declaration is reintroduced.

## Release boundary

9.1.0 core/workspace/lock/readme/changelog values are synchronized. WASM's separate
0.2.5 package version is not changed by the core release. Final distributable and
Node/Chromium evidence is recorded by the release PR CI after push.

No new throughput experiment was run. The prior full 66-case campaign remains
rejected after seven drift failures; no passing subset was rerun or substituted.
Dedicated ARM throughput is still unverified. This refactor is the simplest
validated removal of the demonstrated redundant work, not a universal optimality
claim. [#2853](https://github.com/rustfs/backlog/issues/2853) remains open. See the
[9.1.0 checklist](release-9.1.0-checklist.md) for actual publish prerequisites.
