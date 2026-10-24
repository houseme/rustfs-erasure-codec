# Combined coding metadata compatibility rehearsal

Independent review source [`6193ad3`](https://github.com/houseme/rustfs-erasure-codec/commit/6193ad329f5b53283c859110dfb967f686c3a26e)
combines encoding PR #107 head `0dd9434` and recovery PR #106 head `4fdb83a`
from common main `e758f11`. The source is preserved on the review branch
`houseme/coding-integration-proof-9.1.0`. Both PRs retain their independent code
branches and review scope; this rehearsal performs no GitHub PR merge.
The [complete result](../benchmarks/release-9.1.0/2026-10-25/combined-coding-local-review.json)
retains every one of the 459 resource checks and the exact source boundaries.

## Composition and behavior

The combined source carries the parent implementations unchanged: ordinary
recovery borrows validated missing indices, mixed recovery merges completed data
references backwards into survivor storage, and coding consumes a borrowed parity
matrix range or existing decode-row slices through static access. Matrix/cache
selection, field arithmetic, native eligibility, policy decisions, format identity,
selective absence and generic late-error publication keep their parent contracts.

All twelve changed source/test/workflow blobs match their designated parent
exactly. There are no production-source conflicts or extra runtime patches.
Changelog and parent evidence are both retained. The six merge conflicts concern
only those records and snapshot assertions. Neither parent's dynamic digest is
selected blindly: all 9/4/3 generated observer members are compared in full with
main. Changes to reconstruct/policy/matrix and stream exactly match their complete
production deltas. The combined digest is recomputed, and the freshly rebuilt
combined tool repeats the entire generated report. Tool changes remain limited
to current-source snapshot assertions; formal timing loops and rules are intact.

## Complete combined-source validation

Fresh stable local validation passes 401 library, 167 integration and ten doc
parent tests (578), six separately counted filtered child executions and 43
existing ignored tests. Codec-only no_std passes 271 tests; Rust tooling passes
317. Strict host codec/tool Clippy, formatting, spelling and diff checks pass.
x86 Darwin all-target and x86 Linux library Clippy remain compile-only evidence.
All tested source, test and tool input blobs match `6193ad3`.

All parent resource contracts survive the combination:

| Catalogue | Cases | Observed combined result |
| --- | ---: | --- |
| Preallocated coding / buffered verify / incremental cycles | 270 | Zero release allocation requests; unchanged from the encoding parent |
| Ordinary missing-target reuse | 54 | Four requests per generic recovery; unchanged from the recovery parent |
| Mixed data/parity view reuse | 135 | One wide view-table request; each total matches its recovery-parent result |

These checks retain caller-owned buffers, backend/inverse warmup and their
original fixture scopes. Small-buffer parallel APIs follow the existing serial
policy. This is deterministic allocation accounting, not throughput or parallel
allocation acceptance.

Published 9.0.0 local-default-backend checks pass exact encoding bytes and
bidirectional recovery: 200 non-fast normal configurations, 27 custom / 162 cross
recoveries, 12 wide GF16 and 270 selective / 2,640 cross recoveries. Fresh release
WASM generation/types and 14 Node cases pass: 12 actual-module cases, one
result-code contract and one facade unit, zero skipped.

## Actual crate verification

An offline core package is created with builtin verification disabled, followed
by separate extracted-crate checks. Its 71 members include 68 original source
members that match `6193ad3` byte-for-byte, with clean source-bound VCS metadata.
There are 64 Rust files under src plus build.rs (65 Rust members in total); the
three generated Cargo metadata members are distinguished from originals.
The extracted library passes 401 all-feature tests (five existing ignored) and
271 no_std tests. The WASM source-package inventory remains six. No registry
upload or tag is created; no new NPM-consumer or browser claim is made here.

Independent replay checks 31 unchanged original inputs, every resource record,
raw summary, parent composition and all original crate members without running
codec work. It preserves the same complete generated reports and parent contract
values; no favorable case subset is selected.

## Release boundary

This is a source-bound local integration rehearsal with zero controlled timing
observations. The review branch has no combined-source native Windows/Linux or
browser CI claim. Parent CI, native and timing data retain their own sources.
Actual main integration must be checked against this source; these records are
not relabeled as a later main commit. Final actual source/tool/lock freezing,
accepted complete published-9.0.0 ABBA/paired-layout evidence and strict positive
release preflight remain outstanding. Performance and release approval remain
withheld, with no automatic PR merge.
