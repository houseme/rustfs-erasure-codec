# Sampling selection review — 2026-10-24

Latest integration: [shared tooling architecture review](tooling-architecture-review-2026-10-24.md).
Implementation/resource scope: [shared borrowed metadata model](#shared-borrowed-metadata-model).
Earlier sections retain their original source identities and validation scope.

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


## Integration with current main

The implementation was rebased onto main
`089339b67027781a746835d39c4c4badd71e8b75`. The verified implementation commit is
`e9a73aba93032440ab317f58f626c521573513dc`. This includes main's selected-survivor cache key and
its matching diagnostic observers. The three implementation files in this patch
are unchanged by the rebase. Relative to current main, codec source, observer
templates, manifests, dependencies, lockfiles and historical evidence still
match exactly.

All applicable gates above pass again on this combined source: 314 native-tool
tests (275 unit / 29 CLI / 10 controller CLI), 555 codec tests and ten doctests,
strict tooling Clippy, codec-only no_std, all four repository audits, formatting
and diff checks. The 43 existing ignored codec tests remain ignored. The JSON
inventory is now 623 files / 6,587,511 bytes because main added its two cache
review records; those original bytes remain unchanged. Package allowlists remain
71/6 members, all five members inherit root dependencies, and no Python source
is present in the audited scope.

The resource and old/new output observations above retain their original
pre-rebase source scope. Rebase does not assign new identities to those probes
or turn them into codec throughput, native x86, Linux capture or release proof.
The following validation commit changes this document only; CI belongs to the
PR head and is reported separately.


## Shared borrowed metadata model

This follow-up reviews PR #101 at
`ddb4c08a81a885ab0531a3f225f66abdcb92f8e6`. Its verified implementation is
`fcf12ca787ebebaae3b09852e363b0c65b1c2c7c`. The preceding typed selection removed JSON
intermediates, but `normalize_ref` still returned a fully owned metadata table
for every original worker row. Validation immediately discarded those copied
schema field names and scalar strings.

`Record` and `Hotspot` now share a metadata projection parameter. One scalar
normalizer, alias resolver and row normalizer serves both projections. Borrowed
validation uses static schema names and `Cow` values tied to the original row;
retained indices and ordinary report outputs choose owned keys/values before
the metadata table is collected. This avoids a borrowed-table-to-owned-table
rebuild. Hotspot exports continue retaining original raw provenance, so they
validate a borrowed projection and discard it before updating the report.
The bridge and persistent summary own individual values only at their output
or retention boundaries. No alternative validator, runtime mode, cache or
compatibility fallback is introduced.

The same release-mode resource probe compares the PR head with the final four
implementation files. The measurement scope and cumulative-allocation definition
are the same as the earlier probe; fixture creation is outside the counter.

| Profile | Allocations before → after | Requested bytes before → after |
| --- | ---: | ---: |
| smoke | 14,185 → 8,965 | 1,284,037 → 1,087,657 |
| small_file | 95,103 → 64,479 | 8,912,593 → 7,759,441 |
| hotspot | 11,680 → 8,896 | 928,511 → 822,527 |
| stream | 17,877 → 12,477 | 1,602,489 → 1,405,029 |
| Total | 138,845 → 94,817 | 12,727,630 → 11,074,654 |

Complete sampling allocation requests decrease a further 31.71%; cumulative
requested bytes decrease 12.99%, relative to the already optimized PR head.
Ordinary smoke/small-file/stream report exports have exactly the same allocation
counts and bytes as that head. The hotspot report's allocation requests decrease
626 → 394 and requested bytes decrease 53,902 → 45,070. These are tool resource
observations, with no elapsed-time, codec-throughput or release-approval claim.

Four complete controller reports and 1,786 additional outcomes are byte-identical
to the old binary. The outcomes include the previous 367 wire/scalar cases plus
1,419 report/index/strict-validation outcomes: four report exports, four retained
indices, 1,408 field mutations across all profiles, and three complete strict
profiles. Extra raw fields, schema scalars, alias conflicts and error precedence
retain their previous behavior. New regression tests additionally verify borrowed
source slices and independent retained metadata after the input is consumed.

All local follow-up gates pass: 316 tool tests (277 unit / 29 CLI / 10 controller
CLI), strict host Clippy, all-target Linux and Windows GNU compilation, the four
repository audits, formatting and diff checks. Windows compilation retains
cfg-specific unused/dead-code warnings in untouched controller/test files;
strict host Clippy has no warnings. Cross-compilation does not imply Windows
execution. Codec source, encoding formats, observer templates, dependencies,
lockfiles and all 623 historical JSON files remain unchanged by the follow-up.

Final source snapshots, old/new executables, raw comparisons and local gate logs
are retained under ignored `target/borrowed-model-review/`; normal sources contain
no resource-probe allocator or measurement tests. CI for the original PR head is
separate from this implementation's local verification. The documentation commit
following the implementation changes this review only.
