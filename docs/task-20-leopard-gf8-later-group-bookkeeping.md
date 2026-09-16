# Task 20: Leopard GF8 Later-Group Bookkeeping Tightening

## 1. Goal

Reduce the remaining `128x64_1m` gap in the current Leopard GF8 encode path by tightening bookkeeping inside the
retained later-group loop, without changing helper kernels, traversal branch shape, or schedule-container ownership.

## 2. Why This Task Exists

Task 17 established the current retained baseline through copy-traffic reduction.

Retained baseline:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

Task 18 then rejected a traversal fast path for the common two-full-group/no-remainder case.

Rejected Task 18 first cut:

- `128x64_1m` profile: `11.3490 MB/s`
- `128x64_1m`: `11.1448 MB/s`
- `64x32_1m`: `31.3452 MB/s`

Task 19 then rejected a unified later-group schedule metadata structure.

Rejected Task 19 first cut:

- `128x64_1m` profile: `11.2252 MB/s`
- `128x64_1m`: `11.0231 MB/s`
- `64x32_1m`: `31.0940 MB/s`

That sequence means the next higher-level slice must be smaller still:

- do not change branch structure like Task 18
- do not replace schedule ownership/shape like Task 19
- only tighten what the retained later-group walk does per iteration

## 3. New Cut

This task focuses only on later-group loop bookkeeping.

Target surfaces inside `encode_with_tables(...)`:

- repeated `group_offset` handling
- repeated profile-counter branching inside the loop
- repeated split/dispatch scaffolding around `xor_dst` and `temp_work`
- any hot-path coordination that can be narrowed without introducing a new schedule container

## 4. Current Code Anchors

- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8_with_plan(...)`
- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - retained profile counters
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`

## 5. Core Hypothesis

There may still be measurable overhead in the retained later-group walk even after rejecting:

- helper-level micro-tuning
- traversal branch specialization
- schedule-container replacement

If the current loop still pays unnecessary coordination cost per group, then a smaller bookkeeping-only reduction may
improve `128x64_1m` without destabilizing the retained execution shape.

## 6. In Scope

- narrow later-group loop bookkeeping
- simplify counter/branch/update coordination in the retained loop
- reduce per-iteration scaffolding that does not change helper math or plan shape

## 7. Out of Scope

- helper changes in `slice_xor(...)`, butterfly code, or copy helpers
- branch specialization like Task 18
- metadata-container replacement like Task 19
- SIMD work
- Leopard verify/reconstruct

## 8. Execution Plan

### Step 1

Preserve the retained Task 17 baseline:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`

Retained baseline values:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- profile: `11.4999 MB/s`

### Step 2

Inspect only the retained later-group loop bookkeeping:

- `group_offset` updates
- profile-counter updates
- repeated split/dispatch coordination
- remainder isolation from the common path

### Step 3

Prototype exactly one bookkeeping-level change.

Good candidates:

1. pre-split common later-group and rare remainder bookkeeping without introducing a new schedule object
2. reduce repeated loop-local counter/offset coordination
3. simplify the retained full-group path around `xor_dst` / `temp_work` setup

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

1. it attacks later-group bookkeeping only
2. `128x64_1m` improves meaningfully from the retained `~11.5 MB/s` band
3. `64x32_1m` does not regress materially from the retained `~32.3 MB/s` band
4. the retained result is explainable as lower bookkeeping overhead, not as a reintroduced Task 18/19 direction

## 10. Risks

### R1. The cut is too small to matter

Mitigation:

- keep it extremely narrow
- reject quickly if the benchmark does not move

### R2. It silently recreates a rejected branch or schedule idea

Mitigation:

- do not add a new common-case branch
- do not add a new schedule struct/container

### R3. It becomes helper tuning in disguise

Mitigation:

- leave helper code untouched in this task

## 11. Current Recommendation

Proceed with a later-group bookkeeping-only slice as Task 20.

After Task 18 and Task 19 were both rejected, the next credible cut is even smaller: reduce only the coordination
inside the retained later-group loop, without changing traversal shape or schedule ownership.

## 12. First Cut Result

The first Task 20 prototype is now rejected.

Prototype that was tested:

- move `split_at_mut(driver.m)` out of the later-group loop
- drive later groups through a sliding `later_data` window instead of updating `group_offset` inside the loop
- keep helper math, traversal shape, and schedule ownership unchanged

Measured result:

- `128x64_1m` profile: `11.0687 MB/s`
- `128x64_1m`: `10.9683 MB/s`
- `64x32_1m`: `30.6171 MB/s`

Compared with the retained Task 17 baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- profile baseline: `11.4999 MB/s`

Conclusion:

- the bookkeeping-only later-group rewrite still regressed both the target case and the control case
- the first Task 20 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- hoisting the `split_at_mut(driver.m)` call out of the retained later-group loop
- replacing the retained `group_offset += driver.m` walk with a sliding slice window over `data`

Why it likely failed:

- the retained loop bookkeeping was not the dominant hot-path cost
- the rewrite changed coordination shape without reducing enough real work per chunk

## 14. Next Recommendation

If work continues above the helper layer, the next slice must be smaller or orthogonal again.

The current evidence suggests:

- helper-level retained wins are real
- traversal branching was rejected
- schedule-container replacement was rejected
- later-group bookkeeping tightening was also rejected

That means the next credible cut should not keep iterating within the same encode-loop coordination family without a
new source of evidence.
