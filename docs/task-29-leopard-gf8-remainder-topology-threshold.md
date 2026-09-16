# Task 29: Leopard GF8 Remainder-Topology Threshold

## 1. Goal

Target the newly explained `96x48` collapse by changing LeopardGF8 behavior only for remainder-heavy topologies, while
leaving the retained `64x32` and `128x64` common paths untouched.

## 2. Why This Task Exists

Task 27 broadened the benchmark decision surface and showed that `96x48` collapses between two stronger neighboring
shapes.

Measured results:

### 1m

- `64x32_1m`: `30.3994 MB/s`
- `96x48_1m`: `8.8766 MB/s`
- `128x64_1m`: `10.3648 MB/s`

### 4m

- `64x32_4m`: `25.1694 MB/s`
- `96x48_4m`: `8.5032 MB/s`
- `128x64_4m`: `10.7678 MB/s`

Task 28 then added a same-schema profile for `96x48_1m` and explained the difference:

- `encode_chunks = 64`
- `first_group_ifft_calls = 64`
- `later_group_ifft_calls = 0`
- `remainder_group_ifft_calls = 64`
- `first_group_input_copy_bytes = 134217728`
- `remainder_group_input_copy_bytes = 67108864`
- `remainder_group_zero_fill_bytes = 67108864`
- `remainder_group_xor_bytes = 134217728`
- `output_writeback_calls = 64`

Compared with retained `128x64_1m`:

- `encode_chunks = 16`
- `remainder_group_ifft_calls = 0`
- `later_group_ifft_calls = 16`

Interpretation:

- `96x48` is not just “slower at this fanout”
- it is a remainder-heavy topology still stuck on the smaller chunk regime
- the next implementation cut should target that exact combination rather than another generic loop or helper tweak

## 3. New Cut

The next narrow design cut is:

- change chunk activation only for remainder-heavy high-fanout topologies

Concretely:

- do not change the retained chunk-size behavior for `64x32_*`
- do not change the retained chunk-size behavior for `128x64_*`
- only consider altering the threshold/path for shapes like `96x48_*` where:
  - `data_shards > m`
  - `last_count != 0`
  - total shard count is high but below the current `192` threshold

## 4. Current Code Anchors

- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - `build_leopard_gf8_encode_driver(...)`
- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - `WORK_SIZE8`
  - `WORK_SIZE8_HIGH_FANOUT`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_96x48_1m_exports_results`
  - `benchmark_leopard_encode_96x48_1m_exports_results`
  - `benchmark_leopard_encode_96x48_4m_exports_results`
  - retained neighboring control cases

## 5. Core Hypothesis

The simplest explanation of the `96x48` collapse is:

1. it pays the worst costs of remainder-heavy later accumulation
2. it still runs with the smaller chunk regime
3. its stronger neighbors avoid one of those two conditions

So the next credible implementation cut is not “general high fanout threshold change” but:

- a threshold/path change that only applies to remainder-heavy high-fanout shapes

## 6. In Scope

- driver-level gating for high-fanout chunk selection
- only when the topology is remainder-heavy
- benchmark validation against both the collapse case and the retained neighbors

## 7. Out Of Scope

- helper rewrites
- traversal/schedule/bookkeeping changes
- changing chunk size itself as the first move
- broad threshold changes that also retarget `64x32_*`

## 8. Execution Plan

### Step 1

Preserve the current evidence set:

- `96x48_1m` throughput + profile
- `64x32_1m`
- `128x64_1m`
- `96x48_4m`
- `64x32_4m`
- `128x64_4m`

### Step 2

Prototype exactly one narrow driver-level rule:

- allow the retained high-fanout chunk regime for remainder-heavy high-fanout shapes only

Examples of the kind of shape this is meant for:

- `96x48_*`

And explicitly not for:

- `64x32_*`
- `128x64_*`

### Step 3

Re-run:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_profile_96x48_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_96x48_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_96x48_4m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 4

Keep the change only if:

- `96x48_1m` improves meaningfully
- `96x48_4m` does not regress materially
- retained neighbors (`64x32_1m`, `128x64_1m`) do not materially regress

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it is directly justified by the new `96x48` profile evidence
2. it does not reopen the already rejected broad threshold change from Task 24
3. it improves the collapse case without damaging the retained neighboring shapes

## 10. Current Recommendation

Proceed with a remainder-topology-specific threshold slice as Task 29.

This is the first post-Route-C implementation cut that is directly justified by the new benchmark decision surface,
rather than by another generic nearby optimization idea.

## 11. First Cut Result

The first Task 29 prototype is retained.

Prototype that was tested:

- keep the current chunk sizes unchanged
- keep the broad `total_shards >= 192` high-fanout rule unchanged
- additionally allow the retained high-fanout chunk regime for remainder-heavy shapes where:
  - `total_shards >= 144`
  - `last_count != 0`

Measured result:

- `96x48_1m` profile: `8.7322 MB/s`
- `96x48_1m`: `8.8366 MB/s`
- `96x48_4m`: `9.0192 MB/s`
- `64x32_1m`: `30.9348 MB/s`
- `128x64_1m`: `10.9507 MB/s`

Compared with the immediately preceding decision-surface measurements:

- `96x48_1m`: `8.7537 -> 8.8366 MB/s`
- `96x48_4m`: `8.5032 -> 9.0192 MB/s`
- `64x32_1m`: `30.3994 -> 30.9348 MB/s`
- `128x64_1m`: `10.3648 -> 10.9507 MB/s`

What changed in the `96x48_1m` profile:

- `encode_chunks`: `64 -> 16`
- `output_writeback_calls`: `64 -> 16`
- `first_group_ifft_calls`: `64 -> 16`
- `remainder_group_ifft_calls`: `64 -> 16`

Conclusion:

- the collapse case improved
- the retained neighboring control points also improved
- this is the first post-Task-17 implementation cut in this family that clearly beats the nearby decision surface

## 12. Current Implication

The `96x48` collapse is now best explained as:

- a remainder-heavy topology that was being left on the small-chunk regime too long

That means the next implementation work, if any, should be built on top of this retained threshold refinement rather
than reopening the rejected Task 18/19/20/22/23/24/25 directions.
