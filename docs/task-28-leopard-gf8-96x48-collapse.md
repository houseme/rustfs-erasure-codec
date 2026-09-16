# Task 28: Leopard GF8 96x48 Collapse

## 1. Goal

Explain why the current retained LeopardGF8 encode implementation collapses at `96x48` relative to the neighboring
`64x32` and `128x64` shapes.

This task is diagnostic first. It should not begin with another local optimization patch.

## 2. Why This Task Exists

Task 27 broadened the LeopardGF8 decision surface and found a non-smooth topology response:

### 1m

- `64x32_1m`: `30.3994 MB/s`
- `96x48_1m`: `8.7537 MB/s`
- `128x64_1m`: `10.3648 MB/s`

### 4m

- `64x32_4m`: `25.1694 MB/s`
- `96x48_4m`: `8.5032 MB/s`
- `128x64_4m`: `10.7678 MB/s`

This matters because it means the current retained implementation is not just “slower at larger fanout”.

Instead:

- `96x48` is a distinct collapse region
- both the lower neighbor (`64x32`) and the higher neighbor (`128x64`) are materially better

So the next credible step is to explain `96x48`, not to keep patching LeopardGF8 encode blindly.

## 3. New Diagnostic Cut

This task isolates `96x48` specifically.

The first question is:

- what does `96x48` look like under the same profile schema already used for `128x64_1m`?

## 4. Current Code Anchors

- [benches/common/mod.rs](../benches/common/mod.rs:1)
  - `FULL_CASES`
- [tests/benchmark_smoke.rs](../tests/benchmark_smoke.rs:1)
  - retained Leopard encode/profile exporters
- [src/core/leopard_gf8/mod.rs](../src/core/leopard_gf8/mod.rs:1)
  - current retained LeopardGF8 profile counters
- [src/core/leopard_gf8/driver.rs](../src/core/leopard_gf8/driver.rs:1)
  - `m`, `mtrunc`, `last_count`, `chunk_size`, `work_slices`

## 5. Core Hypothesis

One of these is likely true:

1. `96x48` activates a less favorable retained shape for `m`, `mtrunc`, `last_count`, or group scheduling
2. `96x48` has a worse retained chunk/work interaction than both `64x32` and `128x64`
3. `96x48` is exposing a plan/topology interaction that the current retained `128x64`-oriented reasoning missed

## 6. In Scope

- add `96x48_1m` Leopard profile export with the same schema used for `128x64_1m`
- compare `96x48_1m` profile numbers directly against `128x64_1m`
- optionally add `96x48_4m` profile export only if `1m` leaves the picture ambiguous

## 7. Out Of Scope

- immediate implementation optimization patches
- helper/kernel rewrites
- traversal/metadata redesign
- chunk/work design changes

## 8. Execution Plan

### Step 1

Add:

- `benchmark_leopard_encode_profile_96x48_1m_exports_results`

using the same exporter used for `128x64_1m`.

### Step 2

Run:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_profile_96x48_1m_exports_results --test benchmark_smoke -- --nocapture
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test benchmark_leopard_encode_96x48_1m_exports_results --test benchmark_smoke -- --nocapture
```

### Step 3

Compare against:

- `64x32_1m`
- `128x64_1m`
- `128x64_1m` profile artifact

### Step 4

Only after the `96x48_1m` profile is understood should a new implementation task be opened.

## 9. Acceptance Criteria

This task should be considered successful only if:

1. it produces a same-schema profile artifact for `96x48_1m`
2. it gives a concrete explanation of how `96x48` differs from the retained neighboring shapes
3. it narrows the next implementation cut to something more specific than “keep optimizing LeopardGF8”

## 10. Stable Benchmark Entry

The local `rustup` wrapper may be unreliable in this checkout.

Use:

```bash
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" test <...>
```

as the stable local benchmark entrypoint.
