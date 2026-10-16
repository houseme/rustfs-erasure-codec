# Codec state and recovery refactor review — 2026-10-16

The reviewed branch is `houseme/erasure-codec-review-fixes`. Merge commit
`8f49bd8b174fa776309219c55c697b5b7ad9f05c` incorporates main
`60bb40cffd85e7841be3a543f09ffe9a4aa025cf`. The frozen comparison baseline is
that merge, using the same dependency lockfile as the candidate. The delivered
runtime is `81a95f3441cf77f7c7c128f65bd3096806e54465`.

## Review and resulting structure

The cumulative branch review found overlapping state and execution paths that
were worth consolidating:

| Area | Result |
| --- | --- |
| Family state | The resolved codec family is the only algorithm selector. FFT codecs do not construct an unused Classic Vandermonde matrix or copy its parity rows. Logical GF8 setup-shape metadata remains compatible. |
| Native encoding | Serial and parallel entries share checked byte views and one ISA/shape selection. Execution borrows those views in place. Generated kernels cannot fall back after writing parity. Existing backend overrides, AVX2 detection, and measured NEON activation limits remain enforced. |
| Classic recovery | Generic, optimized, selective, workspace, and stream adapters share layout/matrix planning. One sorted missing-index list supplies data/parity partitions. |
| Recovery publication | Inverse planning finishes before any missing destination is initialized. Generic fallible buffers complete each output before touching the next destination; concrete Option<Vec> buffers retain bulk execution. |
| Selective parity | Composed survivor rows recover requested parity directly, without cloning all shards or publishing unrequested data. Parity-only recovery does not invert a matrix. |
| Complete layouts | A shared fast return validates every present length and skips shape/plan preparation. Wide layouts preserve zero-allocation behavior. |
| Parallel execution | One stage decision feeds the same bounded executor at every output width. Legacy one/two-output dispatch chains were removed; the public legacy specialized-call counter remains available and reports zero. |
| Benchmark oracle | Untimed poison/content/presence checks reject successful no-ops. Forty-three cases now include constructors, serial/parallel encoding, generic/optimized recovery, streams, and complete layouts. |

Two independent probes failed against the frozen baseline and passed against
the delivered runtime: a singular custom matrix must leave missing Option
buffers missing after an error, and an empty selective request must not try to
invert an unused singular matrix. New regression tests also cover small
recoverable masks across matrix modes, GF16 composition, late fixed-buffer
errors, wide layouts, policy budgets, workspace compatibility, and stream tails.

A final static review found no unresolved blocker in the changed Rust runtime
paths. This is a source review with the validation below, not proof of every
downstream integration or workload.

## Validation

Executed on current stable `rustc 1.99.1` unless stated otherwise:

- Workspace tests with `simd-accel benchmark-metrics`, excluding the fuzz runner.
- `no_std` tests with `simd-accel`; the existing NEON test-only thread-local
  capture was gated on `std` to make this feature combination compile.
- Workspace all-target/all-feature Clippy with warnings denied, including fuzz.
- Focused AddressSanitizer recovery/encoding/workspace/stream regressions on nightly.
- x86_64 macOS library and test compilation, x86_64 Linux library compilation,
  and wasm32 wrapper compilation.
- Both frozen-baseline correctness reproducers against the candidate.
- Eighteen Python gate tests, six JavaScript validation/mock-wrapper tests,
  formatting, and diff whitespace checks.

The Linux test cross-check could not build its `alloca` dev dependency without
a cross C compiler; Linux library compilation passed. No x86 runtime or real
browser/Node codec-module integration was executed. The current refactor was
not rerun with the declared Rust 1.98.1 minimum toolchain.

## Allocation evidence and timing limits

[Machine-readable observations](../benchmarks/recovery-paths/2026-10-16-refactor.json)
retain the initial draft, compacted draft, and final-runtime A1/B1/B2/A2 sequences,
including every latency sample and iteration count. Superseded draft observations
cannot establish behavior of the delivered runtime.

The final sequence uses release builds, the identical 43-case harness, SIMD with
metrics disabled, four Rayon threads, a four-job policy budget, three 300 ms
samples per case, twenty warmups, and twenty-second gaps between invocations.
No other Cargo build or benchmark was run during a sequence, but host resources
were not reserved. Only 1/43 final cells passes both 5% drift gates. The overall
latency comparison is rejected: neither a throughput improvement nor a latency
regression can be attributed from these runs. Further reruns on the same
unreserved host were stopped; isolated cross-architecture validation is tracked
in [backlog #2853](https://github.com/rustfs/backlog/issues/2853).

Allocation counts and bytes repeated in both A stages and both B stages for all
43 final cases. Representative cumulative requested allocation bytes for one
warm operation are:

| Case | Baseline bytes | Candidate bytes |
| --- | ---: | ---: |
| GF8 constructor, 10+4 | 2,069 | 1,073 |
| GF8 constructor, 32+16 | 10,241 | 1,073 |
| GF8 constructor, 128+64 | 141,953 | 1,073 |
| Uncached parity-only recovery, 10+4 / 1 KiB | 1,572 | 0 |
| Uncached parity-only recovery, 32+16 / 4 KiB | 6,192 | 0 |
| Selective parity, 32+16 / 64 KiB, one data + one parity missing | 3,146,928 | 0 |
| Complete generic/optimized layouts, 10+4, 40+8, 128+64 | 0 | 0 |

These are requested allocation bytes, not peak or retained memory. Constructors
create fresh codecs with shared tables/policy caches warm. Option<Vec> missing
buffer replacement is included in that adapter's measured cost; retained
ShardSlot cases have a different storage lifecycle. No downstream RustFS
throughput or universal optimum is claimed.

## Separate existing integration issue

Main already contains a TypeScript wrapper that manually calls an incompatible
wasm-bindgen raw allocator/slice ABI. Input-validation mocks and wasm32 compilation
do not validate real package initialization or JS-to-WASM-to-JS round trips.
The generated-binding migration and real Node/browser tests are tracked in
[backlog #2852](https://github.com/rustfs/backlog/issues/2852); this performance
refactor does not change that integration path.
