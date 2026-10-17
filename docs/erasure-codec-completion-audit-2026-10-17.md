# Erasure codec completion audit — 2026-10-17

This audits `houseme/erasure-codec-review-fixes` from `426bd95`, with `origin/main`
`60bb40c` already merged. Gaps were [registered in backlog #2708 before implementation](https://github.com/rustfs/backlog/issues/2708#issuecomment-6229247787).
Historical checked boxes, pushes and local microbenchmarks do not establish completion of every related task.

> Subsequent work: [real WASM integration, DecodeIdx assessment and native Linux validation](erasure-codec-followup-validation-2026-10-17.md). #2852 is now completed; the associated-task table below records the earlier audit baseline.

## Four requested capabilities

| Capability | Baseline | Delivered contract |
| --- | --- | --- |
| Unified format identity | Effective matrix equality existed; no public persistent identity | One `EncodingFormat<F>` owns counts, resolved family and shared effective matrix. Codec equality and prepared recovery workspaces use it. Explicit version-1 complete metadata includes the field algebra/layout and matrix coefficients. |
| External recovery streams | Recovery required `Cursor<Vec<u8>>` | `reconstruct_stream_from` accepts independent `Option<R: Read + Send>` / `Option<W: Write + Send>` endpoints with explicit presence, no Seek requirement and block-bounded scratch. The Cursor API adapts to the same engine. |
| Combination tests and fuzz | Focused regressions existed; old fuzz targets used mostly default Classic and exact payload-size gates | One bounded helper exercises every input and is shared by a new libFuzzer target and 1,318 deterministic contract cases. |
| Comparable performance evidence | Empty/no-common/duplicate/dropped cases already rejected; missing metadata on both sides remained unchecked | `--strict-provenance` requires complete CPU/build/backend/feature/thread metadata and stream I/O metadata; supplied conflicts and conflicting aliases cannot be bypassed. Legacy omissions remain explicitly unchecked without the flag. |

The unified identity replaces duplicated codec/workspace state rather than layering another identity cache over it. Matrix allocations remain shared; cloning and comparing clones do not copy coefficients. Cache and scheduling options remain separate execution settings. Serialization occurs only on explicit request and never during codec execution or equality.

`stable_id_bytes()` returns a full descriptor, not a hash. Its schema and fixed byte fixtures are documented in `EncodingFormat` rustdoc and tested for built-in GF8 and tower GF16. Equivalent effective custom matrices compare equal regardless of their original selection option. Changes that alter encoded bytes must receive a new descriptor version or family tag. The descriptor is separate metadata: it does not change shard bytes or provide a codec deserializer. Custom fields return `None` unless they explicitly guarantee a stable algebra name and injective fixed-width element encoding. Descriptors over 64 MiB or with unsupported/invalid field metadata return `None`.

Verification workspaces retain scratch only and can be reused across compatible buffer sizes regardless of format; they do not cache matrix coefficients. Prepared reconstruction workspaces bind the canonical format, erasure pattern and shard length.

## External stream contract

```rust
use rustfs_erasure_codec::{galois_8::ReedSolomon, stream::StreamOptions};
use std::fs::File;

// Inside a function returning Result<_, Box<dyn std::error::Error>>:
let codec = ReedSolomon::new(2, 1)?;
let mut readers = [None, Some(File::open("data1")?), Some(File::open("parity0")?)];
let mut writers = [Some(File::create("data0.recovered")?), None, None];
codec.reconstruct_stream_from(&mut readers, &mut writers, &StreamOptions::default())?;
```

Both arrays have total-shards entries. Every present reader has no writer; every missing reader has a writer. Counts, Classic family, layout and sufficient survivors are checked before I/O. `Some` with an empty stream remains explicitly present, while all-`None` is insufficient even for an empty dataset. This differs deliberately from the legacy all-empty Cursor success case, whose behavior is retained by its adapter.

The API consumes current positions and retries short/interrupted I/O. It checks each block's input lengths before reconstructing or writing that block, builds one recovery plan, and retains buffers across tail blocks. It does not retain whole input/output shards; caller-provided endpoints may choose to do so. Writers are not flushed automatically. Reads may advance on failure; previous output and partial current writes are not rolled back. The caller owns recovery or atomic publication after errors. Classic GF8 is supported; Leopard streams remain explicitly unsupported.

Tests use non-Seek readers/writers, injected short/interrupted/error I/O, zero writes and generated on-demand data with checking sinks. The generated-stream test enforces at most one block of read-ahead; the metrics test checks one inverse-plan request across all blocks. These establish bounded progression and reuse, not physical file-system throughput.

## Encoder cache retention follow-up

Accumulated review found an inherited main-branch resource issue: a single TLS
encoder cache could retain tens of MiB indefinitely. This was [registered before
implementation](https://github.com/rustfs/backlog/issues/2708#issuecomment-6229423963).
Both encoders now limit retained FlatWork plus scratch to **4 MiB per family per
thread**, using actual allocation size and Vec capacity with saturating sums.
Oversized requests use local resources and preserve an existing small warm
work/scratch/plan cache. Five unit tests cover exact-budget-plus-scratch,
overflow, actual-capacity accounting, and small reuse across a large call. Warmed
10+4 allocation regressions still enforce zero heap allocations.

The budget covers retained workspace, not peak memory or all caches combined.
GF8 and GF16 each have their own cache; immutable encoding plans retain their
separate 1 MiB limit. GF16's global decode-plan LRU limits entry count to 16, not
total bytes: each entry includes its error locator, FFT schedule and key. Large
encodes can allocate again on subsequent calls; this is the deliberate tradeoff
for bounded idle retention. No large-workload speedup is claimed. Unused native
selector stubs were also removed; the central selector retains the same fallback
and all existing SIMD activation limits.

## Compatibility and validation

No encoding polynomial, matrix calculation, default family, padding rule, shard layout or SIMD activation threshold changed in this completion work.

- **400 before/after small parity snapshots and five wide raw snapshots match byte for byte** against `426bd95`: five shard layouts, four matrix options and both fast settings; Classic GF8 and tower GF16 lengths 1/65/192, Leopard GF8/GF16 lengths 64/192. Each snapshot also verifies its codeword. Wide cases cover GF8 129+64 and 128+128 at 128 KiB+64, and GF16 4+511/512/513 at 64 KiB+64, including FFT chunk tails and more than 256 total shards. The wide streams compare all 125,939,835 bytes including deterministic headers. Existing golden-vector tests passed.
- Stable 1.99.1 workspace SIMD + metrics suite: **486 passed, 43 ignored** (corrected from 491 after excluding five nested child summaries; see the [count audit](../benchmarks/recovery-paths/2026-10-17-parent-test-count-audit.json)), including doctests. Ignored benchmarks/platform tests were not silently treated as executed.
- `no_std` + SIMD suite: **291 passed, 20 ignored**, including eight format identity tests.
- New focused tests: **22 passed** (9 format, 10 external stream, 3 combination test functions). The combination helper executes 1,318 deterministic cases.
- Nightly AddressSanitizer: all **22 focused tests passed** with leak detection disabled. Six cache-budget ASAN tests also passed (the five new budget tests and the existing field-width budget test). The final SIMD fuzz target completed **30,000 executions**, starting with 6,720 generated header seeds (`-runs=30000 -max_len=256 -seed=2708 -timeout=10`). No failure was reported. This is a bounded smoke run, not exhaustive fuzzing.
- Performance gate Python suite: **32 passed**. Historical legacy artifacts still compare, while strict mode rejects their missing CPU/build/thread provenance.
- All-feature, all-target workspace Clippy with `-D warnings`, formatting and diff checks passed.
- Compile-only: x86_64 macOS library/tests, x86_64 Linux library, i686 Linux `no_std` library and wasm32 wrapper passed. These are not native execution or real JS/WASM integration results. MSRV 1.98.1 and big-endian runtime were not re-executed in this audit.

The reusable snapshot source is [`scripts/encoding_compatibility_probe.rs`](../scripts/encoding_compatibility_probe.rs). To reproduce: extract baseline `426bd95` and the candidate into separate clean checkouts; create an independent Cargo binary project for each with `[workspace]` and a `rustfs-erasure-codec` path dependency enabling `simd-accel`; use the probe as each `src/main.rs`. Run each with stable and a different `CARGO_TARGET_DIR`, capture stdout as JSONL, and compare the two 400-row files using `cmp`. Run both again with `-- --wide` and compare their binary stdout snapshots for the five wide cases. Keep the locked dependency environment identical; separate targets avoid mixing generated build output from different sources.

## Accumulated branch review and remaining work

The accumulated branch keeps one Classic recovery planner, one checked native-encode boundary, bounded per-thread Leopard encoding and decoding work caches, and one stream recovery engine. The new identity centralizes equality and workspace checks. Review covered malformed input before writes, effective fast-parity matrices, custom-field contracts, tail buffers, retained plans and option-independent identity. It found and fixed a missing module import and one mechanical getter-migration typo before validation, added the encoder retention budget, and removed unused selector stubs. Independent format/stream/SIMD/cache reviews found no remaining blocking defect.

This work establishes compatibility and removes duplicate state/execution paths. No new cross-machine or downstream throughput percentage is claimed, and no universal optimality claim follows from these tests. Existing local paired NEON evidence and its drift limits remain in the [2026-10-16 review](erasure-codec-neon-review-2026-10-16.md). Wide stream layouts can still allocate temporary metadata views beyond inline capacity; scratch remains bounded by layout and block size.

| Associated task | Current audit result |
| --- | --- |
| [#2571](https://github.com/rustfs/backlog/issues/2571) | Closed issue's P0/P1 contracts are implemented and covered: all Leopard families reject DecodeIdx, constructor boundaries are validated, and both required-mask lengths work. P2 evaluation of a DecodeIdx-specific workspace and isolated throughput is not inferred from closure. |
| [#2852](https://github.com/rustfs/backlog/issues/2852) | Still open: TypeScript manually maintains the raw wasm-bindgen ABI. Real generated-bindings Node/browser round trips and CI integration remain incomplete. wasm32 compilation and mock tests do not satisfy acceptance. |
| [#2853](https://github.com/rustfs/backlog/issues/2853) | Still open: resource-isolated Linux/x86 and downstream repeated throughput validation remains incomplete. Metadata equality cannot establish resource isolation or acceptable baseline drift. |
| [#2708](https://github.com/rustfs/backlog/issues/2708) | The four requested capabilities above are delivered within their documented scope. The parent remains open while associated acceptance work remains. |
