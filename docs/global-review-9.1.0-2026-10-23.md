# 9.1.0 global review — 2026-10-23

This review follows the merge of `origin/main` at
`d88e69a895d059af1aa4179d39de91decaa20b49` into the current tooling branch
(merge `48dbf12`). Main contains the same files as diagnostic commit `022c0a7`,
under a different commit identity. The merge retains the native Rust tooling
migration; it does not restore the retired Python validation implementations.
The concurrently developed stream-view migration subsequently completed as
`ff5a31a` and is included in the reviewed branch. Its remaining documentation
edits stay outside this review's commits.

## Architectural decision

Keep the current boundaries rather than introduce another collector, retry loop,
allocator switch or codec tuning policy:

- The codec owns encoding formats, recovery plans, scratch reuse and validation
  before publication. Its production Rust sources are unchanged from `022c0a7`
  in the reviewed branch. Retain sparse/dense update cases and owned/in-place
  caller lifetimes when evaluating its existing resource improvements.
- Repository-only Rust tools own the shared API/mask catalog, native validation
  and reversible source rendering. The persistent framed helper and complete
  worker batches avoid repeating process startup and per-record IPC. Golden
  tests protect the original generated sampling code and catalog semantics.
- Capture controllers own processes, generated files and retained evidence.
  An observation or statistical failure is data in a complete diagnostic;
  execution and ownership errors remain explicit failures to investigate.
- Release acceptance belongs to the complete, source-bound evidence and strict
  preflight. Tooling correctness and resource-request reductions do not substitute
  for accepted latency or throughput measurements.

The migration is a consolidation around common contracts. The concrete lifecycle
defect below belongs in its shared process owner, not in individual collectors.
No new production fast path is justified by an unfinished diagnostic campaign.

## Source-ledger consolidation

The subsequent review moves ordinary verify scratch and stream-core ownership
validation into the same unpublished Rust tool (`0350381`, Python bindings in
`eed9867`). This removes two duplicate Python schedule/shape models. The stream
validator consumes the expected sequence directly, keeps only actual owners in
live scopes, and releases scope state on exit. It allocates neither a second
complete event schedule nor a second map of expected owner shapes. All three
source/view ledgers share nullable-field, offset and residency rules.

Published 9.0.0 and retained-buffer layouts remain explicit alternatives. The
former preserves nested per-block `verify_par`, transferred Option owners and
replacement boundaries. Former-range queries must match the last actual owner;
an unmapped former range cannot certify a live allocation. Empty source storage
still requires an explicit empty snapshot, while view snapshots retain their
existing nonnative unsupported representation. V1/V3 schemas and summaries stay
unchanged. Typed unsigned fields additionally reject negative first timestamps
and values beyond u64 that the old Python model accidentally accepted.

The inventory is now 34 Python files / 8,564 lines, down 225 lines in this step
and about 35% from the original 13,120 lines. Remaining files have active capture,
rollback, replay or test responsibilities. No additional file is classified as
obsolete merely to reduce the count. The 576 JSON files retain their original
bytes and provenance; duplicate historical evidence is not rewritten. Core/WASM
package allowlists still contain 67/6 members and exclude repository-only tools,
Python, evidence and contracts. All five workspace
members inherit dependency declarations from the root Cargo.toml.

The new native tests cover source-version distinctions, required nullable fields,
counter availability, unchanged original verify results, former owners and page
arithmetic at u64 limits. These are tooling resource/structure improvements;
they do not establish a new codec throughput or latency result.

The finalized implementation passes 105 native tests (80 unit, 25 executable),
strict Clippy and Linux all-target compilation. Its original/native comparison
covers 3,941 requests: 3,937 identical decisions/summaries plus four explicit
refusals of the malformed timestamps above. The existing 1,305 stream-view
comparisons also agree after the shared-rule extraction. The two source copies
complete 28 ordinary verify and 30 stream workers, capture and replay, totaling
696 observations / 98,448 events. All 58 complete ledger summaries agree with
the original Python validators. A prior run is retained as failed because source
edits during capture changed the implementation fingerprint; the finalized run
uses the frozen implementation throughout.

The 200-configuration published/local compatibility probe again passes exact
parity, cross-decoding in both directions and the retained field vectors. An
offline Cargo package again contains 67 files / 192.0 KiB compressed and rebuilds
its extracted sources. Package, JSON and workspace audits pass without uploads.

## Dynamic allocation ledger consolidation

The next functional commits are `da103b6` (artifact-sized native transport) and
`2bcc0de` (dynamic ledger migration). The shared Rust tool now owns dynamic
allocation generations, payload schedules, decoder supplement headers and the
private workload-marker adapter. The six supplement definitions also have one
native catalog; the Python controller consumes its values.

The validator prepares each payload schedule once for all 24 iterations. It
tracks live allocation generations, retires previous generations on realloc/free,
and borrows each iteration's original event slice. It does not retain the old
all-generations map or 24 additional iteration lists. Per-thread counters use
bounded thread slots. Page/offset rules reuse the shared diagnostic model,
including explicit nonnative snapshots and the bounded 4,097-page query policy.

Old/current Leopard temporary-owner layouts remain distinct. Leopard GF16's
public shards and matrices contain bytes, while its internal lanes contain
16-bit elements. That distinction is covered by native tests and actual decoder
ledgers. Diagnostic workload markers remain in retained journals; only a private
validation copy is unwrapped, and formal sampling continues to reject that marker.
No production codec kernel, matrix, field representation or allocator changes.

A compatibility check exposed a transport boundary: the old Python validator
accepts a valid 50,625-event ledger whose framed request occupied 23,882,805 bytes,
exceeding the helper's former 16 MiB cap. Requests now allow a complete 32 MiB
artifact plus 64 KiB of envelope space. Both Python and Rust enforce the bound;
responses remain limited to 32 MiB. Compact JSON reduces that same request to
20,997,245 bytes and preserves its complete summary. A regression checks that a
large request also preserves the next worker frame.

The Python inventory is 34 files / 8,236 lines: another 328 lines removed and
about 37% fewer lines than the original inventory. Six pure dynamic tests move
with their model; capture, source restoration, original-journal binding and
replay integrations remain active. The 576 JSON files retain their original
bytes. Core/WASM package allowlists still contain 67/6 members, and all five
workspace members inherit the root dependency catalog.

Validation of the frozen `2bcc0de` source passes 118 native tests (93 unit,
25 executable), strict Clippy and Linux all-target compilation. The original
Python model agrees on 1,583 ledger decisions/summaries and 72 supplement-header
checks. Historical development inputs missing the required shape field remain
refused by both implementations; their stored bytes are unchanged.

Both source versions complete the entire 516-worker capture and independent
replay: 126 original cases in calibration and sample modes, plus all six decoder
supplements. That covers 6,192 observations / 12,384 iteration scopes. A further
original/native comparison checks every one of those 516 complete summaries,
including 1,491,044 events and all twelve actual supplement headers, with no
mismatches. Both native observer semantics checks also pass. These runs are
instrumented functional diagnostics on macOS ARM64; they do not constitute
Linux resource qualification or accepted codec performance measurements.

The compatibility runner again passes 200 exact parity configurations and
cross-decoding in both directions. Core all-feature tests pass 517 cases with
43 existing ignored tests; all ten doctests, 256 Python tests and the independent
no_std check pass. Offline packaging again rebuilds the 67-file, 191.9 KiB
extracted crate.
No tag, push or crate upload is performed by this migration.

## Verified defects and fixes

### Background worker surviving its leader

`xtask/src/diagnostics/owned.rs` originally used `Child::try_wait` to reap the
direct child. A shell wrapper could exit successfully or fail while a background
worker continued running. Cancellation also returned as soon as a terminated
leader was reaped, leaving a worker that ignored SIGTERM alive. The caller could
then remove generated inputs or start its next check with that worker still
consuming resources. This is a controller defect; it is not evidence about the
cause of any historical codec timing drift.

A bounded regression first failed on the old owner: a background worker published
an output after its leader exited with status zero. The shared owner now observes
exit without reaping, stops the original owned group and only then reaps the
leader. The unreaped child prevents its PID/group identity from being reused.
Both successful and failed exits, plus cancellation with a SIGTERM-ignoring
worker, retain their original status and leave no delayed output.

The implementation uses the documented
[`waitid` / `WNOWAIT` contract](https://man7.org/linux/man-pages/man2/waitpid.2.html).
Darwin can return EPERM for a group containing only a zombie; its
[group-signalling implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)
excludes zombies. The exception is accepted only after a native membership query
proves that the sole member is the owned leader. Other permission failures remain
errors and preserve generated inputs. This owner supervises the group it created;
it does not claim to supervise arbitrary detached sessions or Windows job trees.

### Local no_std check accidentally enabled std

`scripts/release-check.sh` ran `cargo test --no-default-features` without selecting
the codec package. The workspace's default members include WASM and fuzz, both
depending on the codec with default features. Cargo therefore unified `std` back
into the supposed no_std run. The previous command actually ran 341 passing core
tests with std enabled, plus the WASM library tests; it was not no_std evidence.

The local release command and release checklist now select
`--package rustfs-erasure-codec`. The focused shell regression failed before the
fix and passed afterward. An actual package-scoped feature-tree check confirms
that defaults are no longer reintroduced. Existing CI no_std jobs already select
the package and need no equivalent change.

## Validation and release state

The review uses stable Rust 1.99.1 on native macOS ARM64. Initial validation of the
working tree included the concurrent stream-view migration; that result must not
be presented as validation of an earlier commit excluding it. The final reviewed
source includes its separate `ff5a31a` commit. The focused process-owner and
release-script fixes are also checked from committed source snapshots.

The current-tree full-feature workspace run passes 622 tests with 43 ignored;
Clippy with warnings denied passes. An independent archive of `1f9b0c2`, excluding
the uncommitted migration, passes all 61 native-tool unit tests and 23 CLI tests.
The actual package-scoped no_std+SIMD run passes 237 library tests. All eight
release-script control-flow tests pass after fixing the package selection.
The real compatibility runner passes 200 normal-format configurations in both
decoding directions and all 45 resource scenarios, retaining 270 allocation
observations. Those request counts do not establish latency or throughput.

Final independent validation of committed source `6f88b5c` (including `ff5a31a`)
passes 617 default-workspace tests and 622 full-feature tests, each with 43 ignored,
plus all-feature Clippy, Rust formatting and the eight release-script regressions.
The complete local Python controller suite passes 269 tests. A clean archive of
the committed source also passes actual `cargo publish --dry-run --locked
--registry crates-io --package rustfs-erasure-codec`: Cargo packages and rebuilds
the distributable sources, then explicitly aborts upload. No `--allow-dirty` or
`--no-verify` was used. An initial offline attempt required registry metadata and
failed before packaging; enabling the metadata read completed the same dry-run.
Generated WASM browser checks and Linux/macOS CI still need their new PR results;
historical CI successes are not attributed to this branch.

Keep the following checks distinct in the delivery record:

- Full-feature workspace tests and Clippy with warnings denied.
- Real package-scoped no_std+SIMD library tests.
- Published 9.0.0/candidate normal-format parity and cross-decoding in both
  directions, historical field vectors and all 45 resource scenarios.
- Native-tool tests, legacy-controller tests, formatting and source-bound package
  preparation. Package preparation never uploads a crate.

The full Linux diagnostic has completed both calibrations and all four original
126-case phases. Paired layout collection is still running; its full report and
independent replay are pending. Frozen remote sources, counts, guard and plan
remain unchanged. All statistical failures must be retained and later compared
with parent-worker timelines and host-condition observations before attributing
a cause or selecting a performance change.

The local branch also upgrades `parking_lot` and changes the lockfile. Even with
unchanged production Rust sources, the running frozen campaign is not evidence
for those new dependency bytes. Finish the existing campaign without modifying
it, then bind subsequent acceptance to the final reviewed candidate.

Version declarations already say 9.1.0. Keep the changelog unreleased. Final
compatibility/package checks, complete accepted performance evidence and strict
positive preflight remain necessary before a separately authorized tag/upload.
See the [release checklist](release-9.1.0-checklist.md) and preserve the historical
[blocker assessment](release-9.1.0-blockers.md) with its original source bindings.

## Follow-up on merged main: optimized native helper

PR #51 was merged as main `69379b3`. Merging that main again produced the same
reviewed file tree. The subsequent review identified a measurable build-policy
cost: the long-lived native helper used Cargo's unoptimized developer profile
for every repeated validation request.

A local diagnostic freezes source `8f214c5` and builds both native profiles with
the same compiler, lockfile and defaults. Each request validates three synthetic
`encode_stream:4:2:65536:16384` ledgers, two calls per observation, complete source
positions/counters and window summaries. The request is 187,523 bytes. Four new
workers run dev/release/release/dev; each keeps three warmup and nine sample
blocks, with 40 requests per block. All 48 blocks and 1,920 complete, equal-result
replies are retained in the
[source-bound diagnostic](../benchmarks/tooling/2026-10-23/native-helper-profiles.json).

| Profile | First median per request | Second median per request | Repeat change |
| --- | ---: | ---: | ---: |
| Developer | 7.2773 ms | 7.2367 ms | -0.5586% |
| Release | 0.8195 ms | 0.8208 ms | +0.1624% |

These are parent wall times for framed IPC, native validation, reply decoding and
complete-result comparison, excluding build/startup. The host is not isolated
and the workload is synthetic. The values describe this tool request and do not
establish codec throughput, explain Linux drift or approve a release.

The common Python binding now builds the host tool with `--release`, retaining
`--locked --offline`, native target selection, Cargo artifact discovery and the
existing cache. This is a single shared build-policy fix. Persistent workers,
batch boundaries, validators, original sampling code and the interactive Cargo
alias are unchanged. A build-policy regression first fails on the former command;
real controller tests and optimized native-tool tests verify behavior afterward.
The already-running Linux campaign keeps its frozen tool and plan.

The measurement's source tree is also bound to public main `69379b3` in the
report's `equivalent_main_commit` field; both trees are identical. In a separate
worktree based on that main, stable Rust 1.99.1 passes all 69 optimized native
unit tests and 24 CLI tests, followed by the complete 270-test Python controller
suite. The new build-policy regression fails before adding `--release` and passes
afterward. Rust sources, codec manifests/lockfile and generated sampling workers
retain the previously tested implementation; this follow-up changes the shared
helper's build command, its regression, documentation and retained observations.
