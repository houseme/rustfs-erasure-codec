# Borrowed parity matrix rows for Classic coding

Runtime `31ccf7e` is based on main `e758f11`. The
[complete resource result](../benchmarks/release-9.1.0/2026-10-25/borrowed-parity-rows.json)
retains all 270 before/after controls. This independent encoding change does not
include the unmerged recovery work in [PR #106](https://github.com/houseme/rustfs-erasure-codec/pull/106);
their validation sources remain separate.

## Problem and change

Each ordinary Classic encode, incremental encode and buffered verify collects
parity matrix rows into a SmallVec reference table. More than 32 parity rows spill
to the heap. Shard-by-shard encoding repeats that work for each input even though
the codec already owns the immutable matrix.

Return a checked borrowed matrix-row range instead. Static row access lets the
existing coding loops consume either that range or existing decoded-row slices
and fixed arrays. There is no trait object, new per-codec cache, reference-table
collection, coefficient copy or production unsafe code. Progressive decode's
parity lookup uses the same range; its arithmetic and destination rules stay intact.

Field/element bounds on public serial APIs, coefficient order, chunk boundaries,
policy decisions, metrics, native eligibility and checked byte-view preparation
are preserved. Parallel loops retain explicit Sync requirements. Decode and stream
callers still use their original slice views. There is no public API, format,
allocator setting, shard-size heuristic or acceptance-threshold change.

## Complete resource and compatibility checks

The new public-API test genuinely fails on unmodified `e758f11`. All 162 wide
controls record temporary allocations: one for ordinary encode/verify, or 4 / 10 /
33 for a full incremental cycle. All become zero on `31ccf7e`. The 108 small-shape
controls remain zero. GF8/GF16, three matrix modes, 4+2 / 10+4 / 4+33 / 10+40 /
33+64, and nine entry patterns are covered. Every output byte equals the complete
encoded codeword. All payloads and backend/policy warmup are outside the tracked
caller-thread window. Parallel API resource cases follow the existing small-buffer
serial policy; they do not claim parallel allocation or throughput improvements.

Separate checks cover every valid row interval in empty, zero-width, small and
wide matrices, with row pointer identity and range-end rejection. A component
check compares borrowed ranges and original decode views against an independent
scalar definition for GF8/GF16 and all three matrix modes, using serial execution
and a configured two-thread pool.

Fresh stable validation passes:

- 398 library, 165 integration and ten doc parent tests (573); six filtered child
  executions are separate; 43 existing ignored tests remain unchanged.
- Codec-only no_std: 268; Rust tooling: 317; strict host codec/tool Clippy,
  formatting, spelling and diff checks.
- Published 9.0.0 local-default-backend compatibility: 200 non-fast configurations,
  27 custom / 162 cross recoveries, 12 wide GF16, and 270 selective / 2,640 cross
  recoveries. Exact encoding bytes and bidirectional recovery pass.
- Fresh release WASM generation, TypeScript and 14 Node tests: 12 actual module
  cases, one result-code contract and one facade unit, zero skipped.
- x86 Darwin all-target and x86 Linux library strict Clippy are compile-only.
  Actual core/WASM package inventories remain 71/6.

The full tracked validation inputs and the separately bound new public regression
match the committed runtime. Independent replay of original allocation logs
matches the focused, full and separately bound runs exactly, without codec work.
All 9/4/3 generated observer members are compared in full. Only matrix.rs and
policy.rs change in the dynamic family, and their complete deltas exactly match
the production deltas. Current-source snapshots are updated after comparison;
formal sampling templates, loops, counts and acceptance rules remain unchanged.

## Release boundary

These are deterministic resource and compatibility results with zero timing
observations, not measured end-to-end throughput, RSS or a unique explanation of
historical refusals. The combined source with PR #106 still needs its own review
and validation after integration. Dedicated native Linux validation, accepted full
published-9.0.0 ABBA/paired-layout data, final source/tool/lock freeze and strict
positive preflight remain outstanding. Older source `4aa4ee0` records and its unlaunched
26-command continuation are not relabeled. No tag, registry upload or automatic
merge is performed.

## Windows debug allocation context

Original CI head `a012a78` fails the new total-allocation budget on Windows
stable, beta and nightly. Every job completes the same 270 controls and all
encoded-byte checks. The 120 nonzero totals correspond exactly to policy-query
counts; the other 150 totals are zero. These original failures remain preserved.

The existing debug policy hook reads `RS_PARALLEL_POLICY_DEBUG` on each query.
The inspected stable standard library's Windows getenv converts that key with
to_u16s into a Vec, including when the variable is absent. Test/workflow fix
`ad00e79` measures the unchanged policy calls independently, and requires the
coding entry to add zero allocations beyond that exact control. No arbitrary
platform allowance is used. Release still requires absolute zero for both the
control and the complete entry. Debug resource fixtures require logging disabled;
production logging semantics, runtime and package-input source bytes, and formal
sampling stay intact.

On an independent unmodified main source copy with only the corrected test
injected, both debug and release still reject all 162 old wide allocations and
retain 108 small zero controls. Locally the changed runtime passes all 270 cases
in both profiles, with zero policy/control/entry allocations on the host.

The fixed Windows jobs now pass the full std workspace suite and explicit debug
and release resource steps on all three toolchains. Each job records all 270
cases in each profile. Debug totals exactly match the independent policy controls
and original totals; coding extras are zero. Every release total/control is
absolute zero. The complete case records agree across all three toolchains;
[all 270 common records and per-toolchain outcomes](../benchmarks/release-9.1.0/2026-10-25/windows-parity-row-allocation-context.json)
are retained. Independent replay checks all original inputs unchanged without
running codec work, and requires unchanged compiler versions within each pair.

This closes the Windows test-accounting failure. It is source-bound functional
and resource evidence, without claiming end-to-end throughput, dedicated Linux
validation, a tested combination with PR #106 or release approval.
