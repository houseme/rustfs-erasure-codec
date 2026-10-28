# Parallel GF16 table-reuse diagnostic

This is an isolated prototype and functional work-count study based on
`c97b8b65702c9d5ad2ae84cf17e13467d92f63d8`. No prototype code is connected to
the production decoder. It is not timing, ABBA, native x86, or release evidence.

`prototype.patch` is self-contained: on little-endian AArch64 with `std`, a
serial or parallel multi-chunk recovery with at most 32 total shards prepares
one bounded, per-call set of distinct tables and borrows it read-only during
the synchronous recovery. Serial and parallel entrypoints use one preparation
helper; the tables are not stored in the global plan LRU.
Other shapes do not prepare tables. The prototype also extracts the shared
serial loop, so other targets can have different code generation even though
they do not use prepared tables. This is a design candidate, not a selected
implementation or a claim of improved throughput.

## Work counts

`baseline-instrumentation.patch` and `prototype.patch` each add a test-only
counter keyed by stage and multiplier. A dedicated four-thread Rayon pool marks
its own workers so other tests cannot contaminate the count. The maps and mutex
perturb execution and allocations; only construction counts are interpreted.
The test resets them before each recovery and checks every recovered byte.
Both overlays were independently applied to fresh source extractions; the
resulting source files matched the tested copies and reproduced the counts.

For GF16 `10+4` on AArch64, the 1 MiB shard has 16 chunks. Counts were the
same with `max_parallel_jobs` set to 1, 2, and 4:

| Missing indices | Baseline input / output builds | Prototype input / output builds | Distinct multipliers |
| --- | ---: | ---: | ---: |
| `0, 10` | 192 / 32 | 7 / 1 | 8 |
| `0, 1, 2, 3` | 96 / 64 | 2 / 1 | 3 |

The 64 KiB cases use one chunk and do not prepare tables. Counts alone do not
establish CPU cost, speedup, or behavior of other layouts and architectures.

The first diagnostic used a process-wide counter without restricting which
test threads contributed. Its full-suite run failed because unrelated tests
contributed multipliers. The dedicated-pool marker fixed the diagnostic; the
subsequent all-features run passed. This failure concerned instrumentation, not
an observed recovered-byte mismatch, and it is retained here rather than omitted.
The [observation transcript](observations.txt) retains all six work-count rows
per variant, the three allocation observations per variant and a summary of
the failed diagnostic run; it does not embed that run's full expanded log.

## Functional and resource checks

- The isolated prototype's all-features suite passed: 412 library tests,
  five ignored, plus integration and documentation tests. The `no_std` library
  suite passed 279 tests; the library Clippy check passed.
- Concurrent recoveries used one, automatic, two and four jobs while changing
  data/parity selection masks. Separate serial and parallel checks straddled the
  32/33-total-shard boundary. All checked requested bytes and unrequested state.
- The 9.0.0 Option cross-version test and 126-check `no_std` WASM probe passed
  against the prototype source. The probe was not run against a packaged crate.
- Offline QEMU AVX2 ran 39 GF16 library tests with one ignored and all four
  Option integration tests. QEMU without SSSE3/AVX2 ran the fallback tests and
  an Option mask test; the SSSE3-only test returned without executing. QEMU
  `no_std` ran 31 GF16 tests. These are simulated functional checks, not native
  x86 validation.

`allocation-two-jobs.patch` changes only the allocation test's job limit from
one to two. With `RAYON_NUM_THREADS=4`, warmed `10+4`, four missing 1 MiB data
shards and three independent processes per version, both the baseline and the
in-place prototype each reported 8,388,912 generic and 4,194,608 optimized
requested bytes in all three observations. The probe is process-wide and counts
requested bytes, not peak memory or latency; it does not prove identical behavior
under every worker schedule.

An earlier prototype that returned the inline table set by value produced one
generic observation of 9,503,024 bytes, 1,114,112 more than its baseline; its
other two observations matched. The difference equals 17 × 64 KiB, consistent
with a possible cold worker workspace, but thread ownership was not recorded,
so the cause is unresolved. `prototype-before-inplace.patch` retains that earlier
source, including its original unscoped test instrumentation. The outlier is
not evidence of either a performance improvement or a confirmed regression.

## Tracing the workspace outlier

`workspace-trace.delta.patch` adds test-only diagnostic exports and rewrites the
isolated prototype's workspace-cache acquisition into equivalent `empty`,
`incompatible` and `reused` branches, with atomic counters. It also captures
the number of new `DecodeBuffers` instances separately for the generic and
Option calls. None of this instrumentation is production code; the extra atomic
operations can change scheduling and are not suitable for timing.

In 21 independent instrumented processes using the same four-thread/two-job
fixture, 20 generic and all 21 Option observations reused two workspaces. One
generic observation instead recorded one empty thread-local cache, one reused
workspace and one new workspace. Its requested bytes were 9,503,024 rather than
8,388,912: the 1,114,112-byte difference equals 16 work lanes plus one scratch
lane at 64 KiB each. The Option call in that process reused two workspaces and
requested 4,194,608 bytes. Every recovery returned the original bytes. The
[trace transcript](cache-trace-observations.txt) retains all 21 observations.

This reproduces the earlier numeric outlier with a directly observed empty
cache and new workspace, but it does not prove the cause of the earlier
uninstrumented observation or why that worker cache was empty. Incompatible
cached buffers were not observed in these 21 runs. These counts describe only
the tested shape and scheduling; they neither establish a latency regression
nor justify changing the production cache.

`workspace-worker-trace.patch` is a separate self-contained overlay that also
records Rayon worker-index bitmaps for both warmups and measured calls. In 51
independent processes, 50 generic calls and all 51 Option calls reused both
workspaces. In one generic call, the
two warmups used bitmap `5` (workers 0 and 2); the measured generic call used
bitmap `3` (workers 0 and 1), and worker 1's cache was empty. That call built
one workspace and requested the same additional 1,114,112 bytes. The subsequent
Option call reused both workspaces. All 51 runs recovered the original bytes;
the complete compact rows are in [worker-trace observations](worker-trace-observations.txt).

This shows that the worker with the empty cache was not included in the two
recorded warmups for that shape. Pool indices are not OS thread identities, and
the atomic instrumentation can affect scheduling; neither this observation nor
its equality in size proves the cause of an earlier uninstrumented run. A
different worker schedule and cache state are valid outcomes, not failures to
discard or timing evidence.

The same worker-index instrumentation was then applied to the committed
baseline without the table-reuse prototype. In another 51 independent
processes, 50 generic calls and all 51 Option calls reused two workspaces. One
generic call again saw an empty cache on worker 1 after both warmups used
workers 0 and 2, and requested the same extra 1,114,112 bytes. All recovered
bytes matched. The [baseline observations](baseline-worker-trace-observations.txt)
retain every row, and `workspace-worker-baseline.patch` reproduces the tested
baseline instrumentation in isolation.

The baseline and prototype series were sequential, instrumented functional
samples, not a randomized or isolated performance experiment. Observing one
miss in each does not establish equal miss rates or exclude a scheduling effect
from the prototype. It does show that this cache-miss pattern can occur without
the prototype; it still cannot identify the cause of the earlier uninstrumented
outlier.

With Rust 1.99.1, AArch64 macOS and default release flags, the committed
`leopard_gf16_reconstruct` initial frame was 1,744 bytes. In the in-place
prototype it was 1,696 bytes; the serial and parallel prepared helpers had
initial frames of 4,384 and 4,400 bytes. No separate `prepare_scales` symbol
was found in that emitted assembly. These are compiler-specific individual
prologues, not a measured peak stack bound or proof of performance.

## Reproduction

Run on little-endian AArch64 from the repository root with dependencies cached:

```bash
repo=$(pwd)
base=$(mktemp -d)
candidate=$(mktemp -d)
git archive c97b8b65702c9d5ad2ae84cf17e13467d92f63d8 | tar -x -C "$base"
git archive c97b8b65702c9d5ad2ae84cf17e13467d92f63d8 | tar -x -C "$candidate"
git -C "$base" apply "$repo/benchmarks/recovery-table-work/2026-10-28/parallel/baseline-instrumentation.patch"
git -C "$candidate" apply "$repo/benchmarks/recovery-table-work/2026-10-28/parallel/prototype.patch"
for root in "$base" "$candidate"; do
  RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$root/target" \
    cargo test --offline --locked --manifest-path "$root/Cargo.toml" \
    -p rustfs-erasure-codec --lib \
    core::leopard_gf16::fused::table_work_probe::compare_parallel_table_work \
    -- --exact --nocapture --test-threads=1
done
```

For the separate two-job allocation check, apply `allocation-two-jobs.patch`
to each extraction and run `RAYON_NUM_THREADS=4 cargo test --release --offline
--locked -p rustfs-erasure-codec --test leopard_option_allocation -- --nocapture
--test-threads=1` with each extraction's manifest and target directory. Apply
`prototype-before-inplace.patch` instead of `prototype.patch` to reproduce the
earlier storage variant; do not apply both to one checkout. Retain all outcomes,
including cold-workspace differences. Confirm one work-count test ran rather
than treating zero selected tests as evidence.

To reproduce the workspace trace, apply `prototype.patch`, then
`allocation-two-jobs.patch`, then `workspace-trace.delta.patch` to a fresh
source extraction. Run the two-job allocation command in 21 separate processes
and preserve every outcome. Scheduling is nondeterministic; an outlier is not
guaranteed to recur. A test failure or a different cache-reason combination is
evidence to investigate, not a result to discard.

For the independent worker-index trace, apply only
`workspace-worker-trace.patch` to a fresh extraction and run the same two-job
allocation test in separate processes. Its worker bitmaps reserve bit 63 for an
unindexed caller or an index of at least 63; the recorded runs used only bits 0–3.
Apply only `workspace-worker-baseline.patch` to a separate fresh extraction for
the corresponding baseline trace.

The next candidate needs a bounded cache and stack review, native architecture
coverage, failure/concurrency tests, and controlled resource-isolated baseline
and ABBA measurements with the A2 drift gate. Until then the committed runtime
remains the reference.
