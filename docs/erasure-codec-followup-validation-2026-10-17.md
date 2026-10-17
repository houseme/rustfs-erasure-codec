# Codec follow-up implementation and validation — 2026-10-17

This continues the [completion audit](erasure-codec-completion-audit-2026-10-17.md) on `houseme/erasure-codec-review-fixes`. The current runtime is `9fcae450af8d98c5735e5c84bb42f28f9f018df8`, including main `60bb40c`. Encoding mathematics, shard payloads, public recovery semantics and SIMD activation limits remain unchanged.

[WASM #2852](https://github.com/rustfs/backlog/issues/2852) is accepted and closed. [DecodeIdx #2571](https://github.com/rustfs/backlog/issues/2571) has an explicit P2 evaluation decision, without claiming a new prepared API. [Performance #2853](https://github.com/rustfs/backlog/issues/2853) remains open. Correctness, compilation, drift checks and throughput acceptance are separate results.

| Commit | Delivered change |
| --- | --- |
| `50f3328` | Reserve progressive DecodeIdx missing-index storage by parity count. |
| `8b624ad` | Replace the handwritten WASM raw ABI with isolated complete generated bindings. |
| `efb1d64` | Reject empty/incomplete/unstable Galois ABBA campaigns and withhold attribution. |
| `c68c257` | Run real WASM and genuine core feature builds on the review branch; correct a hardware-dependent policy test. |
| `f137cac` | Add a pinned downstream benchmark with missing readers, plaintext oracle and observed sink. |
| `024d14d` | Validate Classic recovery lengths in one pass while retaining complete-layout fast returns. |
| `9fcae45` | Bound native byte-view metadata to the generated kernel shapes, keeping Leopard capacity independent. |

## Single-pass recovery correction

The initial downstream comparison found a modest degraded-read regression. Source review confirmed that generic Classic recovery first scanned every length for its complete-layout return, then scanned every length again to construct an erased layout. A 4 MiB object using 4+2 shards and 64 KiB blocks has 64 stripes and 16 KiB RSE shards, so that path repeated 384 length observations per object.

The new planner validates the present prefix, creates recovery metadata only on the first erasure, and continues the same iterator through the remaining shards. One length-validation state supplies the prefix, suffix and reused-workspace checks. All lengths are checked before quorum/inverse planning and destination initialization. Mask/count/length/quorum/inverse error order and the wide all-present zero-allocation path remain. There is no shape-specific branch, new public API, unsafe code or persistent cache.

An independent review traced generic, optimized, stream and workspace callers and found no blocker. New counted-container regressions verify actual recovered bytes and exactly one length observation for complete/first/middle/last-erasure layouts. Error tests retain late bad-length priority over insufficient survivors and prove that destinations remain unchanged. Existing exhaustive small-mask/matrix tests, singular matrices, late fixed-buffer errors, GF16 and wide allocation tests remain in the focused suite. Throughput impact is evaluated separately below.

## Bounded native metadata

The same checked byte-view constructor now accepts separate inline storage shapes: native kernels use 12 data/four parity views, covering every generated shape, while Leopard retains 32/32. On 64-bit targets the native inline slice payload shrinks from 1,024 to 256 bytes. This is a 768-byte metadata reduction, not a measured peak-stack claim. Prepared execution continues to borrow its metadata in place.

No payload copies, new dispatch branch, unsafe code, dependency or SIMD activation change was added. Tests enforce the prepared metadata budget, no spill for all six native shapes and wide Leopard layouts, and unchanged destinations on early/late invalid views. An independent review checked lifetimes, mutable borrows, no_std, field fallback and parallel boundaries without a blocker.

## Actual validation of `9fcae45`

[Final correctness evidence](../benchmarks/recovery-paths/2026-10-17-final-metadata-correctness.json) records the fixed source archive, actual feature lists and parent test totals; spawned child summaries are not counted twice.

| Execution | Result |
| --- | --- |
| Native Linux x86_64, Rust 1.99.1, core std + SIMD + metrics | **490 passed, 43 ignored**. |
| Local macOS ARM64, Rust 1.99.1, core std + SIMD + metrics | **487 passed, 43 ignored**. |
| Same host, core no_std + SIMD | **297 passed, 20 ignored**. |
| ARM64 Linux Docker, Rust 1.99.1, core std + SIMD + metrics | **487 passed, 43 ignored**. |
| Same ARM64 Linux VM, genuine core no_std + SIMD | **297 passed, 20 ignored**; executed library fingerprint excludes std/default/parking_lot/rayon. |
| Focused regressions | **11 Classic recovery tests** and **three native-view budget tests** passed. |
| Pinned RustFS release integration | **Four fixed compatibility fixtures passed** against the final codec source and the formal release target; lock/graph/executable invariants rechecked. |
| Latest real generated WASM | Locked release build and TypeScript check, **14 Node tests** and **8 system Chrome tests** passed. |
| Static checks | Workspace all-target/all-feature Clippy with warnings denied, formatting and diff checks passed. |

The ARM64 image and execution target are both ARM64 Linux on an ARM64 host. The container uses four CPUs/eight GiB, a read-only fixed-source tree, private Cargo/target storage, no network and an executable task-owned TMPDIR. All 358 source files have identical before/after digests and temporary containers were removed. This is VM functional validation, not dedicated ARM performance. The declared Rust 1.98.1 minimum was not reinstalled or rerun for this correction. [Intermediate single-pass results](../benchmarks/recovery-paths/2026-10-17-single-pass-correctness.json) retain the distinct 484/294 totals for `024d14d`.

The [test-count audit](../benchmarks/recovery-paths/2026-10-17-parent-test-count-audit.json) records the parent summary for every executable. The earlier workspace audit total was corrected from 491 to 486 because five nested child summaries had been added twice; its no_std total remains 291. The native Linux c68/024/9f totals are 484/487/490, corrected from 488/491/494 by excluding four child summaries in each run. Raw logs and exit statuses are unchanged.

Earlier runtime `c68c257` passed native Linux x86_64 **484 tests / 43 ignored** and pinned RustFS bridge22 + erasure59 + four fixture tests (**85 total**). Those results remain historical evidence and are not silently relabeled as tests of `9fcae45`. The native run first exposed a test assuming a 256 KiB recovery chunk; it now injects disabled/small/large L2 estimates and checks exact chunk/job decisions, without changing runtime scheduling.

## WASM and DecodeIdx acceptance

The [actual generated-binding CI job](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39005147311/job/117315970215) completed the Rust/WASM build, wasm-bindgen 0.2.129 generation and TypeScript check, then passed 14 Node and eight default Chromium tests for `c68c257`. Both [CI](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39005147311) and [Security Hardening Regression](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39005147307) succeeded for that runtime. The review branch is in the push filter and release preflight depends on real WASM integration. Core no_std jobs select the core package explicitly, preventing the WASM member from re-enabling std through feature unification. Final delivery-SHA CI is checked separately after push.

The Node suite has twelve tests that instantiate the processed WASM binary, one numeric-constant test and one facade-validation spy test. All eight browser cases use real generated WASM. The two Node unit tests are supplemental contract checks, not substitutes for generated-binding execution.

Each initializer owns a static factory around complete generated glue; imports, allocation, copy-back and memory-view refresh use the generator's ABI. Public initializer methods, numeric errors and data-only reconstruction remain. The ESM package has no external npm runtime dependencies; an offline production-only install of its packed artifact passed initialization and recovery. The old facade at `5647098` was run against the same processed module and failed with missing imports. Independent scalar parity checks and a fixed native 4+2 golden vector protect byte compatibility; tests also cover growth, repeated calls, concurrent instances and failed later modules.

The [DecodeIdx P2 assessment](decode-idx-workspace-evaluation-2026-10-17.md) retains all allocation observations and exploratory samples. Warm 4+2/10+4 calls were already zero-allocation. Reserving by parity count makes 32+16 batches zero-allocation and changes 64+16 from two requests/1,152 bytes to one/512 bytes per batch. The prepared-row prototype excluded preparation and had drift; its entire timing campaign is rejected. A new public workspace remains deferred pending an actual repeated-layout consumer and lifecycle evidence. The evaluation decision does not imply that API was implemented.

## Performance experiments and evidence boundaries

The ABBA analyzer now rejects empty/incomplete campaigns, duplicate rows, invalid estimates, conflicting revisions/backends and either-side drift. A failed campaign returns nonzero and nulls every candidate delta, including replacing stale PASS reports. All 61 Python tooling tests passed. This gate does not prove isolation or impose a new speedup threshold.

The pinned downstream consumer is RustFS `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`. Its disposable manifest retains the codec path patch so shadow-rs's nested bare `cargo tree` resolves the same source. The original checkout is untouched, `--locked` remains required, and the minimal lock difference is registry-codec to path-codec plus smallvec 1.16.2 to 1.16.3. An earlier CLI-only patch was lost by nested Cargo before one test invocation; that failed invocation was not counted as executed tests.

The [common benchmark overlay](../scripts/rustfs-streaming-decode-oracle-902c97c.md) uses deterministic nontrivial plaintext, healthy/missing-first-data readers, a complete untimed plaintext oracle and an observed allocation-free sink. Timed calls require no error and exact API/sink written counts. Both sides use the same overlay; old no-op-sink measurements are not comparable. Integrity-enabled cases are primary; verification-off cases are diagnostic controls.

### Initial native codec campaign: rejected

[All 232 observations](../benchmarks/recovery-paths/2026-10-17-linux-x86-vm/report.md) compare merge baseline `8f49bd8` with `c68c257` using the identical 58-case harness and lockfile, three 300 ms samples per case, four Rayon workers and an exclusive eight-vCPU VM cpuset covering a guest-reported complete L3. The codec job budget is automatic (`max_parallel_jobs=0`, eight visible CPUs), distinct from earlier 43-case/four-job observations.

All content/presence preflights passed. Fifty-one of 58 cases passed both 5% drift limits; seven baseline bookends failed, with maximum drift 19.28%. The whole campaign is rejected: every candidate ratio is null, including individually passing rows. Allocation bookends repeated, but the whole-campaign allocation-attribution gate remains false. No case was retried or selected as a favorable subset.

Runner assertions, all-root-cgroup checks, sampled thread affinity, no overlapping task compiler, owned-reservation deletion and parent/controller restoration were verified. Reported CPU steal was zero, but frequency/temperature and physical-host exclusivity were unavailable; the drift cause is unresolved. This result does not establish broader latency gains or regressions.

### Initial downstream campaign: degraded-read signal

[All 160 raw samples and confidence intervals](../benchmarks/recovery-paths/2026-10-17-rustfs-ecstore-vm/report.md) cover four cells in A1/B1/B2/A2: 4+2, 4 MiB objects and 64 KiB blocks through the modern in-memory ECStore path. Both builds retain opt3/thin LTO/one codegen unit, equal source/locks/features/compiler and empty actual Cargo rustflags. Every drift limit passed, all 48 registered untimed plaintext preflights passed, and runtime/isolation/cleanup checks passed. Actual Tokio workers=4 and visible CPUs=8; the resolved backend selection is recorded, not a per-kernel trace. Rayon is configured with four workers while its actual pool count is explicitly unobserved.

| Bitrot | Read mode | Baseline average phase median | `c68c257` average phase median | Candidate throughput change |
| --- | --- | ---: | ---: | ---: |
| Enabled (primary) | Healthy | 810.136 µs | 811.357 µs | -0.15% |
| Enabled (primary) | First data missing | 892.154 µs | 905.451 µs | **-1.47%** |
| Disabled (diagnostic) | Healthy | 223.033 µs | 222.882 µs | +0.07% |
| Disabled (diagnostic) | First data missing | 400.689 µs | 411.462 µs | **-2.62%** |

For both missing-data cells, B1/B2 median 95% confidence intervals exceed both baseline intervals; the primary case adds about 13.30 µs per object. Healthy intervals overlap. Raw sample tails overlap, so this is a profile-specific degraded-read signal. Allocation counts were not measured. The result was [recorded before the correction](https://github.com/rustfs/backlog/issues/2853#issuecomment-6230868487), and remains archived.

The four phases copied one immutable parent environment, but individual GET/locality I/O tuning values were not recorded. Defaults cannot be inferred retroactively or pooled with later explicit-I/O experiments. This reproducibility boundary prevents treating that campaign as complete #2853 acceptance, without discarding its recorded regression signal.

### Symbolized diagnosis and controlled correction validation

The VM supports software `cpu-clock:u`, while hardware `cycles:u` is unavailable. Formal release binaries are stripped. Separate builds preserve opt3/thin LTO/one codegen unit and disabled debug assertions while retaining symbols/line tables only for RSE and ECStore. The formal measured executables remain unchanged.

Both bounded software captures completed at 499 Hz with 8 KiB DWARF stacks. FIFO control gates events to the exact target Criterion profile window, excluding earlier/later plaintext preflights and including its warmup/reader setup. Resource isolation, correct runtime selection and cleanup passed. These profiles locate functions; they provide no speed attribution. The [hotspot aggregates](../benchmarks/recovery-paths/2026-10-17-rustfs-software-profile/hotspots.json) contain 7,495/7,437 samples with no lost or throttled records. About 55%/54% of all sampled leaves resolve to Highway append; generated AVX2 parity encoding accounts for about 11.6%/11.5%. Unresolved leaves are 18.13%/18.92%, and 87.14%/87.60% of samples contain an unresolved caller frame. Those limits prevent complete caller attribution or a per-function speed comparison. Positive metadata/source observations support the two structural changes; no arithmetic kernel was changed based on sampling noise.

New correction comparisons explicitly fix locality off, compatibility preference=false, stripe prefetch=1, bitrot/decode overlap=false and lockstep data-shards-only=false. They are separate controlled-I/O experiments: baseline `8f49bd8` to `9fcae45`, and previous runtime `c68c257` to `9fcae45`. The merged-main comparison covers all accumulated branch changes; only the previous-runtime comparison isolates the final two corrections. Neither isolates one correction alone. Each result is reported after complete measurement and both drift gates.

### Final runtime versus merged-main baseline

[All 160 controlled-I/O samples](../benchmarks/recovery-paths/2026-10-17-final-vs-main-vm/report.md) compare `8f49bd8` with `9fcae45`. All four phases, 48 untimed plaintext oracles, runtime checks and cleanup passed. All eight drift checks passed; maximum drift was 1.11%. The original formal baseline executable was retained.

| Bitrot | Read mode | Baseline average phase median | Final average phase median | Final throughput change |
| --- | --- | ---: | ---: | ---: |
| Enabled (primary) | Healthy | 811.888 µs | 811.306 µs | +0.07% |
| Enabled (primary) | First data missing | 887.835 µs | 901.348 µs | -1.50% |
| Disabled (diagnostic) | Healthy | 222.367 µs | 222.146 µs | +0.10% |
| Disabled (diagnostic) | First data missing | 399.322 µs | 409.096 µs | -2.39% |

Healthy confidence intervals overlap. Both missing-data cells have both candidate median 95% confidence intervals above both baseline intervals. The primary cell adds 13.51 µs/4 MiB: **the final structural changes have not eliminated the approximately 1.50% degraded-read throughput regression versus merged main**. The diagnostic cell remains 2.39% slower. These are accepted measurements of this narrow profile, not a successful performance-regression acceptance. Allocation counts were not measured.

### Final corrections versus previous runtime

[The separate 160-sample comparison](../benchmarks/recovery-paths/2026-10-17-final-vs-c68-vm/report.md) isolates `c68c257` to `9fcae45` under the same explicit I/O controls. All four phases, 48 untimed oracles, runtime/isolation checks and cleanup passed. All eight drift checks passed, with maximum drift 0.84%.

| Bitrot | Read mode | Previous average phase median | Final average phase median | Final throughput point estimate |
| --- | --- | ---: | ---: | ---: |
| Enabled (primary) | Healthy | 811.195 µs | 810.634 µs | +0.07% |
| Enabled (primary) | First data missing | 903.384 µs | 900.787 µs | +0.29% |
| Disabled (diagnostic) | Healthy | 221.466 µs | 220.884 µs | +0.26% |
| Disabled (diagnostic) | First data missing | 410.735 µs | 409.303 µs | +0.35% |

All four cells have overlapping phase median 95% confidence intervals; none has both candidate intervals fully below both baseline intervals. The +0.29%/+0.35% missing-data point estimates **do not establish a speedup**. No samples are pooled across campaigns. The single-pass observation and smaller metadata budget are verified structural results; their isolated latency effects are not established.

Both final campaigns retained all cases and samples without retries. A final task-process audit found no remaining build/benchmark/perf workload. Both owned CPU partitions were removed, root controllers and parent CPU availability were restored, and each runner checked restoration of its original launcher affinity.

The broader task remains open under [#2853](https://github.com/rustfs/backlog/issues/2853). VM CPU reservation does not establish dedicated ARM hardware throughput or S3 cluster performance, and no universal optimum is claimed.

Remaining performance acceptance is explicit: locate and remove the reproducible integrity-enabled degraded-read regression without weakening validation, obtain a valid complete codec campaign under changed/qualified measurement conditions, and keep dedicated ARM or S3 throughput claims separate from this Docker/component validation. The rejected 58-case campaign and negative merged-main result remain published evidence.
