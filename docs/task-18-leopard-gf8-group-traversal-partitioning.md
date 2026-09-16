# Task 18: Leopard GF8 Group Traversal Partitioning

## 1. Goal

Reduce the remaining `128x64_1m` gap in the current Leopard GF8 encode path by changing higher-level group traversal
and work partitioning shape, rather than continuing local copy/xor helper micro-tuning.

This task begins only after Task 17 established a stable retained baseline and a growing rejected-experiments list for
lower-level traffic tweaks.

## 2. Why This Task Exists

Task 17 proved two things at the same time:

1. some traffic reductions were real wins
2. repeatedly tweaking the same local helper layer quickly hit diminishing returns or regressions

The currently retained baseline is:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile artifact: `11.4999 MB/s`

The retained byte-traffic split for `128x64_1m` is:

- `input_copy_bytes = 268435456`
- `xor_bytes = 134217728`
- `output_writeback_bytes = 134217728`
- `zero_fill_bytes = 0`

However, the following low-level follow-ups were already rejected:

- parity-output aliasing
- direct stage-1 materialization rewrite
- full `available == 4` materialization copy-shape rewrite
- custom batched parity writeback helper
- `u64` word-wise `slice_xor(...)`
- manual `with_lane_views(...)` build-path rewrite

That is strong evidence that the next slice should not keep attacking the same local helper surfaces.

## 3. New Cut

The next small cut is:

- reorganize Leopard GF8 encode traversal around group-level partitioning rather than helper-level micro-optimization

More specifically:

- examine how the first group, later full groups, and remainder groups are driven through `encode_with_tables(...)`
- look for a better chunk/work partitioning shape that reduces hot-path coordination overhead without rewriting
  butterfly math or replaying Task 17 traffic experiments

This is intentionally a higher-level slice than Task 17, but still small enough to benchmark independently.

## 4. Current Code Anchors

- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8_with_plan(...)`
  - `fft_dit8_with_plan(...)`
- [src/core/leopard_gf8/work.rs](../src/core/leopard_gf8/work.rs:1)
  - `FlatWork`
- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - `build_leopard_gf8_encode_driver(...)`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`

## 5. Core Hypothesis

Now that:

- stage plans are real
- the retained butterfly kernel is stable
- obvious zero/xor helper wins were already captured

the next worthwhile gain is more likely to come from how groups are partitioned and traversed per chunk than from
further byte-loop surgery.

In particular, one of these is likely true:

1. the current first-group + later-group + remainder-group loop structure still creates avoidable hot-path coordination
2. the current `work_size` / temp-work partitioning is broader than needed for the common `128x64_1m` shape
3. the encode path is paying overhead because the traversal shape is still organized around a generic prototype flow
   rather than the actual retained Leopard GF8 steady state

## 6. In Scope

- restructure group traversal inside `encode_with_tables(...)`
- reduce coordination overhead across first/later/remainder group passes
- revisit work partitioning shape only at the group/chunk level
- keep benchmarking centered on `128x64_1m` plus `64x32_1m`

## 7. Out of Scope

- more `slice_xor(...)` micro-tuning
- more `copy_from_slice(...)` micro-shape rewrites
- more `with_lane_views(...)` micro-tuning
- NEON or other SIMD revivals
- changing the retained `4x` butterfly math kernel
- Leopard verify/reconstruct work

## 8. Execution Plan

### Step 1

Preserve the current retained baseline:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`

Retained baseline values:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- profile: `11.4999 MB/s`

### Step 2

Inspect `encode_with_tables(...)` as a traversal problem, not a helper problem:

- first group path
- later full-group path
- remainder-group path
- temp-work partitioning

### Step 3

Prototype exactly one traversal- or partitioning-level change.

Good candidates:

1. narrow the common-path work partition for full-group accumulation
2. split the common `remainder == 0` case away from the generic path
3. reduce repeated branching/coordination inside the later-group loop

### Step 4

Re-run:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_profile_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_1m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 5

Keep the change only if it beats the retained Task 17 baseline on `128x64_1m` without materially hurting
`64x32_1m`.

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it attacks group traversal / partitioning rather than reopening rejected Task 17 helper experiments
2. `128x64_1m` improves meaningfully from the current `~11.5 MB/s` band
3. `64x32_1m` does not regress materially from the current `~32.3 MB/s` band
4. the retained result is explainable as better traversal/partitioning, not just benchmark noise

## 10. Risks

### R1. Too abstract, not enough leverage

Mitigation:

- prototype exactly one traversal-level change
- reject it quickly if the benchmark does not move

### R2. Accidentally reintroducing rejected helper work

Mitigation:

- do not change `slice_xor(...)`, `copy_from_slice(...)` helper shapes, or the `4x` butterfly kernel in this task

### R3. Losing the stable benchmark entrypoint

Mitigation:

- continue using the direct stable binary invocation via `rustup which`
- do not rely on the local `rustup` wrapper until that environment issue is separately resolved

## 11. Current Recommendation

Proceed with a dedicated group-traversal partitioning slice as Task 18.

Task 17 already narrowed the local helper space enough that continuing there is now low-value and high-risk. The next
credible small cut is one level up: the shape of the encode traversal itself.

## 12. First Cut Result

The first Task 18 prototype is now rejected.

Prototype that was tested:

- specialize the common `data_shards == 2 * m && last_count == 0` path
- treat the common two-full-group / no-remainder encode shape as a dedicated traversal fast path
- leave the retained helper kernels unchanged

Measured result:

- `128x64_1m` profile: `11.3490 MB/s`
- `128x64_1m`: `11.1448 MB/s`
- `64x32_1m`: `31.3452 MB/s`

Compared with the retained Task 17 baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- profile baseline: `11.4999 MB/s`

Conclusion:

- the dedicated two-full-group traversal branch regressed both the target case and the control case
- the first Task 18 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- a branch specialized only for the common two-full-group / no-remainder case
- traversal fast paths that split the common shape away from the generic path without changing schedule metadata

Why it likely failed:

- the extra specialization did not remove enough real hot-path work
- it added control-flow / structure complexity without improving the retained execution shape

## 14. Next Recommendation

If work continues above the helper layer, the next cut should be smaller than the rejected traversal branch.

The most credible next slice is no longer traversal branching itself, but the data carried by the traversal:

- reduce or simplify the metadata and coordination needed for `later_ifft_plans`
- avoid repeating generic plan-carried bookkeeping when the common retained shape is already known

That next slice should be tracked as a new task instead of continuing to mutate Task 18 in place.
