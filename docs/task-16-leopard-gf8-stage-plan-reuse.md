# Task 16: Leopard GF8 Stage-Plan Reuse

## 1. Goal

Reduce the `128x64_1m` scaling cliff in the current Leopard GF8 encode path by turning the existing
`build_ifft_dit8_plan(...)` / `build_fft_dit8_plan(...)` helpers into real execution-time plan reuse, then carry
the follow-up butterfly inner-loop work only far enough to keep the best measured implementation.

This task starts from the conclusion that the current bottleneck is no longer local buffer micro-tuning, but repeated
stage scheduling work at higher fanout.

## 2. Why This Task Exists

The current Leopard GF8 encode path already has:

- a functioning pure-Rust encode kernel
- a `FlatWork` owner type
- lane-view-friendly helper signatures
- baseline wins on several `32x16` / `64x32` shapes

However, `128x64_1m` still showed a severe scaling cliff before the current round of reuse and butterfly work.

The current profile artifact after the accepted work:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`

shows:

- `encode_calls = 2`
- `encode_chunks = 16`
- `encode_later_group_calls = 16`
- `fft_stage_calls = 16`
- `ifft_stage_calls = 32`

This means the original repeated-stage problem was materially reduced, and the remaining gain came from tightening the
full-window butterfly inner loop rather than adding more scheduling work.

## 3. Current Baselines

Accepted high-confidence reference points:

- `64x32_1m`: `22.4391 MB/s`
- `64x32_4m`: `15.8374 MB/s`

Current retained point:

- `128x64_1m`: `8.9467 MB/s`

## 4. Scope

### 4.1 In scope

- make `build_ifft_dit8_plan(...)` a real reused execution plan
- make `build_fft_dit8_plan(...)` a real reused execution plan
- retain the best measured butterfly inner-loop implementation after reuse lands
- validate impact primarily on `128x64_1m`
- use `64x32_1m` as a control case to avoid breaking the stronger path

### 4.2 Out of scope

- further `zero` / `xor_clone` style micro-knobs
- verify/reconstruct Leopard migration
- chunk-size experimentation unless forced by evidence

## 5. Current Code Anchors

- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - `build_fft_dit8_plan(...)`
  - `build_ifft_dit8_plan(...)`
  - `fft_dit8(...)`
  - `ifft_dit_encoder8(...)`
  - `encode_with_tables(...)`

- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - `benchmark_leopard_encode_profile_128x64_1m_exports_results`
  - `benchmark_leopard_encode_128x64_1m_exports_results`
  - `benchmark_leopard_encode_64x32_1m_exports_results`

## 6. Core Hypothesis

The original implementation still recomputed or re-derived stage-driving structure too often across chunks and later
group passes.

If those stage layouts are precomputed once per encode call and reused directly, then:

- `128x64_1m` should improve noticeably
- `64x32_1m` should stay roughly flat

That hypothesis was correct, but only partially sufficient. Once plan reuse was real, the next meaningful gain came
from reducing repeated full-window butterfly slice scans. The best retained version is a fused `4x` butterfly helper.

## 7. Execution Plan

### Step 1

Keep the original pre-reuse profile artifact as the diagnostic baseline:

- `encode_chunks = 64`
- `encode_later_group_calls = 64`
- `fft_stage_calls = 64`
- `ifft_stage_calls = 128`

### Step 2

Turn `build_ifft_dit8_plan(...)` into a real plan object used by `ifft_dit_encoder8(...)`.

### Step 3

Turn `build_fft_dit8_plan(...)` into a real plan object used by `fft_dit8(...)`.

### Step 4

Ensure plans are built once per encode call and reused across every chunk.

### Step 5

Re-run:

```bash
cargo test benchmark_leopard_encode_profile_128x64_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_128x64_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_64x32_1m_exports_results -- --nocapture
```

### Step 6

Keep only the best measured butterfly implementation after reuse lands.

Accepted follow-up outcome:

- retain the fused `4x` butterfly helper in `src/core/leopard_gf8/ops.rs`
- do not keep the later `16-byte` chunking variant
- do not keep the experimental aarch64 NEON fast path, because it did not beat the retained scalar `4x` version on
  `128x64_1m`

## 8. Acceptance Criteria

This task should be considered a success only if:

1. the stage-plan helpers are no longer dead planning code
2. `128x64_1m` improves meaningfully from the original `~6.9 MB/s` band
3. `64x32_1m` does not regress materially from the original `~16 MB/s` band
4. the updated profile artifact is written back to docs

This task now meets those criteria with the retained implementation:

- `128x64_1m`: `8.9467 MB/s`
- `64x32_1m`: `22.4391 MB/s`
- profile artifact: `8.9748 MB/s`, `encode_chunks = 16`, `fft_stage_calls = 16`, `ifft_stage_calls = 32`

## 9. Risks

### R1. Plan reuse changes nothing

Mitigation:

- profile again immediately
- if no change, the next hotspot is likely deeper in the actual butterfly math rather than schedule reuse

Observed outcome:

- plan reuse helped, but not enough by itself
- the next hotspot was indeed deeper in the actual butterfly path

### R2. Control case regression

Mitigation:

- always pair `128x64_1m` with `64x32_1m`
- do not keep the change if the control case drops materially

Observed outcome:

- the retained `4x` fused butterfly helper improved both the target case and the control case
- later `16-byte` chunking and aarch64 NEON experiments were not retained because they did not improve the target
  point enough relative to the retained scalar best point

## 10. Current Recommendation

Stage-plan reuse was the correct next optimization slice for Leopard GF8.

The task is now complete:

- plan helpers are real execution-time plans
- plans are built once per encode call and reused across chunks
- the retained butterfly implementation is the fused scalar `4x` path
- the experimental `16-byte` chunking and aarch64 NEON fast path were evaluated and rejected

At this point, the next optimization slice should not continue tweaking the same scalar helper structure. Any further
work should target a new higher-leverage bottleneck or a more principled Leopard-specific SIMD design.
