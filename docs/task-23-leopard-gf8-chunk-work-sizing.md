# Task 23: Leopard GF8 Chunk And Work Sizing

## 1. Goal

Choose and validate the next meaningful optimization lever for Leopard GF8 by moving up one level from helper and
encode-loop micro-structure into chunk sizing and work-shape design.

This task is intentionally above Tasks 17, 18, 19, 20, and 22. It should not reopen their local experiments.

## 2. Why This Task Exists

The current retained Leopard GF8 baseline is already strong enough that repeatedly tweaking nearby encode-loop details
has become a poor use of iteration time.

Retained baseline:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile artifact: `11.4999 MB/s`

Retained phase/traffic evidence for `128x64_1m`:

- `encode_chunks = 16`
- `first_group_ifft_calls = 16`
- `later_group_ifft_calls = 16`
- `remainder_group_ifft_calls = 0`
- `first_group_input_copy_bytes = 134217728`
- `later_group_input_copy_bytes = 134217728`
- `later_group_xor_bytes = 134217728`
- `output_writeback_bytes = 134217728`

This evidence matters because it shows that:

1. no single tiny helper bucket is obviously dominating anymore
2. several large buckets are now comparable in size
3. the next worthwhile lever is less likely to be another local patch near the retained helper path

At the same time, the first cuts of the following tasks have all been rejected:

- Task 18: group traversal partitioning
- Task 19: group schedule metadata tightening
- Task 20: later-group bookkeeping tightening
- Task 22: later-group accumulation first cut

That is enough evidence that the next attempt should not stay in the same local layer.

## 3. New Direction

The next direction is:

- revisit chunk sizing and work-shape design as a larger encode-level lever

This means changing or validating:

- `WORK_SIZE8`
- `WORK_SIZE8_HIGH_FANOUT`
- the conditions that choose between them
- whether `work_slices = m * 2` is still the right retained shape at high fanout

This is a design-level slice, not a micro-tuning slice.

## 4. Current Code Anchors

- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - `build_leopard_gf8_encode_driver(...)`
- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - `WORK_SIZE8`
  - `WORK_SIZE8_HIGH_FANOUT`
- [src/core/leopard_gf8/work.rs](../src/core/leopard_gf8/work.rs:1)
  - `FlatWork`
- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`
  - `benchmark_leopard_encode_128x64_4m_exports_results`
  - `benchmark_leopard_encode_64x32_4m_exports_results`

## 5. Core Hypothesis

The current retained implementation may now be limited more by chunk/work partitioning choices than by helper code.

In particular, one of these may be true:

1. the current `128 KiB` high-fanout chunk is no longer the best size after the retained Task 17 traffic wins
2. the current low-vs-high-fanout chunk threshold is too coarse
3. `work_slices = m * 2` may be broader than necessary for the retained common path
4. a larger lever such as chunk/work shape can unlock gains that local encode-loop rewrites could not

## 6. In Scope

- chunk-size selection for Leopard GF8 encode
- high-fanout threshold selection
- work-slice budgeting at the encode-driver level
- benchmark comparison across both `1m` and `4m` shapes

## 7. Out Of Scope

- helper-level rewrites in `slice_xor(...)`, butterfly code, or input-copy micro-shapes
- traversal branch specialization
- schedule metadata container changes
- later-group bookkeeping tweaks
- SIMD work
- verify/reconstruct Leopard work

## 8. Execution Plan

### Step 1

Preserve the retained baseline artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`

Retained baseline values:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

### Step 2

Use the already available adjacent topology artifacts as design evidence, especially:

- `128x64_4m`
- `64x32_4m`

The point is to avoid choosing a chunk/work shape that helps only one topology.

### Step 3

Prototype exactly one design-level change.

Good candidates:

1. revise the high-fanout chunk threshold
2. revise the high-fanout chunk size itself
3. revise the driver work-slice budget

### Step 4

Re-run:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_profile_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_4m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_4m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 5

Keep the change only if:

- `128x64_1m` improves from the retained baseline
- `64x32_1m` does not materially regress
- the nearby `4m` shapes do not show a clear regression

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it genuinely moves to a larger lever than Tasks 17/18/19/20/22
2. `128x64_1m` improves meaningfully from the current `~11.5 MB/s` band
3. `64x32_1m` remains stable
4. the retained change also makes sense against nearby `4m` topologies

## 10. Risks

### R1. Overfitting Chunk Size To One Topology

Mitigation:

- always pair `128x64_1m` with `128x64_4m` and `64x32_4m`

### R2. Reopening The Same Local Layer By Accident

Mitigation:

- do not touch helper kernels or loop bookkeeping in this task

### R3. Bigger Lever, Bigger Regression Surface

Mitigation:

- change exactly one driver-level design variable at a time

## 11. Current Recommendation

Proceed with a dedicated chunk/work sizing design slice as Task 23.

After Direction A clarified the retained profile and Tasks 18/19/20/22 rejected nearby local rewrites, the next
credible move is to change a larger encode-level lever instead of continuing to sand down the same local layer.

## 12. First Cut Result

The first Task 23 prototype is now rejected.

Prototype that was tested:

- raise `WORK_SIZE8_HIGH_FANOUT` from `128 KiB` to `256 KiB`
- leave all other encode logic unchanged
- validate the change across the retained `1m + 4m` benchmark set

Measured result:

- `128x64_1m` profile: `10.8329 MB/s`
- `128x64_1m`: `10.8666 MB/s`
- `64x32_1m`: `5.1907 MB/s`
- `128x64_4m`: `11.4158 MB/s`
- `64x32_4m`: `32.1652 MB/s`

Compared with the retained baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- `128x64_4m` baseline: `8.5797 MB/s`
- `64x32_4m` baseline: `20.8331 MB/s`

Conclusion:

- although the higher chunk size helped some high-fanout measurements, it catastrophically regressed `64x32_1m`
- the first Task 23 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- increasing `WORK_SIZE8_HIGH_FANOUT` to `256 KiB` as a single retained driver-level design change

Why it likely failed:

- the change altered chunk behavior for adjacent shapes too aggressively
- it overfit part of the high-fanout space while badly damaging the `64x32_1m` control case

## 14. Next Recommendation

If work continues at the design level, the next cut should be narrower than this rejected chunk-size jump.

More credible follow-up directions would be:

- a threshold change without changing the chunk size itself
- a work-slice budgeting change without changing chunk size
- a broader decision-only benchmark task before any further design variable is changed
