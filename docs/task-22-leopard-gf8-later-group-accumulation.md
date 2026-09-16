# Task 22: Leopard GF8 Later-Group Accumulation Path

## 1. Goal

Reduce the remaining `128x64_1m` gap in the current Leopard GF8 encode path by targeting the retained later-group
accumulation path, using the finer-grained profile evidence added in Task 21 Direction A.

This task should begin only after treating the new phase-level profile as the authoritative direction-setting evidence.

## 2. Why This Task Exists

Task 17 established the current retained baseline:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

Task 21 Direction A then expanded the Leopard profile into per-phase buckets and produced this `128x64_1m` diagnosis:

- `first_group_ifft_calls = 16`
- `later_group_ifft_calls = 16`
- `remainder_group_ifft_calls = 0`
- `first_group_input_copy_bytes = 134217728`
- `later_group_input_copy_bytes = 134217728`
- `later_group_xor_bytes = 134217728`
- `output_writeback_bytes = 134217728`

Interpretation:

- there is no remainder-path opportunity in the retained `128x64_1m` baseline
- first-group work owns one large input-copy bucket
- output writeback owns one large write bucket
- later-group accumulation is the only retained stage that owns both a large input-copy bucket and the full xor bucket

That makes later-group accumulation the highest-confidence remaining execution stage to target next.

## 3. What This Task Is Not

This is not:

- another helper-level butterfly rewrite
- another traversal-branch specialization
- another schedule-container replacement
- another later-group bookkeeping-only rewrite
- another generic copy-shape-only experiment

Tasks 18, 19, and 20 already rejected those families.

## 4. New Cut

Target only the retained later-group accumulation path:

- the path where later groups are materialized into `temp_work`
- the path where those results are xor-folded into `xor_dst`

The intent is to reduce real later-group work while preserving the retained global execution shape.

## 5. Current Code Anchors

- [src/core/leopard_gf8/encode.rs](../src/core/leopard_gf8/encode.rs:1)
  - `encode_with_tables(...)`
  - `ifft_dit_encoder8_with_plan(...)`
- [src/core/leopard_gf8/ops.rs](../src/core/leopard_gf8/ops.rs:1)
  - retained `slice_xor(...)`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`

## 6. Core Hypothesis

The next worthwhile gain is more likely to come from narrowing or fusing later-group accumulation work than from
further modifying:

- first-group-only behavior
- output writeback behavior
- traversal or schedule metadata shape

Specifically, one of these is likely true:

1. later-group materialization and xor-folding still perform avoidable two-pass work
2. later-group accumulation still uses a generic temp-work path that is broader than the retained shape requires
3. the retained xor folding pays full-lane traffic that could be reduced only inside the later-group path

## 7. In Scope

- later-group-only materialization/accumulation changes
- retaining the current first-group and final writeback structure
- retaining the current helper kernels unless a later-group-only change requires a local adapter

## 8. Out of Scope

- first-group direct-stage rewrites
- output writeback replacement
- traversal branch specialization
- metadata-container replacement
- helper-level butterfly rewrites
- SIMD work

## 9. Execution Plan

### Step 1

Preserve the retained Task 17 baseline artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`

### Step 2

Use the new phase-level profile as the task entry evidence:

- no remainder work for `128x64_1m`
- later-group path owns both input-copy and xor traffic

### Step 3

Prototype exactly one later-group-only change.

Good candidates:

1. narrow/fuse later-group temp-work materialization and xor-folding
2. remove redundant later-group full-lane traffic without changing the retained global path

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

## 10. Acceptance Criteria

This task should be considered successful only if:

1. it is explicitly guided by the new phase-level profile evidence
2. it attacks later-group accumulation only
3. `128x64_1m` improves meaningfully from the retained `~11.5 MB/s` band
4. `64x32_1m` does not regress materially from the retained `~32.3 MB/s` band

## 11. Current Recommendation

Proceed with a dedicated later-group accumulation slice as Task 22.

After rejecting helper rewrites, traversal branching, schedule replacement, and bookkeeping-only changes, the best
remaining evidence-backed cut is the retained later-group accumulation path itself.

## 12. First Cut Result

The first Task 22 prototype is now rejected.

Prototype that was tested:

- keep the retained helper kernels and traversal shape unchanged
- share a single `split_at_mut(driver.m)` across later-group and remainder accumulation
- keep the change scoped only to the later-group accumulation path

Measured result:

- `128x64_1m` profile: `9.0656 MB/s`
- `128x64_1m`: `10.3334 MB/s`
- `64x32_1m`: `30.1513 MB/s`

Compared with the retained Task 17 baseline:

- `128x64_1m` baseline: `11.4957 MB/s`
- `64x32_1m` baseline: `32.2955 MB/s`
- profile baseline: `11.4999 MB/s`

Conclusion:

- the later-group accumulation bookkeeping cut regressed both the target case and the control case
- the first Task 22 cut should not be kept

## 13. Rejected First Cut

Do not retry this exact idea without new evidence:

- hoisting the later-group/remainder `split_at_mut(driver.m)` out of the retained loop
- sharing one `xor_dst`/`temp_work` split across the whole later-group walk

Why it likely failed:

- the later-group accumulation path is sensitive to subtle execution-shape changes
- reducing one small bookkeeping step did not remove enough real work to offset the changed hot-path shape

## 14. Next Recommendation

At this point, helper-level traffic cuts have already produced the retained wins, and the first cuts of Tasks 18, 19,
20, and 22 have all regressed.

That means the next step should not be another immediate encode-path patch in the same family.

Instead:

- keep the retained Task 17 baseline in code
- preserve the richer profile exporter from Task 21 Direction A
- use the accumulated retained/rejected evidence to choose a genuinely new direction before resuming implementation
