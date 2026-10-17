# Task 13: Progressive DecodeIdx for Classic GF(2^8)

Current P2 evaluation and its retained implementation: [2026-10-17 workspace assessment](decode-idx-workspace-evaluation-2026-10-17.md). The existing API receives a measured capacity fix; no new prepared-workspace API or isolated throughput conclusion is claimed.

## 1. Goal

Add a progressive decode API for the classic GF(2^8) path that allows callers to reconstruct missing shards from
multiple partial input arrivals instead of requiring a single all-at-once `reconstruct` call.

This task targets operational flexibility, not a new coding family.

## 2. Why This Task Exists

One-shot reconstruction is sufficient for simple cases, but it is limiting for:

- distributed systems receiving shards from multiple peers
- partial reconstruction aggregation
- network flows where inputs arrive over time
- multi-source recovery orchestration

Upstream `klauspost/reedsolomon` exposes a `DecodeIdx` model for this class of workflow. This task brings an
equivalent capability to the classic Rust path.

## 3. Current Code Anchors

Primary anchors:

- `src/core.rs`
  - `get_data_decode_matrix()`
  - `reconstruct_some()`
  - `reconstruct_internal*()`
  - raw coding helpers

This task will likely benefit from the planner work in:

- `docs/task-12-reconstruct-plan-unification.md`

## 4. Scope

## 4.1 In scope

- progressive decode for classic GF(2^8)
- caller-provided destination buffers
- multi-step accumulation
- optional merge/additive mode

## 4.2 Out of scope

- Leopard family support
- stream API
- MinIO metadata/on-disk format integration

## 5. Recommended Public API

A conservative first-pass API:

```rust
pub fn decode_idx(
    &self,
    dst: &mut [Option<Vec<u8>>],
    expect_input: Option<&[bool]>,
    input: &[Option<Vec<u8>>],
) -> Result<(), Error>;
```

Alternative lower-allocation API can be added later, but the first version should prioritize clarity.

Implemented first-pass API in this repository:

```rust
pub fn decode_idx(
    &self,
    dst: &mut [Option<Vec<u8>>],
    expect_input: Option<&[bool]>,
    input: &[Option<Vec<u8>>],
) -> Result<(), Error>;
```

## 6. Semantics

## 6.1 Normal progressive mode

When `expect_input` is `Some(flags)`:

- `flags[i] == true` means shard `i` is expected to arrive as input across calls
- `flags[i] == false` means shard `i` is a reconstruction target or otherwise not expected as source
- successive calls may provide different subsets of the expected input shards
- destination shards accumulate reconstruction contributions

## 6.2 Merge mode

When `expect_input` is `None`:

- `input` is treated as another partial decode result
- matching non-empty input shards are XOR-accumulated into destination shards

This mode is useful for merging partial work produced by different nodes or independent decode passes.

## 6.3 Destination contract

Document clearly:

- destination shards that will be reconstructed should start zeroed on the first call
- destination shard lengths must match all non-empty input shard lengths
- all provided shards in a decode session must have consistent lengths

## 7. Implementation Strategy

## 7.1 Planner reuse

The implementation should reuse the decode matrix and requested-row planning logic instead of inventing a separate
mathematical path.

Recommended flow:

1. validate `expect_input`, `dst`, and `input`
2. derive valid/invalid indices from `expect_input`
3. build decode matrix
4. build requested output rows for non-empty `dst` targets
5. reduce matrix columns to the subset of actually supplied inputs in this call
6. accumulate into destination shards

## 7.2 Accumulation semantics

Accumulation should be additive in GF(2^8), which for `u8` means XOR for the sum of partial contributions.

Do not clear destination output on each call in progressive mode.

## 7.3 Supported first-pass target set

The first implementation may restrict support to:

- data-shard reconstruction targets
- parity targets optionally added if planner logic stays clean

If parity-target support adds too much complexity, stage it behind a clear limitation note and follow-up task.

Current first-pass support in this repository:

- data-shard reconstruction targets
- parity-shard targets are also accepted when destination buffers are provided
- normal progressive accumulation mode
- merge mode (`expect_input == None`)
- destination buffers must already exist for any shard being accumulated into

## 8. Detailed Execution Steps

1. Choose final API name and shape.
2. Implement strict validation for lengths, counts, and expectation flags.
3. Reuse decode matrix generation from classic reconstruct logic.
4. Add matrix-column reduction to handle partial input subsets.
5. Add merge mode.
6. Add multi-step equivalence tests versus one-shot reconstruction.
7. Document caveats and expected zeroed-destination behavior.

Current applied progress:

1. done
2. done
3. done through `get_data_decode_matrix(...)`
4. done
5. done
6. done for the primary `reconstruct_some` equivalence path
7. in progress

## 9. Test Plan

## 9.1 Basic progressive tests

Add tests for:

- reconstruct one missing shard from two progressive calls
- reconstruct multiple missing shards from multiple calls
- mixed data/parity inputs across calls
- empty call that provides no new input

Current coverage:

- two-step progressive reconstruction matching one-shot required recovery

## 9.2 Merge-mode tests

Add tests for:

- merge two partial reconstructions into one result
- reject invalid merge with mismatched lengths
- reject merge into absent destination shard

Current coverage:

- merge accumulation path
- merge with missing destination target

## 9.3 Equivalence tests

For supported cases:

- progressive `decode_idx` final result == one-shot `reconstruct_some` / `reconstruct`

Current coverage:

- `decode_idx` final accumulated result matches one-shot `reconstruct_some(...)` for required data shards
- invalid `expect_input` length is rejected
- invalid `dst` / `input` lengths are rejected
- shard size mismatch is rejected
- too few expected inputs are rejected

## 10. Benchmark Plan

This task is more API/operational than raw throughput, but at minimum ensure no obvious regressions in helper reuse.

Suggested benchmark focus:

- partial two-step reconstruct vs one-shot reconstruct
- small number of requested outputs
- representative 10x4 and 32x16 cases

Current first benchmark:

- `cargo test benchmark_decode_idx_vs_reconstruct_some_10x4_1m_exports_results -- --nocapture`
- `cargo test benchmark_decode_idx_vs_reconstruct_some_32x16_1m_exports_results -- --nocapture`
- results note: `docs/ec-decode-idx-benchmark-results-2026-05-28.md`

Current measured results:

- `10x4_1m`
  - `decode_idx(...)` in a two-step progressive flow runs at about `0.91x` of one-shot `reconstruct_some(...)`
- `4x2_64k`
  - `decode_idx(...)` in a two-step progressive flow runs at about `1.03x` of one-shot `reconstruct_some(...)`
- `32x16_1m`
  - `decode_idx(...)` in a two-step progressive flow runs at about `1.01x` of one-shot `reconstruct_some(...)`

Interpretation:

- the reduced-column + small-output optimization keeps `decode_idx(...)` in the same broad performance band as
  one-shot recovery on the measured cases
- `10x4_1m` still trails one-shot performance and remains the clearest optimization target
- the same optimization was enough to push `32x16` slightly ahead of one-shot `reconstruct_some(...)`
- this reinforces the conclusion that fixed progressive overheads amortize better at higher fanout
- a more aggressive follow-up micro-optimization for small-fanout data-only outputs was evaluated and reverted because
  it did not produce a stable benchmark win

## 11. Acceptance Criteria

This task is complete when:

1. progressive decode works across multiple calls
2. merge mode works for supported shapes
3. final results match classic one-shot reconstruction
4. docs clearly describe destination initialization and expectation rules

Current status:

- first-pass API is implemented
- progressive mode works for the primary supported flow
- merge mode works
- core error-path validation is covered by tests
- README-level public usage documentation has been added
- a first paired benchmark against `reconstruct_some(...)` now exists

## 12. Risks

### R1. API too permissive and hard to reason about

Mitigation:

- validate aggressively
- document invariants precisely

### R2. Partial accumulation mistakes create silent corruption

Mitigation:

- equivalence tests against one-shot reconstruct are mandatory

### R3. This task depends on planner cleanup

Mitigation:

- either land Task 12 first
- or duplicate minimal planner logic temporarily and refactor afterward

## 13. Rollout Guidance

Suggested PR title:

- `task13: add classic progressive decode api`

Suggested implementation sequence:

1. progressive mode only
2. merge mode
3. benchmarks and docs
