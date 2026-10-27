# 9.1.0 release notes — draft

Prepared for the core `rustfs-erasure-codec` 9.1.0 release. This is draft text;
publication remains blocked until the controlled evidence and strict release
preflight in the [checklist](release-9.1.0-checklist.md) pass.

## Highlights

- **Reusable caller state:** generic `ReconstructWorkspace<F>` supports complete,
  data-only and selected Classic recovery; `verify_with_workspace_par` retains
  parity scratch across parallel-policy verification calls.
- **Recovery and streaming:** share validated plans, effective coefficient rows
  and owned block storage; add independent reader/writer recovery through
  `reconstruct_stream_from` and preserve requested-output publication.
- **Encoding and work reuse:** reduce temporary Classic matrix/view construction,
  reuse bounded Leopard GF8/GF16 work and plans, and consolidate checked NEON/GFNI
  dispatch and row processing.
- **Correctness:** fix single-parity entry consistency, custom matrix cloning and
  workspace compatibility, stream count/length handling, Leopard split alignment,
  bounded-prefix joins and finite-field edge cases.
- **WASM:** validate counts and buffer lengths before mutation and return defined
  error codes instead of panicking on unsupported or invalid inputs.

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

Source-bound tests demonstrate reduced allocation requests or repeated work in
specific scenarios. These results are not peak RSS or a universal latency /
throughput improvement. No overall speedup percentage is claimed for this release.
Acceptance of the complete controlled 9.0.0/final-candidate ABBA and paired-layout
campaign remains required by the current publication rules.

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

Full implementation and historical validation details remain in
[CHANGELOG.md](../CHANGELOG.md); operational gates and evidence configuration are
in the [preparation runbook](release-9.1.0-preparation-2026-10-27.md).
