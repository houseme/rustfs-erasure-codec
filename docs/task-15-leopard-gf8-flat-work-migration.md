# Task 15: Leopard GF8 FlatWork Migration

## 1. Goal

Replace the current `Vec<Vec<u8>>` work-buffer organization used by the in-progress pure-Rust `LeopardGF8` encode
path with a flatter work-buffer model built around `FlatWork`.

This task exists because the next likely performance gains are no longer coming from tiny local knobs such as:

- `zero` buffer reuse
- `xor_clone` variants
- later-layer loop-shape micro-edits

The remaining large cost center is the shape of the working memory itself.

## 2. Current Situation

Current GF8 module state in [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1):

- `FlatWork` now exists as a structural scaffold
- the active encode path still executes on `Vec<Vec<u8>>`
- the current stable baseline is approximately:
  - `64x32_1m`: `16.1089 MB/s`
  - `64x32_4m`: `15.8374 MB/s`

This means the next step should be a real data-structure migration, not another micro-optimization round.

## 3. Why This Task Exists

The current `Vec<Vec<u8>>` work layout creates several sources of overhead:

1. per-lane allocation / metadata overhead
2. repeated slice-boundary work across inner helpers
3. poorer cache locality than a flatter lane-packed buffer
4. higher cost when deriving temporary lane views for butterfly operations

If Leopard is going to become a serious alternative family, this structure must be improved before continuing to
optimize the existing helper stack.

## 4. Scope

### 4.1 In scope

- use `FlatWork` as the active encode work-buffer backing store
- migrate helper access one layer at a time
- keep benchmarking tightly focused on `64x32_1m` and `64x32_4m`

### 4.2 Out of scope

- Leopard verify
- Leopard reconstruct
- generic `Field`-wide flattening
- aggressive SIMD work in this task

## 5. Current Code Anchors

- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - `FlatWork`
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8(...)`
  - `fft_dit8(...)`
  - `fft_dit4_at(...)`
  - `ifft_dit4_at(...)`

- [src/galois_8/policy.rs](../src/galois_8/policy.rs:1)
  - specialized `encode_opt(...)` route for `LeopardGF8`

- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - dedicated Leopard encode smoke exporters

## 6. Migration Strategy

### 6.1 Guiding rule

Do not switch all helper signatures at once.

Instead:

1. keep `FlatWork` as the owning buffer
2. derive temporary lane views from it
3. migrate helper layers gradually from `Vec<Vec<u8>>`-shaped signatures to flat-lane access

### 6.2 Recommended phases

#### Phase A

Use `FlatWork` as the backing store in `encode_with_tables(...)`, but keep helper interfaces mostly unchanged by
building borrowed lane views only at the boundary.

#### Phase B

Change `ifft_dit_encoder8(...)` to operate on flat-backed lane views directly, so the function no longer assumes
owned `Vec<u8>` rows.

#### Phase C

Migrate:

- `fft_dit8(...)`
- `fft_dit4_at(...)`
- `ifft_dit4_at(...)`

to the same flat-backed lane-view model.

#### Phase D

Only after A-C stabilize, consider whether `get_pair_mut(...)` can be simplified or removed under the new layout.

## 7. Implementation Constraints

### 7.1 Preserve external behavior

The following must remain true:

- `CodecFamily::LeopardGF8` remains explicit opt-in
- classic path remains untouched
- `verify/reconstruct/update/decode_idx` stay unsupported for Leopard in this task

### 7.2 Keep benchmark interpretation honest

During this task:

- benchmark regressions should be treated as real until disproven
- do not overwrite the documented stable mainline with a regression
- if a trial is worse, revert it or keep it behind a clear experiment-only path

## 8. Execution Plan

### Step 1

Document the current stable baseline:

- `64x32_1m`: `16.1089 MB/s`
- `64x32_4m`: `15.8374 MB/s`

### Step 2

Make `FlatWork` the actual work ownership container in `encode_with_tables(...)`.

### Step 3

Add a narrow adapter layer that exposes lane views from `FlatWork` without copying lane contents.

### Step 4

Migrate `ifft_dit_encoder8(...)` to use those lane views.

### Step 5

Re-run:

```bash
cargo test benchmark_leopard_encode_64x32_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_64x32_4m_exports_results -- --nocapture
```

### Step 6

Only if Step 5 is stable or better, continue into `fft_dit8(...)` and butterfly helpers.

## 9. Acceptance Criteria

This task should be considered successful only if:

1. `FlatWork` is no longer dead scaffolding
2. the encode path actually executes through flat-backed work storage
3. the mainline `64x32_1m` / `64x32_4m` results are at least as good as the current stable baseline
4. results are written back to the Leopard benchmark note

## 10. Risks

### R1. Half-flat, half-owned confusion

If helper boundaries are migrated inconsistently, the code may become harder to reason about than before.

Mitigation:

- migrate one layer at a time
- keep ownership and borrowing explicit

### R2. Benchmark regressions from adapter overhead

Flat storage can still regress if lane-view creation is too expensive.

Mitigation:

- benchmark every phase
- revert or rework regressions immediately

### R3. Over-expanding scope

It is tempting to pull verify/reconstruct into the same migration.

Mitigation:

- explicitly keep this task encode-only

## 11. Current Recommendation

Proceed with `FlatWork` migration as the next serious LeopardGF8 implementation slice.

Among all recent attempts, this is the first direction that is large enough to plausibly move the encode path beyond
the current `~16 MB/s` band.

## 12. Latest Observation

The first full `FlatWork`-owner cutover does not yet beat the stronger `Vec<Vec<u8>>` mainline, but it is close
enough to justify continuing the migration as a dedicated slice rather than abandoning it outright.

Recent observation:

- `64x32_1m`: around `15.6 MB/s`
- `64x32_4m`: around `15.9-16.1 MB/s`

Micro-tuning the lane-view container inline capacity was effectively flat, which suggests the remaining gains are more
likely to come from deeper work-buffer access/layout changes than from further `SmallVec` tuning.

## 13. Current Migration Checkpoint

What is now true in code:

- `FlatWork` exists as a real owner type
- helper interfaces are lane-view friendly
- `encode_with_tables(...)` can execute through the `FlatWork -> with_lane_views(...) -> helper` path

Current benchmark checkpoint for the migrated path:

- `64x32_1m`: `15.6476 MB/s`
- `64x32_4m`: `16.0906 MB/s`

Comparison against the stronger preserved baseline:

- preserved baseline `64x32_1m`: `16.1089 MB/s`
- preserved baseline `64x32_4m`: `15.8374 MB/s`

Interpretation:

- the `FlatWork` migration is no longer just structural scaffolding; it runs end-to-end
- the migrated path is already competitive with the preserved baseline
- but it has not yet won clearly enough on both cases to replace the current accepted mainline

Latest checkpoint after trimming fixed per-call overhead:

- `64x32_1m`: `15.7147 MB/s`
- `64x32_4m`: `16.2620 MB/s`

Updated interpretation:

- the migrated path benefits from fixed-overhead cleanup
- the larger-shard case now clearly beats the preserved baseline
- the smaller-shard case is still slightly behind, so migration should continue with `64x32_1m` as the primary
  acceptance target

Latest focused `64x32_1m` result:

- `64x32_1m`: `16.4482 MB/s`

Updated interpretation:

- the `FlatWork` migration path now beats the preserved `64x32_1m` baseline of `16.1089 MB/s`
- `64x32_4m` had already beaten the preserved baseline
- this means the FlatWork migration has now cleared both key acceptance gates and is ready to be treated as the new
  preferred experimental mainline for LeopardGF8 encode

## 15. Broader Matrix Check

Additional regression-matrix evidence for the `FlatWork`-driven Leopard encode path:

- `32x16_1m`: `20.2759 MB/s`
- `32x16_4m`: `20.8361 MB/s`
- `64x32_64k`: `15.3197 MB/s`
- `64x32_1m`: `16.4482 MB/s`
- `64x32_4m`: `16.2620 MB/s`
- `128x64_1m`: `6.8649 MB/s`

Interpretation:

- the migration is no longer justified by only one or two isolated wins
- it now shows a broader pattern of competitive or winning encode results across adjacent high-fanout shapes
- this is strong enough evidence to keep the `FlatWork` path as the preferred LeopardGF8 experimental mainline
- but the `128x64_1m` reading shows that the current implementation still has another scaling cliff at the next fanout
  tier, so further work is required before claiming broad high-count readiness

## 16. `128x64_1m` Stage Profile

Profile artifact:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`

Observed counters:

- `encode_calls`: `2`
- `encode_chunks`: `64`
- `encode_later_group_calls`: `64`
- `fft_stage_calls`: `64`
- `ifft_stage_calls`: `128`

Interpretation:

- the next scaling cliff is not caused by remainder handling
- it is driven by the stage-level repetition count at higher fanout
- future work should prioritize reducing repeated later-group / FFT / IFFT stage cost rather than continuing local
  lane-view or zero-buffer micro-tuning

Follow-up experiment result:

- increasing chunk size alone for `>= 192` total shards did not resolve the cliff
- `128x64_1m` remained around `6.9985 MB/s`

Updated interpretation:

- the `128x64_1m` bottleneck is not primarily a simple chunk-count problem
- future work should look at stage reuse / consolidation rather than only chunk-size scaling

## 14. Immediate Recommendation

Continue `Task 15` only if the next round is willing to treat the `FlatWork` path as an experiment branch that must
beat both preserved baseline numbers before it replaces them.

Otherwise, keep the current best known baseline as the accepted mainline and continue `FlatWork` as a parallel
migration effort.
