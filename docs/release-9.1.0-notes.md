# 9.1.0 release notes

Release date is November 13, 2026. Core `rustfs-erasure-codec` 9.1.0 uses the
maintainer-approved functional and compatibility qualification. Validation
covers the tested release source, cross-platform CI and the published-9.0.0
compatibility catalog. Performance acceptance is tracked separately through the
retained diagnostic records.

## Highlights

- **Reusable caller state:** generic `ReconstructWorkspace<F>` supports complete,
  data-only and selected Classic recovery; `verify_with_workspace_par` retains
  parity scratch across parallel-policy verification calls.
- **Bounded caches:** byte-accounted Classic inverses, compact rows for wide
  sparse recovery, shared concurrent GF16 plan construction and borrowed-key
  hash reuse reduce retained storage and repeated work in covered scenarios.
- **Recovery and streaming:** share validated plans, effective coefficient rows
  and owned block storage; add independent reader/writer recovery through
  `reconstruct_stream_from` and preserve requested-output publication.
- **Repeated streams:** caller-owned `StreamWorkspace` reuses block buffers and
  recovery plans across encoding, verification and reconstruction calls.
- **Encoding and work reuse:** reduce temporary Classic matrix/view construction,
  reuse bounded Leopard GF8/GF16 work and plans, and consolidate checked NEON/GFNI
  dispatch and row processing.
- **Small Leopard recovery:** pack private staging for eligible x86-64 `std`
  workloads, reducing allocation requests while preserving selective recovery
  and destination error handling.
- **Correctness:** fix single-parity entry consistency, custom matrix cloning and
  workspace compatibility, stream count/length handling, Leopard split alignment,
  bounded-prefix joins and finite-field edge cases.
- **WASM:** validate counts and buffer lengths before mutation and return defined
  error codes instead of panicking on unsupported or invalid inputs. Retain two
  codec layouts across repeated calls with bounded inverse caches.

## Compatibility and migration

Classic GF8/tower GF16 and Leopard GF8/GF16 remain the codec families. The normal
encoded format is unchanged; the published-9.0.0 comparison checks exact parity
and bidirectional recovery over 200 non-fast configurations and historical vectors.

`EncodingFormat::stable_id_bytes()` provides an explicit versioned descriptor
for separate metadata. It is not inserted into shards and does not recreate a
codec. Keep the configured family, effective matrix and shard layout consistent
with stored data.

The ordinary-format comparison does not cover historically inconsistent
`fast_one_parity`, custom-matrix or mixed-entry codewords. Review those options
separately; rebuild parity from healthy original data where required. New
workspace checks reject incompatible formats/layouts instead of using a stale plan.

## Performance scope

Source-bound tests demonstrate reduced allocation requests, retained storage
and repeated work in covered scenarios. Timing records retain their workload,
platform and drift observations. The latest Classic and GF16 follow-ups include
complete matrices, fixed-affinity Docker runs, same-instance controls and focused
investigations. Overall throughput qualification remains a separate follow-up.

Recent capture, JSON ownership and digest improvements live in repository tools;
they are excluded from the published core crate. The WASM companion keeps its own
version and publication process.

## Installation after publication

```toml
[dependencies]
rustfs-erasure-codec = "9.1.0"
```

Enable the existing `simd-accel` or per-backend features for the deployment target.
The workspace declares no MSRV; current preparation uses the installed stable
Rust toolchain and the locked dependencies.

Full implementation and historical validation details are recorded in the
changelog. The publication record documents this release's selected qualification
and registry verification.
[Changelog](https://github.com/houseme/rustfs-erasure-codec/blob/9.1.0/CHANGELOG.md),
[publication record](https://github.com/houseme/rustfs-erasure-codec/blob/9.1.0/docs/release-9.1.0-publication-2026-11-13.md)
