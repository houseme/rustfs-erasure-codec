# GF16 decode table-construction work count

This is a functional work-count diagnostic against clean source commit
`c97b8b65702c9d5ad2ae84cf17e13467d92f63d8`. It is not a latency benchmark,
an ABBA comparison, a production profile, or release evidence.

The adjacent patch records a test-only per-thread count by multiplier and stage
immediately before each NEON fused-kernel table build outside the `MODULUS16`
fast path. It also adds
one test that encodes
and recovers complete GF16 `10+4` shards, checks every restored byte, and uses
`max_parallel_jobs=1`. The test resets the counter immediately before each
decode. It does not instrument butterfly multiplications, non-NEON platforms,
or the `MODULUS16` fast-path conversions. The diagnostic's maps and counters
perturb the test binary. A `log_m` of zero is also an identity but still uses
the table path and is included if it occurs.

Observed on AArch64 macOS with stable Rust:

| Missing indices | 64 KiB input/output builds | 1 MiB input/output builds | Distinct multipliers across both stages |
| --- | ---: | ---: | ---: |
| `0` | 12 / 1 | 192 / 16 | 13 |
| `0, 10` | 12 / 2 | 192 / 32 | 8 |
| `0, 1, 2, 3` | 6 / 4 | 96 / 64 | 3 |

For these inputs the erasure pattern and coefficients are unchanged across
chunks, so construction count scales with chunk count. Some multipliers also
repeat across different shards or stages. This confirms repeated work, not its
CPU share or the value of caching it. It does not prove results
for other shard counts, cache states, concurrent calls or architectures.

To repeat without changing the checkout:

```bash
repo=$(pwd)
scratch=$(mktemp -d)
git archive c97b8b65702c9d5ad2ae84cf17e13467d92f63d8 | tar -x -C "$scratch"
git -C "$scratch" apply "$repo/benchmarks/recovery-table-work/2026-10-28/instrumentation.patch"
RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$scratch/target" \
  cargo test --offline --locked -p rustfs-erasure-codec \
  --manifest-path "$scratch/Cargo.toml" --lib \
  core::leopard_gf16::fused::table_work_probe::compare_table_build_work_across_chunk_counts \
  -- --exact --nocapture --test-threads=1
```

Run from the repository root on little-endian AArch64 with the dependencies
already cached. Confirm that **one test** ran; zero selected tests are not
evidence. The temporary directory is intentionally left to the operator.

Any candidate to reuse tables needs bounded memory, correct behavior for
identity coefficients, short tails, serial and parallel execution, and `no_std`.
Its costs and gains need controlled baseline checks followed by an isolated
ABBA comparison and the A2 drift gate; this diagnostic does not authorize that
attribution.

A candidate design could lazily prepare unique non-sentinel tables after a
multi-chunk request, with immutable per-shard indices into that bounded table
set. The current plan cache key contains shard counts and presence, not the
requested-output mask, so selected output coefficients cannot be cached as if
the first request's mask applied to later requests. An implementation must also
account for active `Arc` plans surviving LRU eviction, avoid holding the cache
mutex while preparing tables, retain fallback behavior for large shapes, and
measure cold-plan and single-chunk costs. This is a design direction, not an
implemented or accepted optimization.

## Isolated per-call prototype

`prototype-inline.patch` is an experimental overlay against the same `c97b8b6`
source. It includes the work-count instrumentation and, only for serial
little-endian AArch64 `std` recovery, prepares distinct multiplier tables once
per call when the shard spans multiple chunks and the total shard count is at
most 32. Other configurations retain the committed path. This is deliberately
not production code: the threshold, linear table lookup and per-call lifetime
are hypotheses rather than a selected design.

The same three 1 MiB cases constructed 13, 8 and 3 tables respectively, while
the 64 KiB cases remained at 13, 14 and 10. Full restored bytes were checked.
The Option, Leopard recovery and GF16 decode-reuse integration tests passed in
release mode, and 31 `no_std` GF16 library tests passed. They do not cover a
native x86 implementation or make a latency claim.

Two storage choices were examined with the same prototype:

| Storage | 10+4, 1 MiB, four missing data shards: generic / Option requested allocation bytes | AArch64 release initial stack frame |
| --- | --- | --- |
| Committed baseline | 8,388,608 / 4,194,304 | 1,744 bytes |
| Per-call heap vector | 8,390,428 / 4,196,124 | not assessed |
| Inline 32-entry storage | 8,388,608 / 4,194,304 | 10,032 bytes |

The heap-vector numbers increase each call by 1,820 requested bytes in this
case. The inline version avoids that increase but adds 8,288 bytes to the
initial `leopard_gf16_reconstruct` frame in the observed AArch64 release
assembly, before the single- versus multi-chunk decision. Frame sizes are one
compiler/target observation, not a portable guarantee or a timing result.
The compiler was Rust 1.99.1 for `aarch64-apple-darwin`, default release
profile, with `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS` unset. The allocation
probe is process-wide, warmed, one-worker, and checks recovered bytes; its
numbers are requested bytes rather than peak or retained memory.

To reproduce, extract the commit as above and apply `prototype-inline.patch`
instead of `instrumentation.patch`. Run the work-count test above and:

```bash
RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$scratch/target" \
  cargo test --release --offline --locked -p rustfs-erasure-codec \
  --manifest-path "$scratch/Cargo.toml" --test leopard_option_allocation -- --nocapture
RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$scratch/target" \
  cargo rustc --release --offline --locked -p rustfs-erasure-codec \
  --manifest-path "$scratch/Cargo.toml" --lib -- --emit=asm
```

Apply `prototype-heap-delta.patch` after the inline patch to reproduce the heap
variant. Check the emitted `leopard_gf16_reconstruct` prologue against a
separately compiled unmodified checkout with the same flags; generated code may
vary by compiler and target. The first prototype's extra allocation and the
second's unconditional larger frame are reasons to keep both out of the runtime
until a bounded alternative and controlled performance evidence are available.

## Isolating the large frame

`prototype-split-delta.patch`, applied after `prototype-inline.patch`, moves
the inline table storage into a separate non-inlined function called only for
eligible serial multi-chunk recovery. The shared serial loop remains separate.
The same work-count, allocation and targeted recovery regressions passed;
31 `no_std` GF16 library tests and the all-features library Clippy check passed.
The prototype still has no implementation for parallel cached execution or
native x86 table reuse.

With the same AArch64 compiler and release flags, the initial
`leopard_gf16_reconstruct` frame was 1,680 bytes for this variant, versus
1,744 bytes in the committed baseline. The separate prepared function had an
8,624-byte initial frame and the shared serial-loop function had a 608-byte
initial frame. These are individual function prologues, not a measured peak
stack bound. They show that the large frame moved off the single-chunk branch;
they do not establish the cost of the extra calls, branch, lookups, or complete
stack chain.

To reproduce, apply the inline patch and then the split delta to a fresh
`c97b8b6` source extraction. Run the same work-count, allocation and assembly
commands above. The codegen observation is compiler-specific; no timing has been
collected. Retain the committed implementation until small/large shards, cache
and memory bounds, concurrent and selective recovery, native architectures, and
isolated ABBA evidence all support the tradeoff.

`prototype-split-concurrent.patch` is a self-contained version of the split
overlay plus a separate concurrency test. On AArch64, twelve concurrent serial
recoveries each alternated data-only, parity-only and combined requests for the
same erasure pattern; every requested and unrequested shard matched its expected
state and bytes. A second test covered the prototype's 32/33-total-shard
eligibility boundary with a short final chunk. Both passed in release mode.
These checks do not exercise table reuse in the parallel decoder, where this
prototype intentionally retains the committed implementation, nor do they
establish a speedup. Apply this single overlay directly to a fresh `c97b8b6`
extraction and run `cargo test --release --test leopard_prepared_scales_concurrent`
to repeat those two tests.

The separate [parallel follow-up](parallel/) extends the isolated prototype to
share immutable prepared tables among chunks in one synchronous recovery. It
retains the failed unscoped-counter run and the allocation outlier, alongside
the bounded functional results; no prototype is accepted as runtime code.
