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
