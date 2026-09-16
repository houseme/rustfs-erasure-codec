# Task 30: Leopard GF8 Remainder-Path Follow-up

## 1. Goal

Continue LeopardGF8 optimization only within the remainder-heavy high-fanout region identified by Task 29, without
reopening the retained `64x32` / `128x64` neighboring paths as the primary experiment surface.

## 2. Why This Task Exists

Task 29 produced the first clearly positive post-Task-17 implementation cut:

- a remainder-topology-specific threshold refinement

Measured improvements:

- `96x48_1m`: `8.7537 -> 8.8366 MB/s`
- `96x48_4m`: `8.5032 -> 9.0192 MB/s`
- `64x32_1m`: `30.3994 -> 30.9348 MB/s`
- `128x64_1m`: `10.3648 -> 10.9507 MB/s`

And the same-schema profile for `96x48_1m` shows:

- `encode_chunks = 16`
- `first_group_ifft_calls = 16`
- `later_group_ifft_calls = 0`
- `remainder_group_ifft_calls = 16`
- `first_group_input_copy_bytes = 134217728`
- `remainder_group_input_copy_bytes = 67108864`
- `remainder_group_zero_fill_bytes = 67108864`
- `remainder_group_xor_bytes = 134217728`
- `output_writeback_calls = 16`

That means:

- the `96x48` collapse is no longer a generic high-fanout problem
- it is now specifically a remainder-path problem inside the retained large-chunk regime

## 3. New Cut

The next cut should stay entirely inside the retained `96x48`-style remainder path.

The new scope is:

- follow-up only on remainder-heavy shapes that now enter the high-fanout chunk regime
- do not reopen the neighboring `64x32` / `128x64` shapes as the primary optimization target

## 4. Current Code Anchors

- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - retained remainder-topology threshold refinement
- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8_with_plan(...)`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_96x48_1m_exports_results`
  - `benchmark_leopard_encode_96x48_1m_exports_results`
  - `benchmark_leopard_encode_96x48_4m_exports_results`

## 5. Core Hypothesis

Now that `96x48` is inside the retained large-chunk regime, the next worthwhile gain is likely to come from the
remainder path specifically rather than from generic high-fanout machinery.

In particular, one of these may be true:

1. remainder-group materialization still carries avoidable zero/input-copy work
2. remainder-group xor folding is now the dominant path worth targeting
3. remainder-group handling can be improved without touching the retained non-remainder neighboring shapes

## 6. In Scope

- remainder-group-only logic for the retained large-chunk regime
- `96x48_1m` and `96x48_4m` as the primary target cases
- `64x32_1m` / `128x64_1m` only as guardrails

## 7. Out Of Scope

- reopening generic high-fanout threshold changes
- changing chunk size again
- traversal/schedule/bookkeeping rewrites from Tasks 18/19/20
- broad helper rewrites for all shapes

## 8. Execution Plan

### Step 1

Preserve the retained Task 29 evidence:

- `target/benchmark-smoke/leopard-encode-profile-96x48_1m.csv`
- `target/benchmark-smoke/leopard-encode-96x48_1m.csv`
- `target/benchmark-smoke/leopard-encode-96x48_4m.csv`
- neighboring `64x32_1m` and `128x64_1m` artifacts

### Step 2

Prototype exactly one remainder-path-only change.

Examples:

- reduce remainder zero-fill work
- reduce remainder xor folding cost
- narrow remainder materialization without touching the common non-remainder path

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

- `96x48_1m` improves
- `96x48_4m` does not regress materially
- neighboring retained guardrails do not regress materially

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it stays inside the remainder-heavy post-Task-29 slice
2. it improves the collapse-region cases
3. it does not damage the retained neighboring shapes

## 10. Current Recommendation

Proceed with a remainder-path follow-up slice as Task 30.

Task 29 finally isolated a specific, successful new direction. The next implementation work should stay tightly inside
that successful slice instead of reopening the broader encode-path layers.
