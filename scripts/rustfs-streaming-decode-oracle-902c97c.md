# RustFS streaming decode benchmark oracle overlay

Tracking: [rustfs/backlog#2853](https://github.com/rustfs/backlog/issues/2853).

Apply `rustfs-streaming-decode-oracle-902c97c.patch` only to an isolated RustFS
checkout at commit `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`. The patch changes
only `bench_streaming_decode` in `crates/ecstore/benches/erasure_benchmark.rs`.
It does not change production code, quorum, surplus-source verification, or
bitrot settings.

## Behavior

- Keep all six healthy benchmark IDs; add six IDs ending in `_missing-first-data`.
  The new scenario supplies `None` only for reader 0, requiring actual recovery.
- Generate deterministic, nontrivial plaintext from each absolute byte offset
  using the SplitMix64 output transformation. Unlike the old `offset % 256`
  fixture, data shards and stripes no longer repeat the same short pattern.
- Before timing, decode into a `Vec<u8>` and require no error, the exact written
  count, and equality with the complete original plaintext.
- During timing, use a function-local, allocation-free `ObservedSink` whose
  `poll_write` passes the byte slice through `black_box` and counts bytes actually
  received. Each iteration requires no error, the exact API-reported written
  count, and the exact sink-observed count. Any `Some(error)` fails, even with
  partial or full output. Fixture encoding, bitrot stream construction, reader
  and sink construction, and the full plaintext comparison remain untimed.
  The sink makes decoded bytes observable without retaining or copying them;
  this is a measurement-design improvement, not evidence of a production bug.
- Immediately after creating the runtime, emit one stderr line prefixed with
  `RSE_STREAMING_PROVENANCE=` followed by JSON. It reports `resolved_backend`
  from `galois_8::active_backend_name()`, OS `available_parallelism`, and
  `actual_tokio_workers` from the pinned Tokio 1.53.2 stable runtime metrics.
  Backend selection is explicitly not a per-operation kernel trace. OS query
  failure produces `null`, never a substituted environment value.
- `actual_rayon_threads` is explicitly `null`, with
  `rayon_observation="unavailable_no_direct_dependency"`: pinned ECStore has no
  direct Rayon dependency, so this source-only overlay cannot observe its pool
  without adding a dependency. Do not treat `RAYON_NUM_THREADS` or the OS CPU
  count as an actual pool measurement. `options_max_parallel_jobs` reports the
  codec default used by pinned `Erasure::new` (0 in both compared codecs).

Pinned `Erasure::new` calls `try_new_with_options(..., false)`, which constructs
the modern `rustfs-erasure-codec` encoder. It does not select `reed-solomon-simd`
based on shard size. The selected missing-first-data cases therefore exercise
modern recovery; healthy cases retain their all-data-present control path.

The changed fixture, observed sink, and timed assertions mean old, unpatched
measurements and earlier no-op-sink overlay measurements are not comparable.
Apply this identical patch to both baseline and candidate in
all A1/B1/B2/A2 phases. Keep separate release target/output directories and
record source, dependency, feature, toolchain, and overlay provenance.

## Apply and inspect

Set `RUSTFS_ORACLE_CHECKOUT` to the disposable downstream checkout and
`CODEC_OVERLAY_ROOT` to this codec repository. Existing dependency overlays may
be present, but the benchmark file must still match the pinned source blob.

```sh
(
set -eu
: "${RUSTFS_ORACLE_CHECKOUT:?set the disposable pinned RustFS checkout}"
: "${CODEC_OVERLAY_ROOT:?set the codec repository containing the overlay}"
test "$(git -C "$RUSTFS_ORACLE_CHECKOUT" rev-parse HEAD)" = 902c97ce055b1faddb8cd689f0d3a04a0a3e6f35
test "$(git -C "$RUSTFS_ORACLE_CHECKOUT" hash-object crates/ecstore/benches/erasure_benchmark.rs)" = b7c5bc9bda62bda4ef71d4d2fb90fd6d420cf685
git -C "$RUSTFS_ORACLE_CHECKOUT" apply --check "$CODEC_OVERLAY_ROOT/scripts/rustfs-streaming-decode-oracle-902c97c.patch"
git -C "$RUSTFS_ORACLE_CHECKOUT" apply "$CODEC_OVERLAY_ROOT/scripts/rustfs-streaming-decode-oracle-902c97c.patch"
git -C "$RUSTFS_ORACLE_CHECKOUT" diff --check -- crates/ecstore/benches/erasure_benchmark.rs
git -C "$RUSTFS_ORACLE_CHECKOUT" diff --stat -- crates/ecstore/benches/erasure_benchmark.rs
)
```

## Narrow measurement filter

The exact Criterion ID prefix is
`streaming_decode/decode/4+2_4096KB_64KB-block_verify-`.
The following filter selects four measured cells: healthy and missing-first-data,
each with bitrot verification enabled or disabled. Each object has 64 stripes.

```sh
cargo bench -p rustfs-ecstore --bench erasure_benchmark --locked --offline -- '^streaming_decode/decode/4\+2_4096KB_64KB-block_verify-(true|false)(_missing-first-data)?$'
```

Run this only under the coordinator's isolated release-build and measurement
procedure. The command above describes selection, not an instruction to use a
shared target directory or change the selected compiler/CPU/features.
Report all four cells separately. `verify=true` is the primary integrity-enabled
case; `verify=false` is a diagnostic control, not a production speedup claim.

Pinned Criterion is 0.8.2. The existing group explicitly sets `sample_size(10)`
and `measurement_time(5 seconds)`; these override the corresponding CLI defaults.
Warm-up uses Criterion's default unless configured at the CLI. The patch leaves
these settings unchanged. The filter restricts timed cases, but fixture setup
and the untimed preflight outside `bench_function` still execute for all twelve
registered streaming cases, including when listing/filtering benchmarks.

The overlay by itself is not downstream compilation, runtime correctness, or
performance evidence. Those require the coordinator's real release build, oracle
passes, and four-phase measurements with an accepted baseline-drift gate. Actual
execution and its remaining boundaries are recorded in the
[follow-up validation report](../docs/erasure-codec-followup-validation-2026-10-17.md).
