# Sampling selection review — 2026-10-24

Reviewed local main `76897f6518821208887cf62aa598c3400265a52e`, initially clean.
The scope follows the latest borrowed-selection integration, native controller
boundary fixes, reduced systematic decoder and GFNI row dispatch. The workspace
uses Rust edition 2024 and declares no MSRV; verification uses stable 1.99.1.

## Architectural decision

The recovery planner already separates validation, immutable matrix planning and
output publication. Its direct one-erasure solve and reduced multi-erasure solve
have distinct work-count benefits. The exhaustive small-survivor comparisons,
large erasure blocks, custom pivot swaps and singularity tests pass. There is no
supported reason from this review to replace those specializations with dense
inversion or introduce another recovery scheduler.

The GFNI dispatch keeps its bounded multi-output kernel and coefficient-streaming
single-output policy. Checked views and dimensions precede kernel writes. This
review finds no concrete defect in those inspected contracts; native x86 GFNI
execution and controlled throughput acceptance are outside this host validation.

Two tooling issues are fixed at their shared source:

- `sampling_contract/batch.rs` now returns a typed `Selection` with borrowed
  preparation to current controllers. Both the native controller and wire
  adapter use the same `select_groups` algorithm after `validate_groups`.
  The native path no longer serializes discarded groups or encodes/decodes its
  index vector. Only final report fields cross a JSON boundary.
- `benchmark_contract.rs` borrows string scalars with `Cow` until their retained
  normalized value is needed. Alias normalization retains the first value and
  validates all following aliases without allocating a temporary value list.
  Only a conflict builds the full alias-name list. Later malformed aliases still
  take precedence over an earlier disagreement, as in the original contract.

No extra compatibility fallback, tuning flag or selection algorithm is added.
Codec source, public APIs, encoding formats, manifests, dependencies, lockfiles
and historical evidence remain unchanged. Scope-specific regression tests cover
Unicode whitespace, unknown values, schema scalars, compiler prefix removal,
complete alias diagnostics and malformed-alias error precedence.

## Resource and compatibility observations

The same temporary release-mode test probe was compiled against the base and
candidate. A thread-local counting `System` allocator measures one complete
controller selection per profile, including result construction; fixture inputs
are built before counting. Common lazy initialization is included in the first
profile of each fresh process. Reallocation counts as an allocation request, and
requested bytes are cumulative requests, not peak live memory or RSS.

| Profile | Original rows | Allocations before → after | Requested bytes before → after |
| --- | ---: | ---: | ---: |
| smoke | 180 | 17,730 → 14,185 | 1,631,226 → 1,284,037 |
| small_file | 1,056 | 115,809 → 95,103 | 10,951,060 → 8,912,593 |
| hotspot | 96 | 13,578 → 11,680 | 1,115,475 → 928,511 |
| stream | 180 | 21,782 → 17,877 | 1,979,925 → 1,602,489 |
| Total | 1,512 | 168,899 → 138,845 | 15,677,686 → 12,727,630 |

Total allocation requests decrease 17.79%; cumulative requested bytes decrease
18.82%. Removing only the internal JSON boundary accounts for less than 1% of
allocation requests; the shared normalization change supplies most of the gain.
These are tooling resource observations, not codec throughput measurements or
release-performance approval. No elapsed-time speedup is claimed.

Four complete controller reports are byte-identical between old/new binaries.
Another 367 serialized outcomes are byte-identical: 12 complete-profile wire
reports covering groups/select/describe, 352 field/scalar combinations, and three
multi-alias agreement/conflict/error-precedence cases. All 12 profile wire
reports succeed; the scalar matrix intentionally includes refusals.

Local probe sources, old/new executables, raw reports, resource counts and gate
logs are retained under the ignored `target/selection-review/` directory. The
probe and its allocator were removed from normal source after measurement.

## Verification

All commands below use `RUSTUP_TOOLCHAIN=stable` and pass:

- `cargo test --locked -p xtask`: 273 unit, 29 CLI and 10 controller CLI tests.
- `cargo clippy --locked -p xtask --all-targets -- -D warnings`.
- `cargo test --locked -p rustfs-erasure-codec --features simd-accel`: 551 tests
  and 10 doctests pass; 43 existing tests remain ignored.
- `cargo check --locked -p rustfs-erasure-codec --no-default-features`.
- `cargo xtask audit-json`: all 621 historical/contract JSON files retain the
  existing inventory of 6,568,679 bytes.
- `cargo xtask audit-package`: codec/WASM allowlists remain 71/6 members.
- `cargo xtask audit-workspace`: all five members inherit root dependencies.
- `cargo xtask audit-no-python`: zero Python files in its checked source scope.
- `cargo fmt --all --check` and `git diff --check`.

This run does not repeat the published-version compatibility harness, formal
Linux capture, x86 native GFNI execution or the 516-worker dynamic audit.
Codec source and observer templates are unchanged. Tool source identities remain
subject to existing provenance binding; historical captures are not relabeled.
Existing release acceptance requirements and historical refusal records continue
to apply.
