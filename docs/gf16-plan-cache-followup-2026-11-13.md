# GF16 plan-cache lookup and diagnostic follow-up

The runtime follow-up in `8781e8de` borrows presence masks on cache hits and
propagates a computed hash through cold-plan rechecks and insertion. PR #123
merged the runtime changes into `f090a809`. The paired diagnostic records drift
for every comparison and continues through its complete workload matrix.
Measurements began on November 12 and continued on November 13.

## Investigation

The original screen warmed each phase with 128 calls. Its first 10+4, 64-byte
case measured approximately 2 microseconds per call, making that warmup about
0.25 milliseconds. Baseline A1/A2 drift reached 6.35% and the screen stopped.

An expanded screen used one second of interleaved warmup, process CPU time,
context-switch counts and minor-fault counts. It completed all 16 serial cases
and 48 comparisons. Fifteen comparisons exceeded the 5% drift reference.
Several rows combined substantial CPU-time drift with CPU/wall ratios above
96%. Longer initial warmup alone therefore did not explain the variability.

The first concurrent probe used one recovery per barrier handoff. Large wall
differences appeared alongside much smaller CPU differences. Same-code A/A
controls also exceeded the reference. These observations motivated fixed Linux
affinity, per-phase warmup, batches of 32 recoveries per warm concurrent handoff,
and controls that use the same codec instance for both roles.

The final follow-up adds 20-millisecond phases and 24 balanced ABBA/BAAB rounds.
Every raw phase and every over-reference comparison remains in the evidence.
Sleep calibration in the container measured approximately 0.017 milliseconds
of process CPU time across 200 milliseconds of wall time.

## Following flagged cases

The complete short-window run covered all 32 workloads and 768 comparisons.
Same-instance A/A controls added 48 comparisons. Their all-round wall medians
were +1.29% for serial 10+4 recovery and -0.69% for eight callers. Individual
rounds continued to carry drift flags, which remain in the raw records.

The full matrix's 300+4, 64-byte warm case prompted a separate fresh-process
experiment with six rounds and 500-millisecond phases. Both the runtime
comparison and same-instance control used the same pinned CPU arrangements.

| Focused case | All-round wall median | All-round CPU median | Drift flags |
| --- | ---: | ---: | ---: |
| Serial candidate versus baseline | +0.63% | -0.07% | 0/6 |
| Eight callers, candidate versus baseline | +0.52% | +0.14% | 1/6 |
| Serial same-instance A/A | +0.20% | -0.48% | 1/6 |
| Eight callers, same-instance A/A | +0.22% | +0.44% | 0/6 |

The spread across the full-matrix and fresh-process measurements indicates
run-context sensitivity. Cache history, placement and system activity remain
follow-up variables. The allocation regression directly verifies the removed
temporary key, and the focused timing records provide the narrower comparison
above. The archive retains 1,101 paired comparisons with 770 drift flags, plus
the original short screen.

## Runtime changes

The shared cache keeps a clone of its hash builder outside the mutex. Lookups
hash `(data_shards, parity_shards, present_slice)` before locking. The stored
SmallVec key hashes its slice, and equality checks both dimensions and every
presence bit. Borrowed hits advance LRU recency and return the shared plan.

Cold requests create their owned mask after the first miss. The same hash is
used for the coalescing recheck, duplicate-result check and raw-entry insertion.
Weighted admission charges owned key capacity and plan storage. Tests exercise
hash agreement across inline and heap boundaries, full-key collision checks,
recency before eviction, pending initialization and panic recovery.

The allocation regression uses warmed serial `ShardSlot<Vec<u8>>` recovery,
one missing data shard, 64-byte shards and one execution job. Caller setup and
warmup finish before allocation counting starts.

| Layout | Allocation requests before | After | Requested bytes before | After |
| --- | ---: | ---: | ---: | ---: |
| 252+4 | 5 | 5 | 14,656 | 14,656 |
| 253+4 | 6 | 5 | 19,305 | 18,793 |
| 300+4 | 6 | 5 | 21,232 | 20,720 |

Each wide hit removes one 512-byte temporary mask request. The complete
allocation records are retained with the diagnostic evidence.

## Measurement protocol

The workload matrix covers 10+4, 252+4, 253+4 and 300+4 layouts at 64 and
4,096 bytes, using one or eight callers and warm or rotating-erasure workloads.
The rotating working set contains 20 masks against the cache's 16-entry limit.
Each concurrent wave gives all workers the same mask. Warm concurrent waves
execute 32 recoveries per worker. Cold waves execute one recovery per worker.

Caller buffers keep their addresses across both implementations. Worker threads
and their TLS buffers remain alive across paired phases. Every phase checks all
recovered bytes and presence flags. A scoped shutdown guard releases workers
when an assertion unwinds.

Phase records include completed calls, wall and process CPU nanoseconds per
call, context switches and minor faults. Concurrent wall cost includes the
wave's synchronization. The Linux runs use a two-CPU Docker quota, one pinned
CPU for serial cases and two for concurrent cases.

Raw observations, allocation records, source revisions and summaries are
retained together. The timing observations describe these diagnostic runs.
The same-code controls and all drift flags remain part of their interpretation.
[Evidence directory](../benchmarks/gf16-plan-cache-followup/2026-11-13/)

## Validation

- Host GF16 decoder tests, allocation regressions, no_std tests, complete xtask
  tests, Clippy and formatting checks passed.
- Docker Linux ARM64 passed the all-features library/integration/doc suite,
  no_std library and compact-recovery tests, WASM Rust tests and complete xtask
  tests. Property-test settings used four cases and generator size 32.
- The published-9.0.0 compatibility harness passed its parity, cross-decoding,
  corruption-detection and historical tower-field cases.
- All 516 dynamic workers completed. Built-in and standalone numeric replays
  matched across 6,192 observations and 1,540,602 storage events.
- The runtime PR completed 31 successful Actions checks with three conditional
  release-related skips.

## Reproduction

Build both codec revisions with `std simd-accel` and distinct rustc metadata
values, then link their copied rlibs and the matching libc rlib into the probe.
The metadata values allow two builds of the same package version to coexist in
one executable. The frozen runtime comparison uses `59347ca5` and `8781e8de`.
[Current probe](../scripts/paired_gf16_decode_probe.rs)

From the repository root, the following builds a standalone comparison in a
temporary directory.

```sh
repo_root="$PWD"
task_dir=$(mktemp -d)
mkdir -p "$task_dir/base" "$task_dir/candidate"
git archive 59347ca5 | tar -x -C "$task_dir/base"
git archive 8781e8de | tar -x -C "$task_dir/candidate"
export CARGO_TARGET_DIR="$task_dir/target" RUSTUP_TOOLCHAIN=stable
cargo rustc --manifest-path "$task_dir/base/Cargo.toml" \
  -p rustfs-erasure-codec --release --lib --features 'std simd-accel' \
  -- -C metadata=gf16_base
cp "$CARGO_TARGET_DIR/release/librustfs_erasure_codec.rlib" "$task_dir/libbase_codec.rlib"
cargo rustc --manifest-path "$task_dir/candidate/Cargo.toml" \
  -p rustfs-erasure-codec --release --lib --features 'std simd-accel' \
  -- -C metadata=gf16_candidate
cp "$CARGO_TARGET_DIR/release/librustfs_erasure_codec.rlib" "$task_dir/libnext_codec.rlib"
set -- "$CARGO_TARGET_DIR"/release/deps/liblibc-*.rlib
test "$#" -eq 1
rustc --edition=2024 -O "$repo_root/scripts/paired_gf16_decode_probe.rs" \
  --extern "base_codec=$task_dir/libbase_codec.rlib" \
  --extern "next_codec=$task_dir/libnext_codec.rlib" --extern "libc=$1" \
  -L "dependency=$CARGO_TARGET_DIR/release/deps" -o "$task_dir/paired-probe"
```

The short-window diagnostic uses the following settings. CPU affinity is
available on Linux, and its effective CPU count is recorded in each output.

```sh
RSE_CACHE_ROUNDS=24 \
RSE_CACHE_WARM_MS=1000 \
RSE_CACHE_PHASE_WARM_MS=0 \
RSE_CACHE_SAMPLE_MS=20 \
RSE_CACHE_PIN_CPUS=0,1 \
RSE_CACHE_FILTER=concurrent/ \
"$task_dir/paired-probe" > "$task_dir/concurrent.jsonl"
```

Set `RSE_CACHE_SAME_CODEC=1` for a same-instance control. Set
`RSE_CACHE_WARM_BATCH=1` to investigate per-request handoff cost. The archived
initial probes retain the earlier warmup and phase arrangements.
