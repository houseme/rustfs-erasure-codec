# Task 19: Leopard GF8 Group Schedule Metadata Tightening

## 1. Goal

Reduce the remaining `128x64_1m` gap in the current Leopard GF8 encode path by tightening group-level schedule
metadata and coordination, without reopening helper-level micro-tuning or the rejected traversal branch from Task 18.

## 2. Why This Task Exists

Task 17 established a strong retained baseline through copy-traffic reduction, but also built a large rejected list for
helper-level experiments.

Retained baseline:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

Retained traffic split for `128x64_1m`:

- `input_copy_bytes = 268435456`
- `xor_bytes = 134217728`
- `output_writeback_bytes = 134217728`
- `zero_fill_bytes = 0`

Task 18 then tested a traversal-branch specialization for the common two-full-group/no-remainder case and rejected it:

- `128x64_1m` profile: `11.3490 MB/s`
- `128x64_1m`: `11.1448 MB/s`
- `64x32_1m`: `31.3452 MB/s`

That means the next credible higher-level slice should be smaller than a new traversal branch. The next suspect is not
the branch structure itself, but the metadata and coordination carried through it.

## 3. New Cut

This task focuses on schedule metadata, not helper math and not branch specialization.

Target surfaces:

- `later_ifft_plans`
- `remainder_ifft_plan`
- per-chunk coordination around `group_offset` and plan dispatch
- any repeated generic plan-carried state that can be narrowed for the retained LeopardGF8 steady state

## 4. Current Code Anchors

- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8_with_plan(...)`
- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - `IfftDit8Plan`
  - `FftDit8Plan`
- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - retained driver shape for `m`, `mtrunc`, `last_count`, `chunk_size`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`

## 5. Core Hypothesis

The next worthwhile gain is more likely to come from reducing metadata/coordination carried through the group loop than
from rewriting the traversal shape or replaying helper-level experiments.

In particular, one of these is likely true:

1. the current `later_ifft_plans: Vec<_>` plus runtime `group_offset` walk still carries more generic coordination than
   the retained common shape needs
2. remainder handling is already rare enough that its metadata should be isolated more cleanly from the common path
3. the encode hot path still pays for schedule generality that is no longer buying correctness or flexibility

## 6. In Scope

- tighten plan/offset metadata around later-group dispatch
- separate rare remainder metadata from the common retained path if that reduces hot-path coordination
- reduce repeated generic schedule bookkeeping in `encode_with_tables(...)`

## 7. Out of Scope

- helper-level changes to `slice_xor(...)`, `copy_from_slice(...)`, `with_lane_views(...)`, or butterfly code
- new traversal branch specialization like the rejected Task 18 first cut
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

Inspect `encode_with_tables(...)` as a schedule-metadata problem:

- how `later_ifft_plans` is built
- how `group_offset` walks the later groups
- how the rare `remainder_ifft_plan` still influences common-path structure

### Step 3

Prototype exactly one metadata-tightening change.

Good candidates:

1. replace the current generic later-plan walk with a narrower common-path metadata structure
2. isolate remainder metadata from the common hot path more aggressively
3. reduce plan-carried bookkeeping that is re-derived or re-walked per chunk

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

1. it attacks schedule metadata rather than reopening rejected helper or traversal-branch experiments
2. `128x64_1m` improves meaningfully from the current `~11.5 MB/s` band
3. `64x32_1m` does not regress materially from the current `~32.3 MB/s` band
4. the retained result is explainable as lower coordination/schedule overhead rather than benchmark noise

## 10. Risks

### R1. Metadata tightening changes too little

Mitigation:

- keep the cut narrow
- reject quickly if benchmark movement is absent

### R2. Accidentally recreating the rejected Task 18 traversal branch

Mitigation:

- do not split out a new common-case branch
- focus only on what metadata the common path carries

### R3. Reopening rejected helper experiments indirectly

Mitigation:

- keep helper code untouched during this task

## 11. Current Recommendation

Proceed with a dedicated group-schedule metadata slice as Task 19.

Task 18 already showed that branch specialization alone is not enough. The next credible higher-level cut is smaller:
carry less generic schedule baggage through the common path.

## 12. First Cut Result

The first Task 19 prototype is now rejected.

Prototype that was tested:

- replace the split `later_ifft_plans + remainder_ifft_plan + group_offset` coordination with a prebuilt later-group
  schedule
- carry `data_offset + plan + profile_kind` through a single metadata structure
- leave the retained helper kernels and traffic helpers unchanged

Measured result:

- `128x64_1m` profile: `11.2252 MB/s`
- `128x64_1m`: `11.0231 MB/s`
- `64x32_1m`: `31.0940 MB/s`

Compared with the retained Task 17 baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- profile baseline: `11.4999 MB/s`

Conclusion:

- the unified later-group schedule metadata path regressed both the target case and the control case
- the first Task 19 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- prebuilding a single later-group schedule object that combines full-group and remainder-group metadata
- replacing the current split coordination with a single `data_offset + plan + profile_kind` schedule walk

Why it likely failed:

- the new metadata structure reduced conceptual branching but did not reduce enough real hot-path work
- it added coordination/state that still had to be walked per chunk without changing the retained execution shape

## 14. Next Recommendation

If work continues above the helper layer, the next slice should be smaller again.

The most credible next cut is now not schedule-container replacement, but only the bookkeeping around the retained
later-group walk:

- reduce later-group loop bookkeeping without changing plan ownership or shape
- keep remainder handling isolated
- avoid introducing new schedule structs unless a profile shows they remove real work

That next slice should be tracked as a new task instead of continuing to mutate Task 19 in place.
