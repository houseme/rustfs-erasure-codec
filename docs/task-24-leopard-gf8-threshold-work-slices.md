# Task 24: Leopard GF8 Threshold And Work-Slice Budgeting

## 1. Goal

Define the next narrower design-level optimization slice for Leopard GF8 by limiting the search space to:

- the high-fanout threshold that decides when the alternate chunk regime activates
- the encode-driver `work_slices` budget

This task explicitly avoids changing chunk size itself as the first move, because Task 23 already showed that a coarse
chunk-size jump can badly damage nearby retained control cases.

## 2. Why This Task Exists

The current retained baseline remains:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

Recent task history is now clear:

- Task 17 retained real wins in zero-fill and xor traffic
- Tasks 18, 19, 20, and 22 all rejected increasingly small encode-loop / traversal / metadata cuts
- Task 23 rejected the first broader design cut: raising `WORK_SIZE8_HIGH_FANOUT` from `128 KiB` to `256 KiB`

The rejected Task 23 result matters because it shows that:

- a larger design lever can move the benchmark
- but a too-coarse design jump can overfit part of the high-fanout space while catastrophically regressing
  adjacent control shapes

So the next design cut must be narrower than “change chunk size itself”.

## 3. New Cut

This task narrows the design space to exactly two levers:

1. the high-fanout threshold condition
2. the encode-driver `work_slices` budget

That means:

- do not change `WORK_SIZE8`
- do not change `WORK_SIZE8_HIGH_FANOUT` as the first move in this task
- do not reopen helper, traversal, schedule, or accumulation experiments

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
  - retained Leopard encode/profile exporters

## 5. Core Hypothesis

The next viable design gain may come from changing when the high-fanout path is chosen, or how much work storage it
reserves, rather than how big the high-fanout chunk itself is.

In particular, one of these may be true:

1. the current high-fanout threshold is too eager or too broad
2. the retained high-fanout chunk size is acceptable, but the shapes that activate it are wrong
3. `work_slices = m * 2` may be broader than necessary in the retained common path
4. a narrower design cut can preserve the `64x32_*` control cases while still helping `128x64_*`

## 6. In Scope

- adjust the high-fanout threshold condition only
- adjust the encode-driver `work_slices` budget only
- benchmark against both retained `1m` and `4m` shapes

## 7. Out Of Scope

- changing `WORK_SIZE8_HIGH_FANOUT` again as the first cut
- helper-level changes
- traversal / metadata / bookkeeping changes from Tasks 18/19/20
- later-group accumulation changes from Task 22
- SIMD work

## 8. Execution Plan

### Step 1

Preserve the retained baseline artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_4m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_4m.csv`

Retained baseline values:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_4m`: `8.5797 MB/s`
- `64x32_4m`: `20.8331 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

### Step 2

Prototype exactly one design variable at a time:

Candidate A:

- change only the high-fanout threshold

Candidate B:

- change only the `work_slices` budget

Do not combine them in the first cut.

### Step 3

Re-run:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_profile_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_4m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_4m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 4

Keep the change only if:

- `128x64_1m` improves from the retained baseline
- `64x32_1m` does not materially regress
- the nearby `4m` cases do not show a clear regression

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it stays within the narrower design slice
2. it does not reopen the rejected Task 23 chunk-size jump
3. it improves `128x64_1m` without materially hurting the `64x32_*` control cases
4. the retained result is explainable as a threshold/work-budget win rather than noise

## 10. Risks

### R1. Threshold Change Does Nothing

Mitigation:

- reject quickly and do not stack more design changes on top

### R2. Work-Slice Change Creates Hidden Coordination Or Allocation Costs

Mitigation:

- keep the first cut limited to the driver budget only

### R3. The Design Slice Is Still Too Local

Mitigation:

- if both threshold and work-slice first cuts fail, stop again and re-open direction selection rather than trying more
  nearby variants

## 11. Current Recommendation

Proceed with a threshold/work-slice budgeting slice as Task 24.

After Task 23 showed that changing chunk size itself is too blunt, the next credible design move is to change when the
high-fanout path activates, or how much work state it reserves, without changing the chunk size again immediately.

## 12. First Cut Result

The first Task 24 prototype is now rejected.

Prototype that was tested:

- lower the high-fanout activation threshold from `192` total shards to `96`
- leave chunk sizes and helper logic unchanged
- validate the change against the retained `1m + 4m` benchmark set

Measured result:

- `128x64_1m` profile: `10.5908 MB/s`
- `128x64_1m`: `10.4835 MB/s`
- `64x32_1m`: `28.3505 MB/s`
- `128x64_4m`: `10.7151 MB/s`
- `64x32_4m`: `30.8250 MB/s`

Compared with the retained baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- `128x64_4m` baseline: `8.5797 MB/s`
- `64x32_4m` baseline: `20.8331 MB/s`

Conclusion:

- lowering the high-fanout threshold regressed both retained `1m` control cases
- the first Task 24 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- lowering the high-fanout activation threshold to `96` total shards as a standalone design change

Why it likely failed:

- it activated the alternate chunk regime too broadly
- it helped some `4m` high-fanout behavior while dragging down the retained `1m` control cases

## 14. Next Recommendation

If work continues at the design level, the next narrower design cut should be:

- keep the current threshold
- keep the current chunk sizes
- try only `work_slices` budgeting as the next single design variable

Do not combine threshold and work-budget changes in the next cut.
