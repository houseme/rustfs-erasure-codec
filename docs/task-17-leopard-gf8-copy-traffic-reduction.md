# Task 17: Leopard GF8 Copy-Traffic Reduction

## 1. Goal

Reduce the remaining `128x64_1m` scaling gap in the current Leopard GF8 encode path by targeting bulk data movement
in the post-plan-reuse, post-fused-butterfly implementation.

This task starts from the conclusion that the retained scalar `4x` butterfly path is now the best local math kernel,
and that the next likely bottleneck is memory traffic rather than more butterfly micro-tuning.

## 2. Why This Task Exists

Task 16 completed the two most important slices that were still obviously unfinished:

- `build_ifft_dit8_plan(...)` / `build_fft_dit8_plan(...)` are now real execution-time plans
- the retained full-window butterfly implementation is the fused scalar `4x` path

That work materially improved the target point:

- `128x64_1m`: `8.9467 MB/s`
- `64x32_1m`: `22.4391 MB/s`

and reduced the stage-repeat profile to:

- `encode_calls = 2`
- `encode_chunks = 16`
- `encode_full_groups = 32`
- `encode_later_group_calls = 16`
- `fft_stage_calls = 16`
- `ifft_stage_calls = 32`

At this point, the remaining gap is no longer best explained by stage scheduling or by the local 4-lane butterfly math.
The strongest remaining suspect is bulk lane copy / zero / xor / parity writeback traffic across every chunk.

## 3. Current Situation

Current retained implementation anchors in [src/core/leopard_gf8/](../src/core/leopard_gf8/mod.rs:1):

- [encode_with_tables(...)](../src/core/leopard_gf8/encode.rs:112)
- [ifft_dit_encoder8_with_plan(...)](../src/core/leopard_gf8/encode.rs:698)
- [fft_dit8_with_plan(...)](../src/core/leopard_gf8/encode.rs:666)
- [fft_dit4_full_lut_scratch(...)](../src/core/leopard_gf8/ops.rs:502)
- [ifft_dit4_full_lut_scratch(...)](../src/core/leopard_gf8/ops.rs:540)

The current code still performs several large memory-moving operations per chunk:

- input shard materialization into work lanes via `copy_from_slice(...)`
- zero padding for missing lanes via `copy_from_slice(&zero[..size])` / `fill(0)`
- xor accumulation from temp work into destination lanes via `slice_xor(...)`
- final parity writeback via `output.as_mut()[offset..end].copy_from_slice(...)`

These operations now stand out more clearly because stage counts have already been reduced.

## 4. Core Hypothesis

The next meaningful gain will come from reducing bytes moved, not from further rewriting the retained scalar `4x`
butterfly helper.

If we can reduce one or more of the following:

- redundant lane materialization copies
- zero-fill work for known-empty lanes
- separate xor accumulation passes over full lane slices
- final parity copy-out from work into output shards

then:

- `128x64_1m` should improve further from the current `~8.95 MB/s` band
- `64x32_1m` should remain at least flat or improve modestly

## 5. In-Scope Targets

### 5.1 Primary targets

- reduce input-to-work copy traffic in `ifft_dit_encoder8_with_plan(...)`
- reduce zero-lane initialization traffic for partial groups
- reduce xor accumulation passes over `xor_dst`
- inspect whether final parity copy-out can be narrowed, deferred, or partially avoided

### 5.2 Explicitly out of scope

- further micro-tuning of `fft_dit4_full_lut(...)` / `ifft_dit4_full_lut(...)`
- reviving the rejected `16-byte` chunking variant from Task 16
- reviving the rejected aarch64 NEON fast path from Task 16
- verify/reconstruct Leopard work

## 6. Evidence From Current Code

Current data-movement hotspots visible directly in code:

- [src/core/leopard_gf8/encode.rs:270](../src/core/leopard_gf8/encode.rs:270)
  - final parity writeback copies `work[idx][..size]` into every output shard slice
- [src/core/leopard_gf8/encode.rs:717](../src/core/leopard_gf8/encode.rs:717)
  - initial shard lane materialization into `work`
- [src/core/leopard_gf8/encode.rs:730](../src/core/leopard_gf8/encode.rs:730)
  - partial-group materialization via additional `copy_from_slice(...)`
- [src/core/leopard_gf8/encode.rs:739](../src/core/leopard_gf8/encode.rs:739)
  - lane clearing with `fill(0)`
- [src/core/leopard_gf8/encode.rs:794](../src/core/leopard_gf8/encode.rs:794)
  - xor accumulation over every lane via `slice_xor(...)`

These are now stronger suspects than the butterfly helper itself because Task 16 already retained the best local
butterfly implementation and rejected more elaborate math-kernel variants.

## 7. Recommended Task Slice

This task should treat Leopard GF8 as a memory-traffic problem first.

Recommended order:

1. instrument or reason precisely about bytes moved per chunk/group
2. target one traffic source at a time
3. keep the retained scalar `4x` butterfly unchanged unless a change is required to support the traffic reduction

## 8. Execution Plan

### Step 1

Preserve the current accepted baseline artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`

Accepted baseline values for this task:

- `128x64_1m`: `8.9467 MB/s`
- `64x32_1m`: `22.4391 MB/s`
- profile: `8.9748 MB/s`

### Step 2

Add narrowly scoped profiling or counters if needed to attribute copy / zero / xor / writeback volume.

### Step 3

Prototype the highest-confidence traffic reduction first. Candidate order:

1. avoid unnecessary zero-copy work for known-empty lanes
2. reduce full-slice xor passes into `xor_dst`
3. narrow or restructure final parity copy-out
4. reduce repeated input materialization into temp work

### Step 4

Re-run:

```bash
cargo test benchmark_leopard_encode_profile_128x64_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_128x64_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_64x32_1m_exports_results -- --nocapture
```

### Step 5

Keep only changes that beat the retained Task 16 baseline on the target point without materially hurting the control
case.

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it attacks a real post-Task-16 hotspot rather than reworking the same butterfly helper again
2. `128x64_1m` improves meaningfully from the current `~8.95 MB/s` band
3. `64x32_1m` does not regress materially from the current `~22.44 MB/s` band
4. the retained change is clearly attributable to lower copy/zero/xor/writeback traffic, not just benchmark noise

## 10. Risks

### R1. Copy-traffic work changes little

Mitigation:

- treat that as evidence that the next bottleneck is elsewhere
- do not force increasingly invasive copy-elision rewrites without measurements

### R2. Output correctness becomes subtle

Mitigation:

- change one traffic source at a time
- keep parity writeback behavior explicit until a better ownership model is proven

### R3. Reopening rejected Task 16 directions by accident

Mitigation:

- do not reintroduce `16-byte` chunking of the scalar butterfly
- do not reintroduce the rejected NEON fused path unless a new field-correct design appears

## 11. Current Recommendation

Proceed with a dedicated copy-traffic reduction slice as the next Leopard GF8 task.

Task 16 already proved that plan reuse plus the retained scalar `4x` butterfly was the right local endpoint. The next
serious opportunity is now the amount of data copied, zeroed, xor-folded, and written back around that math kernel.

## 12. Current Checkpoint

The first two traffic-focused cuts now have a clear outcome:

### Accepted

- remove the dedicated zero-buffer read path and zero missing lanes directly with `fill(0)`
- reduce full-slice xor loop overhead by batching `slice_xor(...)` in fixed-width chunks

These changes improved the current retained measurements to:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_1m` profile artifact: `11.4999 MB/s`

The profile counters remained:

- `encode_calls = 2`
- `encode_chunks = 16`
- `encode_full_groups = 32`
- `encode_later_group_calls = 16`
- `fft_stage_calls = 16`
- `ifft_stage_calls = 32`

This is important because it shows the gain came from lower traffic around the same retained stage/butterfly shape,
not from changing the stage schedule again.

The latest retained profile also exposed the current byte-traffic split for `128x64_1m`:

- `input_copy_bytes = 268435456`
- `xor_bytes = 134217728`
- `output_writeback_bytes = 134217728`
- `zero_fill_bytes = 0`

This matters because it identifies the next most likely bottleneck with much higher confidence:

- the zero path is no longer the main problem
- the largest remaining traffic source is now input materialization into work lanes
- xor accumulation and final output writeback are the next tier

### Rejected

- directly aliasing the parity output slices as the first half of the work buffer to avoid final parity copy-back
- directly materializing the first `dist == 1` IFFT stage from input shards into work lanes to force
  `input_copy_bytes` toward zero
- changing only the full `available == 4` materialization copy shape for `initial_blocks` while keeping stage
  calculation unchanged
- replacing final parity `copy_from_slice(...)` writeback with a custom batched copy helper
- replacing the retained byte-unrolled `slice_xor(...)` path with a `u64` word-wise XOR path
- replacing the current `with_lane_views(...)` iterator/collect path with a manual `SmallVec` push build path

That prototype regressed both the target and the control case and should not be revived without new evidence.

## 13. Next Recommendation

Continue within Task 17, but stay disciplined:

1. keep the retained zero-fill and batched `slice_xor(...)` improvements
2. do not retry parity-output aliasing
3. if another traffic cut is attempted, prefer a narrowly scoped `xor_dst` or materialization-path refinement over a
   broad ownership rewrite

Refined priority from the current retained profile:

1. target `input_copy_bytes` next
2. only after that, revisit `xor_bytes` / `output_writeback_bytes`

Latest rejected follow-up evidence:

- the direct stage-1 materialization prototype drove `input_copy_bytes` to zero in the profile artifact but still
  regressed throughput, which means that simply removing the copy is not sufficient if it worsens the downstream
  execution shape
- even the more conservative full-4-lane copy-shape-only variant still regressed, which suggests the remaining
  `input_copy_bytes` problem is not solved by a narrower memcpy-style rewrite alone
- the custom batched parity writeback helper also regressed, so output-copy replacement is not currently the right
  next slice
- the `u64` word-wise `slice_xor(...)` replacement also regressed relative to the retained unrolled byte path
- the manual `with_lane_views(...)` build path also regressed, so lane-view construction should not be treated as the
  next leading hotspot without stronger evidence

Current implication:

- `input_copy_bytes` is still the largest remaining traffic bucket
- but the next viable attempt must preserve the retained execution shape better than the rejected direct-stage rewrite
- `xor_bytes` remains a productive path only when the retained byte-unrolled implementation is kept
- `with_lane_views(...)` is not currently a high-confidence next cut

## 14. Execution Notes

During this task family, the local `rustup` wrapper may report that `cargo` is not applicable to the active stable
toolchain even though the stable toolchain binaries themselves are present and usable.

For reproducible local validation, prefer invoking the real stable binaries via:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test <...>
```

Treat that as the canonical local benchmark entrypoint until the wrapper issue is resolved.
