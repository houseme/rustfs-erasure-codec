# Main architecture and performance review — 2026-10-18

Reviewed current main: `03e804943667b42f646cd97187b3c9d3f6a52034`, after the
maintainer's rebase merges of PR #8 and PR #9. Its tree was checked equal to the
previous final candidate `c656368`; this review retains both accepted refactors.
The new allocation finding was [recorded before implementation](https://github.com/rustfs/backlog/issues/2708#issuecomment-6245351022).

## Architecture assessment

The reviewed code does not justify an overall replacement or a new dispatcher.
The existing ownership and execution boundaries are coherent:

| Area | Current design and review conclusion |
| --- | --- |
| Format identity | One effective `EncodingFormat` owns shared immutable coefficients. Clones use an Arc identity fast path; serialization is explicit. |
| Classic recovery | One shape/plan selects survivors, outputs and inverse rows. Generic fallible destinations retain ordered publication; concrete Vec adapters support batched computation. Their different contracts justify separate executors. |
| Streaming | Initialized storage, actual read lengths and logical prefixes are owned together. Serial/parallel reads share completion; short tails preserve initialized storage for EOF checks. Recovery plans remain reusable. |
| Native encoding | One checked view constructor and prepared dispatch. Activation remains bounded by the measured ISA/shape/job policy. |
| FFT resources | GF8/GF16 layouts and bounded per-thread work/plan retention remain separate. No new cache or retained payload is introduced by this review. |
| Matrix construction | The zeroed constructor unnecessarily allocated temporary Vec storage before choosing inline SmallVec storage. This shared ownership transition is the verified defect below. |

No additional blocking correctness defect was found in this scope. Validation
and error publication precede output use. The current generic-field capability
hooks, matrix identity checks and complete-layout fast returns remain intact.

## Verified allocation defect and shared fix

`[src/matrix.rs:60] Temporary Vec allocation before inline matrix storage`

Severity: Minor. This is a reproducible resource inefficiency; valid encoded
bytes and recovery results are unchanged.

`Matrix::new` previously used `SmallVec::from_vec(vec![zero; rows * cols])`.
For matrices within the existing 1024-element inline capacity, that allocated a
Vec, copied its elements into inline storage, then freed the temporary. Codec
construction, inverse submatrices, identity matrices, augmentation and extraction
all use this constructor. A cache miss takes the same inverse path; a warm
inverse cache already avoids that work.

The constructor now uses `SmallVec::from_elem`, initializing the final storage
directly. The existing capacity, initialization value, matrix coefficients and
algebraic operations are preserved. Larger matrices still allocate their actual
storage. This changes one shared construction policy, with no caller-specific
branches, new public API or unsafe code.

The full generic data-recovery call was measured with reusable, poisoned
ShardSlot destinations and inversion caching disabled. Expected data and storage
were prepared outside the counting scope. Both GF8 and tower GF16 use 65-element
shards, two parity shards, and two independent recoveries per layout:

| Data shards | Main requests per recovery | Candidate requests | Required retained storage |
| --- | --- | --- | --- |
| 4, 10, 22 | 5 | 1 | Independently owned inverse Arc |
| 23, 32 | 5 | 2 | Inverse Arc plus genuinely large augmented matrix |

At 22 data shards the augmented matrix has 968 elements; at 23 it has 1058.
At 32, the inverse itself has exactly 1024 elements while augmentation has 2048.
The tests exercise these inline/heap boundaries, check every recovered element
and presence bit, and enforce maximum allocation budgets. Both tests fail on
original main with only tests overlaid, then pass with the shared constructor
change. No timing campaign or peak-memory measurement is inferred from them.

[Machine-readable evidence](../benchmarks/recovery-paths/2026-10-18-matrix-storage-review.json)
contains all 20 observations, compiler/features, source hashes and budgets.

## Compatibility and validation

Stable Rust 1.99.1 on macOS ARM64: workspace 502 passed/43 ignored (core495 plus
WASM Rust7), genuine no_std+SIMD 297/20, 45 focused release regressions,
all-target/all-feature Clippy and wasm32 checks pass. Counts use each test
executable's final parent summary, excluding five nested child summaries.

The existing shared executable links actual published 9.0.0 and candidate 9.1.0;
all 200 non-fast full-byte parity/roundtrip cases pass. Existing fast-mode,
custom-matrix, GF16, wide-layout, late-error and stream tests also pass. The
algorithm family, finite-field arithmetic, encoded matrix, descriptor version,
cache lifetimes, public trait bounds and job thresholds are unchanged.

## Evidence boundary

This is an allocation optimization in construction and uncached/cold inverse
planning. It does not establish steady-state throughput gains. The previous
full 66-case timing campaign remains rejected after seven drift failures, with
all candidate ratios null; no selected subset was retried. Dedicated ARM/S3
throughput and strict release-preflight still require their actual evidence.
#2708/#2853 stay open; this review does not tag or publish a version. Final
branch/PR CI and distributable evidence are recorded in the follow-up PR.
