# Task 25: Leopard GF8 Work-Slices Budget

## 1. Goal

Test the narrowest remaining design-level variable in Leopard GF8 encode:

- the `work_slices` budget only

This task explicitly does not change:

- chunk size
- high-fanout threshold
- helper kernels
- traversal shape
- schedule metadata shape

## 2. Why This Task Exists

Task 23 already rejected the first broader chunk/work sizing change:

- increasing `WORK_SIZE8_HIGH_FANOUT` to `256 KiB` regressed the `64x32_1m` control case badly

That means the remaining design-level lever should be even narrower than changing chunk size or threshold.

The current retained baseline is still:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_4m`: `8.5797 MB/s`
- `64x32_4m`: `20.8331 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

## 3. New Cut

The only design variable this task may change is:

- `LeopardGf8EncodeDriver::work_slices`

Current behavior assumes:

- `work_slices = m * 2`

This task asks whether the driver can budget work lanes more precisely from the actual need for later-group
accumulation, without changing any other variable.

## 4. Current Code Anchors

- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - `build_leopard_gf8_encode_driver(...)`
- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - retained Leopard encode/profile exporters

## 5. Core Hypothesis

`work_slices = m * 2` may be broader than needed for some shapes, and a more exact budget could reduce retained
overhead without touching the helper or traversal layers.

The first cut should be extremely conservative:

- if a later-group accumulation path is required, keep `2 * m`
- if not, reduce the budget to only the slices actually needed

## 6. In Scope

- compute `work_slices` from actual accumulation need
- keep all benchmark coverage on the retained `1m + 4m` shapes

## 7. Out Of Scope

- changing chunk size
- changing threshold
- changing `work_size` consumption semantics beyond respecting the new driver budget
- helper/traversal/metadata rewrites

## 8. Execution Plan

### Step 1

Preserve the retained baseline artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_4m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_4m.csv`

### Step 2

Implement exactly one change:

- derive `work_slices` from whether the encode path actually needs temp accumulation lanes

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
- nearby `4m` shapes do not clearly regress

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it changes only the `work_slices` budget
2. it does not reopen chunk-size/threshold/helper/traversal work
3. it improves or at least holds the retained `1m + 4m` comparison set

## 10. Current Recommendation

Proceed with a work-slices-only slice as Task 25.

After Task 23 rejected a chunk-size jump and Task 24 rejected a threshold change, `work_slices` is the last narrow
design variable worth testing before another direction reset.

## 11. First Cut Result

The first Task 25 prototype is now rejected.

Prototype that was tested:

- keep chunk sizes unchanged
- keep threshold unchanged
- reduce the driver `work_slices` budget to `m` only when no accumulation lanes are needed
- keep `work_slices = 2 * m` when accumulation is required

Measured result:

- `128x64_1m` profile: `11.4314 MB/s`
- `128x64_1m`: `11.1230 MB/s`
- `64x32_1m`: `31.4200 MB/s`
- `128x64_4m`: `11.0363 MB/s`
- `64x32_4m`: `30.2685 MB/s`

Compared with the retained baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- `128x64_4m` baseline: `8.5797 MB/s`
- `64x32_4m` baseline: `20.8331 MB/s`

Conclusion:

- the narrower work-slices budget improved some `4m` shapes
- but it regressed both retained `1m` control cases
- the first Task 25 cut should not be kept

## 12. Rejected First Cut

Do not retry this exact idea without new evidence:

- reducing `work_slices` to `m` when no accumulation lanes are needed as a standalone design change

Why it likely failed:

- the smaller work budget helped some larger-shard cases
- but it reduced flexibility or locality enough to hurt the retained `1m` control shapes

## 13. Next Recommendation

At this point:

- helper-level first cuts have mostly been exhausted
- traversal/schedule/bookkeeping first cuts have been rejected
- chunk-size, threshold, and work-slices first cuts have also been rejected

The next step should therefore be another direction reset rather than another immediate local or design-level patch.
