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

The next candidate needs a bounded cache and stack review, native architecture
coverage, failure/concurrency tests, and controlled resource-isolated baseline
and ABBA measurements with the A2 drift gate. Until then the committed runtime
remains the reference.
