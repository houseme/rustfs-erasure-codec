# Task 27: Leopard GF8 Benchmark Decision

## 1. Goal

Decide whether LeopardGF8 encode optimization should continue beyond the current retained baseline by broadening the
benchmark decision surface instead of attempting another local implementation cut.

This task follows Task 26 Route C and is intentionally a benchmark/decision task, not a code-optimization task.

## 2. Why This Task Exists

The current retained LeopardGF8 baseline is real and useful:

- `128x64_1m`: `11.4957 MB/s`
- `64x32_1m`: `32.2955 MB/s`
- `128x64_4m`: `8.5797 MB/s`
- `64x32_4m`: `20.8331 MB/s`
- `128x64_1m` profile: `11.4999 MB/s`

But after that retained point, the first cuts of all nearby follow-up tasks were rejected:

- Task 18: traversal branch specialization
- Task 19: schedule metadata tightening
- Task 20: later-group bookkeeping tightening
- Task 22: later-group accumulation first cut
- Task 23: chunk-size jump
- Task 24: threshold-only change
- Task 25: work-slices-only change

That means the next decision should not be “which nearby patch do we try next”.

It should be:

- whether broader evidence justifies continuing LeopardGF8 optimization at all
- and if so, which topology/fanout region is still worth targeting

## 3. New Direction

This task broadens the benchmark decision surface.

It should answer:

1. is `128x64_*` still the right primary target?
2. do nearby shapes behave similarly enough that one more implementation wave is justified?
3. is the retained baseline already good enough relative to likely future engineering cost?

## 4. Candidate Evidence To Gather

### 4.1 Adjacent fanout shapes

Add one or more shapes between the current retained anchors and the historical problem topology.

Examples:

- `96x48_1m`
- `96x48_4m`

The point is to see whether the current LeopardGF8 behavior changes smoothly with fanout or whether there is another
boundary where the retained design stops scaling well.

### 4.2 Relative comparison surface

Compare retained LeopardGF8 against:

- itself across `1m` and `4m`
- nearby fanout shapes
- optionally the classic path or setup path if that helps a product/repo decision

### 4.3 Decision-oriented questions

The benchmark output should support a decision such as:

- continue LeopardGF8 optimization for high-fanout encode
- hold the current baseline and stop
- or redirect effort to a different family/layer

## 5. Current Code Anchors

- [benches/common/mod.rs](../benches/common/mod.rs:1)
  - `BenchCase`
  - `FULL_CASES`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - retained Leopard encode/profile exporters
- [docs/task-17-leopard-gf8-copy-traffic-reduction.md](../docs/task-17-leopard-gf8-copy-traffic-reduction.md:1)
  - retained implementation baseline
- [docs/task-26-leopard-gf8-route-options.md](../docs/task-26-leopard-gf8-route-options.md:1)
  - route decision leading to this task

## 6. In Scope

- add one or two adjacent LeopardGF8 benchmark shapes
- export the same retained profile/encode artifacts for those shapes if useful
- summarize the decision implications in docs

## 7. Out Of Scope

- changing LeopardGF8 implementation code
- opening another local optimization first cut
- revisiting any rejected Task 18/19/20/22/23/24/25 patch directly in this task

## 8. Execution Plan

### Step 1

Preserve the retained artifacts:

- `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`
- `target/benchmark-smoke/leopard-encode-128x64_4m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_4m.csv`

### Step 2

Add adjacent fanout benchmark cases, ideally:

- `96x48_1m`
- `96x48_4m`

### Step 3

Run the retained decision surface:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_96x48_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_64x32_4m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_96x48_4m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_128x64_4m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 4

Write back a short decision summary:

- continue
- hold
- or redirect

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it expands the benchmark decision surface without reopening local implementation tuning
2. it produces enough adjacent-topology evidence to support a real continuation/stop/redirection call
3. the outcome is written back into docs so later work starts from an explicit decision

## 10. Stable Benchmark Entry

The local `rustup` wrapper may be unreliable in this checkout.

Use:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test <...>
```

as the stable local benchmark entrypoint.

## 11. Current Decision Surface

The widened `64x32 -> 96x48 -> 128x64` benchmark surface is now available for both `1m` and `4m`.

Current measurements:

### 1m

- `64x32_1m`: `30.3994 MB/s`
- `96x48_1m`: `8.7537 MB/s`
- `128x64_1m`: `10.3648 MB/s`

### 4m

- `64x32_4m`: `25.1694 MB/s`
- `96x48_4m`: `8.5032 MB/s`
- `128x64_4m`: `10.7678 MB/s`

## 12. Route C Outcome

Route C has now been validated strongly enough to make a decision.

Interpretation of the expanded benchmark surface:

- the current retained LeopardGF8 implementation is not degrading smoothly with fanout
- the `96x48` region is materially worse than both `64x32` and `128x64`
- that means the current retained baseline is not yet topology-stable enough to justify more local encode tuning as the
  default next move

Decision:

- hold the current retained LeopardGF8 implementation as an experimental baseline
- do not immediately continue local encode-path optimization
- redirect future work toward explaining the `96x48` collapse or toward a higher-level product/repo decision about
  whether continued LeopardGF8 investment is worthwhile

## 13. Current Recommendation

After validating A/B/C in sequence, the current recommendation is:

1. keep Route A's richer profile and benchmark surface
2. do not resume local LeopardGF8 encode patching yet
3. treat the next step as either:
   - a focused investigation into why the `96x48` region collapses
   - or a broader product/repo decision about whether LeopardGF8 should continue beyond the current retained baseline
