# 9.1.0 global review — 2026-10-23

Latest scope: complete native controllers and Python retirement; see the
[current native review](#complete-native-controllers-and-python-retirement).
Earlier sections preserve their original identities, counts and validation scope.
The canonical release readiness review retains the independent performance
acceptance boundary.

## Integration with current main

The tooling branch is rebased onto main
`e3aca327264b2956f14690456b06e060aaad53b8`. The independently validated code
snapshot is `1de54689a275c7e6f976fcb4b8b27f3bf47d4875`; the final documentation
commit changes only this review and the inventory in `xtask/README.md`.
The 26 rebased functional/review commits retain their boundaries. Production
`src/`, build.rs, all Cargo manifests and the lockfile match this main exactly,
including its shared Classic verification and automatic GFNI priority fixes.
The review found no additional supported runtime defect requiring another codec
path or tuning policy. The observer passthrough, source generators and new native
analysis models pass together on the combined source.

Python now totals 34 files / 7,702 lines (3,594 production/helper and 4,108
test/fixture lines), compared with main's 8,495 lines and the original 13,120.
The remaining active ownership, platform and frozen-replay adapters stay explicit.
All 584 JSON files / 3,006,767 bytes match main; eleven identical-byte groups
retain their evidence paths. All five workspace members inherit root dependencies.
The core/WASM distributable allowlists contain 67/6 members. The actual core
archive rebuilds from its extracted sources and compresses to 192.7 KiB;
repository-only Python, tools, tests, JSON evidence and contracts are excluded.

Validation on this rebased snapshot passes 194 native-tool tests, 247 Python
tests, 356 codec unit tests, 160 top-level integration tests, ten doctests, strict
tool Clippy, the Linux tooling cross-check, codec-only no_std, all three audits,
actual Cargo package verification, formatting, spelling and diff checks. The
existing 43 ignored codec tests remain ignored. The published-9.0.0/local-9.1.0
probe passes 200 non-fast configurations with exact parity and cross-decoding in
both directions, plus historical tower-field vectors. The probe deliberately
does not certify the older release's inconsistent fast-one-parity mode.

The dated sections below retain the original source identities and validation
scope of each migration. Rebase does not relabel their historical measurements.
Local functional and package checks do not approve codec throughput, Linux
isolation, Windows execution or strict release publication. The existing release
acceptance records and retained failures still apply.

An independent follow-through at local merge `709c40b` tests the same
implementation as published review head `c176ac2`: their complete Git file
difference contains only `CHANGELOG.md`, this review and `xtask/README.md`.
All codec/tool sources, tests, workflows, manifests and existing data match.
The additional complete comparison report retains the executed source identity;
this correspondence does not relabel frozen Linux measurements. See the final
integration section for the independent checks and request witnesses.

## Earlier migration review

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

### Independent dynamic-ledger scope review

A further review froze `e07e82c`, with `origin/main` at `69379b3` already in its
history. It found one migration regression: the borrowed iteration slice includes
`call=0` events, whereas the original validator collected only events bound to
the open call. An unscoped `decode_ready` matrix view could therefore supply a
missing iteration witness. The equivalent Leopard `chunk_coded` check had the
same omission. A complete 24-call / 1,177-event reconstruction reproducer is
accepted by both implementations before mutation; changing its matrix views to
`call=0` is refused by the original model but was accepted by the migration.

Require a positive, already-validated call for both decoder-witness checks.
`Ledger::validate` still enforces the exact open call and observation for every
positive-call event. This restores the existing scope contract without copying
event lists, retaining closed generations, or adding another validation pass.
Keep unscoped setup/allocation observations in the original ledger and summary.

Both new native regressions fail before the fix and pass afterward. The complete
fixed native suite passes 95 unit and 25 executable tests, and strict Clippy
passes. An independent 50-request comparison with the original Python dynamic
validator has identical acceptance decisions and complete summaries, including
the repaired unscoped witness case. The actual Python/native dynamic, ordinary
verify, stream-core and controller-binding integrations pass 32 tests, including
the artifact-sized request followed by a second frame in the same worker.
These are functional/tooling checks; no new codec performance conclusion follows.

The reviewed helper release-profile change is integrated with this migration.
Keep both its host-build-policy test and the artifact-sized-request/next-frame
regression when resolving their shared insertion point. Both change-record
entries and the complete synthetic helper profile report remain present. This
combination uses the optimized host executable with the same persistent worker,
request bounds and strict ledger contracts; it does not alter codec sampling.

## Complete worker validation

The following independent review freezes `39f6ee7`, which includes the completed
`d29b264` native worker batch and `01f3667` controller bindings together with the
reviewed scope fix and optimized host-helper build. Remote main remains
`69379b3` and is already in the branch. The frozen Linux campaign is unchanged.

Validate each complete twelve-observation diagnostic worker in one native frame.
The worker reuses the sampling workload/schedule, original row normalization,
caller geometry, source ledger and counter-window models. Row callbacks inspect
original field types before normalization; formal telemetry requirements stay
separate from portable diagnostic observations. Preserve calibration rehearsals,
stream I/O windows, retained owners, exact call/observation binding and complete
summaries. Borrow rows and stream views rather than build their normalized output
only to discard it. Release evidence shares the same checked row normalization.
Its existing bounded cache keys include current input and context, store immutable
serialized results and return fresh dictionaries on every call.

Independent validation of this fixed source passes 101 native unit tests,
25 executable tests, strict Clippy and all 259 Python controller tests. A separate
105-request comparison with `02c6812` covers complete/reversed/partial rows for
all four profiles, three full worker types, calibration and malformed inputs.
All acceptance decisions and complete outputs agree; both paths preserve their
input objects. The complete ordinary, stream and dynamic fixtures are positively
accepted, so matching refusals cannot stand in for successful replay coverage.
The production codec, build script, package manifests and lockfile match main.

The retained [complete-worker comparison](../benchmarks/tooling/2026-10-23/complete-worker-batching.json)
uses the synthetic `WorkerContractTests` stream fixture from this source:
twelve observations, two calls each, 1,200 core events and twelve view ledgers.
Both versions use optimized native helpers. Each A1/B1/B2/A2 phase starts a fresh
Python/native worker; one untimed call validates the common summary and primes
its caches. Three warmup blocks and nine sample blocks each execute five complete
requests. Retain all 48 blocks / 240 requests and compare every returned summary.
The timer includes controller validation, native IPC, decoding and full-summary
comparison; executable build/startup and fixture construction are outside it.

On this local unisolated run, steady-state IPC falls from 55 frames per complete
worker to one. Phase medians are 11.856525 / 8.695400 / 8.7182166 / 11.884275 ms per
request. Before/after same-version repeats differ by +0.2340% / +0.2624%. These are
descriptive timings for one synthetic tooling workload, not codec throughput,
overall RSS, Linux drift attribution or publication acceptance. The full report
keeps candidate attribution null and release approval false.

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
Darwin can return EPERM for an already-exited group; its
[group-signalling implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)
excludes zombies. The earlier exception proved only a sole owned leader. Reviewed
main `62632e5` handles multiple exited members through two complete native
membership snapshots with PID/start identities. Live, unknown or incomplete
groups remain errors and preserve generated inputs. Integrated follow-through
`aa89274` retains main `549b0ac`'s three-second member-reaping stabilization and
strict identity/live/unknown/truncated refusals. The platform reader is separate
from shared complete-pair confirmation; duplicate identities also refuse.
Deterministic real-kernel tests reap during either snapshot, while pure tests
exercise fresh-pair and refusal boundaries. The original macOS CI refusal remains
retained; its exact internal snapshot state was not recorded. This owner supervises
the group it created;
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

## Integration: native worker batches and merged helper

Commits `d29b264` and `01f3667` consolidate the remaining per-record Python
worker checks into one shared Rust request. The same release-row validator now
serves evidence replay and all three source-worker families. Its callbacks see
original field types before normalization, so string schema IDs or boolean
iteration counts cannot gain validity through normalization. Worker rows are
borrowed for validation; no normalized output JSON is built merely to discard it.
Stream ledgers also borrow their views. Sampling, raw source identity, caller
geometry, fault counters and I/O-window enclosure retain their existing rules.
Portable diagnostic telemetry remains explicitly null, while formal sampling
still requires native telemetry; a regression protects that distinction.

Python bindings reuse the existing 512-entry immutable row cache, including the
64 KiB bypass. Keys contain the current values and full validation context.
Changed counts, revisions or inputs revalidate, and callers receive fresh result
objects. This preserves evidence replay's existing cache behavior rather than
replacing cached checks with repeated IPC. One actual worker drops from 42 framed
native requests to one; this is a request-count result, not codec throughput.
The independent synthetic stream fixture above has its own fixed input and
primed-cache context and reports 55 frames to one. Keep those counts attached to
their respective workers rather than treating either as a universal request cost.

The final inventory is 34 Python files / 8,177 lines, about 38% below the original
13,120 lines. This batching step removes 73 Python lines including its regression
coverage; the merged build-policy test adds 14, for a net reduction of 59 since
the previous inventory. All remaining modules retain active callers. At that snapshot the
source-bound helper-profile comparison brings JSON inventory to 577 files /
2,818,980 bytes, with no audit errors. The independently retained complete-worker
comparison subsequently brings the audited inventory to 578 files / 2,828,460
bytes, including 37 versioned records. Historical bytes are unchanged.
Core/WASM package allowlists remain 67/6 members; all five workspace members use
the root dependency catalog. No production codec source or build script changes.

Validation is bound to exact sources. A shared-checkout campaign stopped after
350 complete steps when a parallel merge changed HEAD; its failure is preserved.
An independent clean checkout of `9b75e02` then completes all 574 capture/replay
workers: 516 dynamic workers, 28 ordinary verify workers and 30 stream workers,
covering 6,888 observations. It passes all 126 native tests (101 unit, 25 CLI),
258 Python tests, strict Clippy, Linux compilation, 517 core tests with 43 existing
ignored cases, ten doctests and a package-scoped no_std check. The compatibility
probe again passes 200 exact parity configurations and both decoding directions;
offline packaging rebuilds the extracted 67-file crate.

The later `39f6ee7` integration changes the helper build profile, its test,
documentation and archived benchmark. Its Rust source, codec manifests and
lockfile are identical to `9b75e02`. Optimized native tests again pass all 126
cases, and all 35 relevant current controller/integration tests pass, including
the new optimized-build policy test. Both the development and optimized helpers
agree with the original paths across 7,503 checks: 562 complete real workers,
6,744 raw row batches and 197 mutated inputs. Complete values and refusal
decisions agree, including all newly captured sampling workers. The larger native
counterpart and source-bound replay validations remain distinct from performance
qualification; no new codec latency or throughput claim is made.

## Latest main integration and final tooling checks

Remote main advanced to `24c88c8` during the review. Merge `b52099b` includes its
VM source-overlay cleanup, optional synchronous guest service-counter observer
and GNU metadata-check setup fix. Retain both change-record entries when resolving
the insertion conflict. This adds no production codec or dependency delta against
current main and does not deploy new code into the frozen Linux campaign.

The exact merged source passes 103 optimized native unit tests, 25 executable
tests, strict Clippy and all 273 Python controller tests. The independent
105-request original/new worker and release-row comparison is repeated against
this source: all decisions and complete outputs still match without mutating
input objects. A local x86_64 Linux metadata compilation also checks the native
counter module's Linux-specific code. This is compilation evidence, not a positive
hardware capability probe. Generated observer semantics/cleanup checks remain in
the native/controller suites; the actual optional VM probe waits until the full
capture, resource restoration and original observer-service checks have finished.

The read-only JSON audit now covers 579 files / 2,833,273 bytes, including 38
versioned records, and reports no errors. The added upstream service-drift review
has its own provenance. The earlier helper-profile and complete-worker timing
reports retain their original source identities; their measurements are not
relabeled as timings for the merged tool or as codec performance acceptance.
Version declarations remain 9.1.0, the changelog remains unreleased and final
source-bound release evidence and strict positive preflight are still pending.

## Complete layout analysis and one paired policy

Functional commits `f5c8c24`, `7adcefe` and `cc73d76` move CPU-placement plans and
analysis, Slot histograms, and complete verify-layout reports into the existing
native helper. Review found two further duplicated rules: Slot built a synthetic
Vec fixture for each histogram bucket, and diagnostic replay independently
computed layout ratios and regression limits in Python. Both now use the shared
Rust implementation. The same constants export the original paired protocol;
its schema, integer/float median types, cell ordering and inclusive 5%/12%
boundaries remain unchanged.

The native analyzer validates the declared forward/reverse grid directly rather
than constructing a second expected-row collection. Provenance fields borrow the
input strings, and all source/clone positions use the formal buffer validator.
Each complete CPU/Slot request crosses the helper boundary once. With the
immutable contract cache primed, the fixed synthetic CPU fixture falls from 48
frames to one and the Slot fixture from 24 to one, with identical outputs and
unchanged inputs. These are request counts, not codec throughput measurements.

Diagnostic comparisons retain all 256 cells whether qualified or unqualified.
Failed quality never gains candidate attribution; diagnostic replay always withholds
it, even when the layout gate passes. Capture, cancellation, source ownership,
raw-journal binding and cleanup continue through the existing controllers.
All 38 original subsystem tests passed before 17 pure cases moved to 13 native
tests. Additional native and raw-evidence replay regressions protect diagnostic
mode; 22 controller tests remain across these three suites.

The current inventory is **34 Python files / 8,137 lines**, including 4,004
production/helper lines and 4,133 test/fixture lines. This migration removes 349
lines relative to `75bc034` (8,486), after preserving the independently merged VM
observer checks. The original 56-file / 13,120-line inventory is reduced by about
38%; all retained Python modules have active uses. The read-only audit retains
579 JSON files / 2,833,273 bytes and eleven identical-byte groups with their
separate historical paths. All five workspace members continue inheriting root
dependencies. Core/WASM package allowlists remain 67/6 files, excluding all of
these Python tools, JSON evidence, external tests and benchmarks.

Final validation uses an independent checkout fixed at `cc73d76`, so concurrent
shared-checkout changes cannot alter the tested source. All 143 optimized native
tests (117 unit / 26 executable), 257 Python tests, strict tooling Clippy and
x86_64 Linux compilation pass. The 2,115 original/native data/protocol comparisons
and six source-bound diagnostic replays agree on complete values and refusal
decisions without input mutation. The oracle Python files are byte-identical to
`75bc034`; the local ignored `target/tooling-review-round12` retains their copies,
comparison drivers and source bindings.

The same checkout passes 517 core tests (43 existing ignored cases), ten doctests,
no_std compilation, 200 exact 9.0.0/9.1.0 parity configurations and both decoding
directions. Native probe checks pass for Slot (2), CPU placement (19) and portable
verify layouts (2); the Linux-only timing test remains ignored on this host.
Offline packaging builds the extracted 67-file crate successfully (191.9 KiB
compressed). Formatting, workspace dependency and package allowlist audits pass.
No production codec or build-script delta exists against the reviewed main.
This establishes tooling and compatibility behavior, not new codec performance
acceptance or permission to publish.

## Profile batches, shared baseline statistics and finite arithmetic

Review reproduced a concrete timing regression in the previous native layout
implementation. A finite metric such as `1e308`, multiplied by 100 iterations,
overflows to infinity. Comparing its error against an equally infinite relative
tolerance accepted the input; the original Python rejected it. An independent
request confirms the earlier `cc73d76` helper accepts all 64 layout cells while
the corrected helper refuses the same input. Commit `cc51995` rejects the
nonfinite product before comparison and rejects nonfinite quotients before JSON
can encode them as null. Its standalone checkout passes all 144 native tests and
strict Clippy. A regression exercises the actual complete-grid validation path.

Commits `cae8589` and `46426b6` consolidate the remaining profile grouping,
selection and baseline calculations. A whole profile now shares the existing
worker and raw-row validators. Validation borrows records; the response contains
stable indices rather than copied original rows. Python bindings preserve the
identity of selected rows and grouped objects. Stable nine-sample ordering uses
a fixed array and preserves integer precision and ties. All former normalization
callbacks delegated to the same row contract and have been removed from callers.

The native baseline model retains independent service ranks, unclamped run/wall
ratios, unknown spread for zero runtime, short windows, layout mismatches and every
repeat category. It also computes the complete adjacent schedule, repeat checks
and conservative wall-window separation. Original files, trusted frozen-tool
execution, source identities, snapshot bounds and resource restoration stay with
the existing controllers. Frozen replay disagreement remains a refusal.

The complete v15 protocol moves to the native catalog with its original canonical
digest. Python and Rust share spread/repeat constants, including the formal and
paired-layout consumers. Protocol values, schemas, measured workers and codec
algorithms are unchanged. With the immutable normalization cache primed, one
complete 126-case fixture falls from 126 selection frames to four: smoke 15 to
one, small-file 88 to one, hotspot eight to one and stream 15 to one. Complete
outputs and inputs agree; this is an IPC count, not codec throughput attribution.

Validation is fixed to clean source `46426b6`. All 157 optimized native tests
(130 unit / 27 executable), 256 Python tests, strict tooling Clippy and x86_64
Linux compilation pass. The original 84 subsystem tests passed before two pure
schedule/bound tests moved to Rust; a new controller regression checks one frame
and original object identity. All 2,316 original/native sampling and baseline
comparisons agree, including complete and interleaved profiles, malformed
nonmedian rows, protocol identity, legacy/current provenance and frozen adjacent
service replay. The reference Python files are byte-identical to `4f74e0c`.
Another 2,115 layout comparisons and six raw-evidence layout replays also agree.
The local ignored `target/tooling-review-round13` retains the comparisons,
overflow reproducer and source-bound logs.

The same source passes 517 core tests with 43 existing ignored cases, ten
doctests, no_std and the 200-configuration 9.0.0/9.1.0 parity and bidirectional
decode probe. Both source versions pass 16 generated native checks and all 23
one-iteration smoke/hotspot workers; every hotspot input witness matches.
Offline packaging rebuilds the extracted 67-file crate successfully. All five
workspace members inherit root dependencies, and core/WASM allowlists remain
67/6 files. No production codec, build-script or dependency delta is introduced.

The inventory is now **34 Python files / 7,988 lines**: 3,865 production/helper
lines and 4,123 test/fixture lines. This removes 149 lines from `4f74e0c` and about
39% from the original 13,120-line inventory. Retained modules still have active
uses. The JSON audit remains 579 files / 2,833,273 bytes, with eleven identical
groups retained under their historical paths and no errors. Repository tools and
evidence remain excluded from both published crates. Earlier timing campaigns
retain their original source identities and release-acceptance status.

## VM snapshot and counter consolidation

Review reproduced a counter-reset defect in `diagnose_release_vm.analyze`.
Anonymous slots are stable for the process observer's `(tid, start time)`
identity. One shared thread's run counter can regress from 100 to 2 while another
advances by 200, leaving a positive aggregate of 102. The former aggregate-only
check accepted the data and emitted ordinary shared service. Commit `d7c4c77`
checks run, wait and timeslice counters per shared thread before summing them.
The regression fails before the fix and passes afterward; a standalone checkout
passes all 33 VM/frequency tests. Original snapshots remain available on refusal.

Commits `9b8dedf` and `0e44c58` consolidate the VM data model in
`diagnostics::vm::{process,clock}`. One native path owns snapshot validation,
shared-thread statistics, explicit missing/idle readings, synchronous clock
enclosure and external frequency deltas. The ordered register list is exported
as pairs so JSON object sorting cannot reorder TSC/APERF/MPERF reads. The same
shared baseline module now computes formal service-time summaries. Exact ratios,
the v1 clock schema, alignment boundary, scope and absence of timing normalization
are preserved.

The profile preparation request validates process observations once and returns
both the sampler copy and diagnostic report. Parsed input trees move into the
reply rather than being cloned again. Calibration copies use batches of 64
records. Python retains device reads, capability/source checks, original journals,
worker supervision and cleanup. No formal worker, process-observer template,
clock-reader template or production codec changes are introduced. Obsolete
point/window wrappers and unused Python clock constants are removed.

All 45 original VM/frequency/qualification tests passed before eleven pure checks
moved to native tests. Retained integration tests verify source restoration,
failure journals, zero-codec capability refusal, real Rust clock serialization,
ordered read-only device access and frozen replay. A new boundary test verifies
one profile request, bounded calibration batches and independent returned copies.
The 763 original/native comparisons agree on complete values and refusals for
valid JSON observations, including malformed snapshots, thread changes, read
skew, CPU sets, window boundaries, unavailable readings, probes and every formal
profile's service report. The VM reference is the committed post-fix Python at
`d7c4c77`; the service-summary reference is `e7d1142`. The local ignored
`target/tooling-review-round14` retains the source-bound drivers and reset witness.

The current inventory is **34 Python files / 7,813 lines**, comprising 3,745
production/helper lines and 4,068 test/fixture lines. This removes 175 lines from
`e7d1142`, including the reset fix and retained integration coverage, and about
40% from the initial 13,120 lines. All remaining files have active uses. The JSON
audit retains 579 files / 2,833,273 bytes and eleven historical identical-byte
groups, with no errors. All five members inherit root dependencies; core/WASM
allowlists remain 67/6 files and exclude these repository-only tools and records.

The final independent checkout at `0e44c58` passes all 168 optimized native tests
(140 unit / 28 executable), 247 Python tests, strict tooling Clippy and x86_64
Linux compilation. The same source passes 517 core tests (43 existing ignored
cases), ten doctests, no_std and 200 exact-parity configurations with decoding in
both directions between published 9.0.0 and local 9.1.0. Offline Cargo packaging
successfully rebuilds the extracted 67-file crate. Production sources, build
scripts, sampling/clock templates and dependency declarations are unchanged from
this round's parent. Actual Linux MSR capability is not established by these
portable tests and cross-compilation.

A local optimized preparation diagnostic includes Python serialization, native
execution, response parsing and independent returned records. Its fixed synthetic
input contains 88 small-file cases / 1,056 observations, with six threads per
snapshot. Original and native complete outputs agree before timing; input bytes
remain unchanged afterward. Each A1/B1/B2/A2 phase retains two warmups and nine
timed invocations. Original medians are 103.12/102.59 ms and native medians are
64.59/63.67 ms, with 0.51%/1.42% respective repeat drift. The desktop has no proven
exclusive resource partition, so this remains an overhead observation with null
performance attribution and false release approval. It is not codec timing and
does not qualify a release or a general speedup claim. Raw durations, input hash
and code binding are retained locally with the review artifacts.

## Structural inventory consolidation

Commits `1cffa46` and `b197cdb` remove the remaining Python retained-layout
comparison and raw stream-event signature model. The shared native
`sampling_contract::batch::validate_groups` checks complete workers and original
release rows for both sampling and structural inventories. Provenance validation
borrows the original rows; only the compact cross-phase preparation contract is
serialized back to the stream controller. The typed stream ledger produces its
lifecycle summary and unique shapes from the same validated parse. The native
inventory module compares all six phase pairs and retains sorted cases, exact
changed-buffer fields and the original coverage/refusal flags. The controller
retains the validated lifecycle summaries in observation order.
Capture, original journals, source ownership and cleanup stay in their existing
controller.

The retained inventory covers 103 cases / 4,944 observations / 618 comparisons.
The stream inventory covers 15 cases / 720 ledgers / 90 comparisons. Stream
ledgers remain bounded to four observations per frame. Both modes preserve null
candidate attribution and false release approval. No codec fast path, field,
matrix, format, worker template or dependency declaration changes in this round.
The production sources and build/package inputs also have no delta from the
current branch's `origin/main` comparison base.

The 50 old/native comparisons agree on complete reports and refusal decisions.
The reference is the committed Python inventory at `e7766d7`; its bytes and the
source-bound driver are retained under the ignored local tooling-review output.
Coverage includes interleaved case workers, u64 capacities, changed source and
scratch offsets, malformed nonmedian rows, missing observations/phases,
constructor/backend changes, all 15 stream geometries, 4 KiB/64 KiB pages and
one/two/three-call ledgers. Retained integration checks verify a single request
for the complete retained inventory, at most four stream ledgers per frame and
unchanged original records. Eight additional native tests protect the complete
phase/catalog/schedule, exact difference fields, per-iteration shape deduplication
and lifecycle summaries.

The current inventory is **34 Python files / 7,789 lines**: 3,705
production/helper lines and 4,084 test/fixture lines. The net reduction from this
round's parent is 24 lines, including added binding checks; the reduction from
the original 13,120-line inventory is about 41%. Every retained module has a
caller. The obsolete comparison helper is removed; active capture and replay
modules remain executable while their ownership boundaries are retained.

The final expanded review reproduced an input-boundary regression introduced by
the migration: the previous stream inventory accepted an absent CPU policy,
while converting `None` to a Python list raised a type error before validation.
Commit `4ee794a` retains the optional policy in both bindings and native
preparation. The new native test also rejects a regressing original worker
counter with that policy absent; complete-report integration compares both
policy forms. Native lifecycle-counter requirements remain enforced.

The independent clean checkout at `7fce4b2` passes all 176 optimized native tests
(148 unit / 28 executable), 247 Python tests, strict tooling Clippy, x86_64 Linux
compilation, 517 core tests (43 existing ignored cases), ten doctests and no_std.
The published 9.0.0/local 9.1.0 probe passes all 200 non-fast configurations,
exact parity, decoding in both directions and historical field vectors. All five
workspace members inherit root dependencies. The JSON audit retains 579 files /
2,833,273 bytes and eleven historical identical-byte groups, with no errors.
Core/WASM package allowlists remain 67/6 members. Explicitly selecting the core
package and crates.io registry verifies the extracted 67-file crate offline;
the initial unqualified command encountered the local registry replacement and
default workspace members, and is retained as an unsuccessful invocation.

The local overhead review found redundant work in the first stream migration:
validated lifecycle summaries and repeated shape objects made another complete
JSON roundtrip through the comparator. Commits `7f24aa5` and `7fce4b2` retain the
validated summaries locally and union shapes per case within each bounded native
batch. Private wire tuples carry the same role/index/length/capacity/offset
values. Every original observation index remains present, and the comparator
still requires the complete twelve-observation schedule. A mixed-case native
regression retains both equal and distinct geometries, all lifecycle entries and
the four-ledger bound. Python only relays the resulting summaries and signatures.

The final source-bound synthetic comparison reduces retained-inventory frames
from 88 to one and request bytes from 17,593,995 to 12,026,533. The single retained
frame remains within the existing 32 MiB plus envelope limit. Stream frames fall
from 200 to 185, request bytes from 40,443,047 to 38,521,452, and the largest stream
request is 530,168 bytes. Complete outputs agree and input hashes remain unchanged.
These counts apply to the 103-case retained and 15-case/two-call stream fixtures.
Each A1/B1/B2/A2 phase has two warmups and nine timed invocations. Native repeat
variation reaches 8.72%/7.63%, exceeding the five-percent repeat criterion, and the
desktop has no proven exclusive resource partition. No candidate timing
conclusion or release approval follows from these measurements. The source-bound
drivers, original timings, frame counts, pre-projection trials and fifty-case
semantic comparison remain in the ignored local review artifacts.

## Calibration and campaign validation consolidation

Review reproduced a report defect with finite inputs: a baseline metric of
`1e-308` and a candidate metric of `1e308` produce an infinite quotient. The
previous complete diagnostic accepted the inputs and could serialize `Infinity`.
Repeat drift has the same boundary. Commit `9d30044` refuses both nonfinite ratios;
the complete-replay regression fails before the fix and passes afterward. Original
records remain available when this mathematical boundary is refused.

Commits `d8e964c` and `b05b15f` consolidate the remaining calibration rehearsal and
campaign models in Rust. Complete calibration profiles borrow every original row
through the existing raw-row, workload, telemetry and rehearsal checks. Shared
protocol constants retain the v15 digest, twelve observations per worker, twelve
maximum batches, bounded growth and the complete final rehearsal. Optional CPU
policies remain optional; bound policies validate the original placement and
counters in the same pass. The baseline analyzer, adjacent controls and release
verifier remove their repeated per-record placement/telemetry requests.

Campaign comparison validates complete unique phase/profile/case sets and
comparable provenance before producing all 252 repeat checks and 504 diagnostic
comparisons. Original case order crosses the bridge as ordered pairs. Both
repeat and candidate quotients must remain finite. Short windows, wide
measurements, repeat failures, regression-program outcomes and all original
report fields remain explicit. The controller owns source/file/digest checks and
external regression programs. The intermediate Python ratio helper and duplicate
classification loops are removed. Encoding kernels, field/matrix formats, build
inputs and dependencies remain unchanged.

The 116 original/native comparisons agree on complete values and refusal
decisions. Their reference is the committed post-fix Python at `9d30044`.
Coverage includes all four calibration profiles, interleaved cases, missing and
extra observations, original row types, changed provenance, counter resets,
optional and mismatched CPU policies, formal/diagnostic campaigns, original case
order, exact repeat limits, simultaneous failures, failed regression programs
and both finite-input overflow paths. All 44 original sampling tests pass before
two pure checks move to native coverage. Retained integrations verify one frame,
unchanged inputs, source ownership, journals, frozen replay and release refusal.

On the fixed primed-cache fixture, all 126 calibration cases including CPU and
telemetry validation use four frames instead of 3,175. Complete diagnostic
profile verification uses 135 frames instead of 3,464, with request bytes reduced
from 37,867,709 to 27,983,082. That witness mocks external regression processes
and verifies their existing controller boundary; it measures internal validation
requests, not codec latency, throughput or release acceptance. Complete outputs
and original input hashes agree. Source-bound drivers, counts, comparisons and
the failing overflow witness are retained in ignored local review output.

The independent clean checkout at `b05b15f` passes all fourteen verification steps:
183 optimized native tests (155 unit / 28 executable), 246 Python tests, strict
Clippy, x86_64 Linux compilation, 517 core tests (43 existing ignored cases), ten
doctests, no_std and all 200 published-9.0.0/local-9.1.0 non-fast parity and
bidirectional decode configurations. The extracted 67-file core crate builds
offline. Core/WASM package allowlists remain 67/6 members, and all five workspace
members inherit the root dependency catalog. No upload or push is performed.

The current inventory is **34 Python files / 7,745 lines**, comprising 3,657
production/helper and 4,088 test/fixture lines. This round removes 44 net Python
lines, including the finite-ratio fix and binding regressions; the cumulative
reduction from 13,120 lines is about 41%. Retained files have active callers.
The 579 JSON files / 2,833,273 bytes and eleven historical identical-byte groups
remain valid and unchanged. Repository tools and evidence remain excluded from
both published crates.

## Native baseline directory replay

Commit `411fdb4` adds an independent native baseline capture analyzer and shared
bounded snapshot, runtime and fingerprint implementations. Commit `6b2f61a`
replaces the Python baseline directory pipeline with a thin native call. The new
`analyze-baseline-capture` command loads the original provenance, calibration and
measurement files directly, borrows prepared workload/trace/fixture references,
and invokes the existing complete calibration, worker, provenance, statistics
and report models. It avoids serializing complete input trees between Python
and Rust or sending descriptive reports back through the helper for assembly.
No statistical thresholds or codec implementation changes are introduced.

The source-fingerprint implementation retains the former byte ordering and
matches the Python producer on the real source tree. The standalone command
produces the complete legacy report with an empty PATH, requiring neither Python
nor Cargo at replay time. It still needs the matching tool sources to calculate
the retained source fingerprints. Python keeps its existing CLI and explicit
trusted historical-controller adapter, including isolated execution and the
before/after source, resource and original-member checks. This step does not
replace or bypass the historical controller with a current interpretation.

The shared snapshot reads regular members with a 32 MiB per-file and 256 MiB
aggregate byte budget. Input hashes cover the exact original encoded bytes.
UTF-8, UTF-16 and UTF-32 with or without their supported byte-order marks retain
the former byte-loader behavior. Member hashes and the optional A2 directory are
checked again before returning a report. The analyzer preserves v1 history's
explicit unknown CPU/page policy; v2 still requires the complete recorded runtime.
Shared runtime validation retains libc, allocator environment, base-page and
kernel-policy checks. Three complete-file service tests move to Rust after all
original baseline behavior tests pass; CLI, ownership, frozen replay and malformed
input tests remain active.

The 51 old/native comparisons use the Python analyzer at `9210509`. Forty-nine
retain complete values or refusal decisions, allowing only the expected analyzer
source-stamp difference. Two explicit original-file policy changes refuse linked
members and duplicate JSON keys that the former unbounded byte loader accepted.
Coverage includes basic/v1/v2 provenance, missing policies, original encoded-byte
hashes, one/two phases, optional historical policies, protocol numeric equality,
interleaved workers, changed nonmedian records, counters, repeat limits, short
windows and geometry differences. The new reports retain false release approval
and null candidate attribution.

A primed-helper witness replays every original baseline case. The two-phase
fixture changes from 68 cross-language requests / 15,257,891 request bytes to one
fresh file-adapter request / 289 bytes; the one-phase fixture changes from 39
requests / 9,399,810 bytes to the same small request. The Rust process reads and
rechecks the files directly. Complete report values and original hashes agree.
These are request counts and transfer volumes, with no timing, codec throughput
or release-acceptance inference. Source-bound drivers, reference bytes and
comparison results remain in ignored local review output.

Commit `62bbb43` also consolidates pure runtime and kernel-policy validation
caches. Each current value is serialized before lookup; successful checks alone
are cached. At most 128 payloads below 64 KiB remain resident, bounding cached
serialized text below 8 MiB. Larger inputs are validated without retention. The
binding regression verifies repeated small values, changed invalid input and
large-value bypass. Live policy capture remains a fresh uncached process.

The current inventory is **34 Python files / 7,693 lines**: 3,594 production/helper
and 4,099 test/fixture lines. This round removes 52 net lines including the added
boundary checks. All remaining modules have active capture, replay, adapter or
test responsibilities. Historical JSON bytes and repository-only packaging
boundaries remain intact.

The independent clean checkout at `62bbb43` passes all fourteen final verification
steps: 193 optimized native tests (165 unit / 28 executable), 247 Python tests,
strict Clippy, x86_64 Linux compilation, 517 core tests (43 existing ignored
cases), ten doctests, no_std and the 200 published-9.0.0/local-9.1.0 non-fast
exact-parity and bidirectional-decode configurations. The 579 JSON files /
2,833,273 bytes and eleven historical identical-byte groups remain valid and
unchanged. All five workspace members inherit root dependencies. Core/WASM
allowlists remain 67/6 members; the extracted core package compiles offline.
No production codec, build input or dependency delta exists in this round.


## Current main integration and complete native diagnostic boundary

Merge `709c40b` integrates main `e3aca32`, including the shared ordinary Classic
encoder, adaptive verification, cached automatic GFNI priority, native reporter
framing and the retained complete Linux reviews. Conflicts retain all functional
entries and source-bound historical records. No codec, manifest, lockfile, build
configuration or WASM delta remains against main. The primary working checkout
and its subsequent edits are preserved separately.

One native model owns each validation rule: profile/calibration grouping and
stable median selection, finite campaign arithmetic, CPU/Slot/paired layouts,
VM thread and counter windows, runtime metadata and complete baseline replay.
Whole-worker requests borrow the original values and return original indices.
The Python controllers retain process/resource ownership, source restoration,
trusted historical-controller execution, device reads and retained journals.
The file adapter reads bounded original members directly rather than transporting
complete input trees through Python. No new collector or retry policy is added.

The successful-only metadata cache is keyed by each current serialized value,
with 128 entries below 64 KiB each. Larger values bypass retention. Live policy
capture remains uncached. Source review follows all callers through mutation,
error and restoration paths; no further supported runtime defect was found.
The retained nonfinite product/ratio and individual-thread reset regressions
remain active. The final source preserves validation before aggregation and
refuses malformed original files without rewriting them.

The merged tree passed all 194 optimized native tests (166 unit / 28 CLI),
247 complete Python controller tests, strict tooling Clippy, formatting, spelling
and diff checks. Linux x86_64 all-target metadata compilation passed; this is not
positive hardware-counter capability or Linux ISA execution evidence. Actual
core/WASM package inventories remain 67/6 members and exclude the repository tools.
The code under test is bound to merge `709c40b`; later documentation does not
change the tested Rust/Python implementation.

A new independent old/native comparison retains every result: 51 baseline cases
against Python `9210509`, including two declared stronger refusals for linked
members and duplicate JSON keys, and 116 complete calibration/campaign cases
against post-fix Python `9d30044`. All other complete values and refusal outcomes
match exactly, with unchanged fixture input bytes. The comparisons include
optional policy, UTF-8/16/32, interleaved cases, nonmedian changes, reset counters,
original limits, simultaneous failure classes and nonfinite ratios.

On a fixed primed-helper complete baseline fixture, two-phase replay changes
from 68 requests / 15,257,891 request bytes to one request / 289 bytes; one-phase
replay changes from 39 / 9,399,810 to one / 289. The adapter reads and rechecks
the source-bound files directly. These are IPC and transfer-volume witnesses,
not elapsed-time, codec throughput, memory or release-acceptance measurements.
The [complete derived comparison](../benchmarks/tooling/2026-10-23/native-diagnostic-consolidation.json)
retains all 167 comparison outcomes and both request witnesses.

The source inventory is 34 Python files / 7,702 lines: 3,594 production/helper
and 4,108 test/fixture lines. Before this new derived report, the merged source
retained all 584 JSON files / 3,006,767 bytes without parse errors. All historical
evidence members remain unchanged. These inventories do not introduce a release
condition or justify removing source-bound historical copies.

The live complete Linux diagnostic stays frozen at `9f935ee` against published
9.0.0. This tool integration is not deployed into that owned capture. All original
calibration, ABBA and paired-layout phases continue after statistical failures;
only their complete terminal result and independent frozen replay can inform the
next performance decision. Previously failed repeats and regressions remain
unaccepted. Final source freezing, accepted complete evidence and actual strict
positive release preflight remain outstanding. No tag or registry upload occurs.

## Observer source-plan consolidation

The follow-up starts from merged main `bfff0fc`. Functional commits `3d89e31`
and `0205bc3` move verify, stream-core and dynamic observer source maps and their
byte bindings into one native plan and share the controller's installation and
restoration owner. The old per-file planning wrappers and unused private bridge
operations are retired. Existing transforms and templates remain the source of
truth; replay requests bindings alone instead of returning complete patch maps.
No production codec, Cargo dependency, encoded format or JSON evidence changes.

Eighteen old/native comparisons cover full plans and both projections for all
three observers on current and published-9.0.0 source. Every generated map and
nullable original/diagnostic SHA binding agrees. Permanent native golden tests
protect the maps, and controller tests exercise the shared owner, including
failure restoration and preservation of foreign edits. With tools already built,
installing verify, stream-core and dynamic observers uses 1/1/1 native requests
instead of 4/6/2. This reduces repeated generation, subprocess startup and IPC;
it is not a codec timing or throughput claim.

The experiment also reproduced a build-ownership defect: a shared caller
`CARGO_TARGET_DIR` allowed an older checkout to replace the cached helper path,
causing a new source-plan request to reach an incompatible old protocol. Commit
`8dd9ffe` explicitly keeps helper artifacts in their source checkout, matching
the sampler's existing build policy. A real older-checkout rebuild under the
shared caller cache no longer changes the new plan or its byte bindings. The
updated 29 controller lifecycle tests pass on this fix.

The independently frozen `0205bc3` matrix passes 196 native-tool tests, all 248
Python tests, strict tool Clippy, Linux tool compilation, 356 codec unit and 160
top-level integration tests, ten doctests, codec-only no_std, all audits, actual
package reconstruction, formatting and diff checks. The later build-directory
fix changes only the helper's Cargo command and its regression assertion; its
focused lifecycle and real cross-checkout regressions pass. The 200 non-fast
published/local parity configurations and bidirectional cross-decoding still
pass, as do the historical field vectors. The original 43 ignored codec tests
remain ignored. Windows/platform coverage retains its CI boundary.

The inventory is 34 Python files / 7,683 lines: 3,557 production/helper and 4,126
test/fixture lines. It removes 37 production lines while adding 18 binding and
build-isolation regression lines. After rebasing onto main `892e075`, all 585 JSON files retain their original bytes
and eleven duplicate evidence groups. All five members inherit root dependencies;
core/WASM allowlists remain 67/6, and the actual extracted core archive rebuilds.
Historical release-evidence refusals and acceptance gates remain unchanged.

Final integration is rebased onto main `dc38db9`, preserving its empty-shard
alignment correction. The combined `b743f82` snapshot passes all 196 native
checks, strict tool Clippy, Linux tool compilation, 517 top-level codec tests
(43 existing ignored), ten doctests, no_std, the 200-configuration compatibility
probe, JSON/workspace/package audits and actual extracted-package compilation.
The 248-test final Python run passes on `ce10134`; all Python/tool source and
contracts match the final integration exactly. Its subsequent main delta is
limited to the existing alignment fix, its codec regression and documentation.
The final review update changes documentation only. All 585 JSON members match
main, and core build inputs/codec sources match `dc38db9` without a tooling delta.

## Bound original-file I/O and source-interface review

This follow-up starts from main `549b0ac`; it retains the concurrent cache-metric
and Darwin worker-group fixes. The five functional commits separate the original
reader correction, digest-bound Rust gates, replay integration, retirement of
per-file source interfaces and independent memory-policy test directories.
Production codec sources, build inputs and dependencies are unchanged.

Two deterministic regressions fail on the former reader: replacing a member
after its first read binds the parsed old JSON to different new hash bytes, and
growing a file after its size check bypasses the stated byte limit. Shared reads
now hash and decode the same bounded bytes. Root reports retain their original
parsed snapshot hashes. All 585 archived JSON values match the former reader,
and their original 3,026,754 bytes remain unchanged.

Profile summaries reuse their parsed original rows, reducing three full content
reads to one. JSON/numeric replay journals share the same bound reader, including
calibration, ordinary verify, stream, dynamic, paired and frequency inputs.
Supplement manifests are checked before profile programs and bound again when
consumed afterward. Both native regression programs accept optional declared
JSON digests through the existing bounded snapshot model, closing the later
file-reopen gap without duplicating normalization or threshold models. Valid
reports and unbound JSON/CSV behavior remain unchanged. Exit 1 still represents
a complete statistical refusal; input/launch errors do not become a successful
complete diagnostic. These are tooling I/O and correctness improvements, not
codec throughput or release acceptance measurements.

The source migration removes six private per-file operations and the unused
Python row-file adapter. Seven pure Python tests, plus source assertions in two
binding tests, transfer to four native tests after their original checks pass.
Generated source, API scope, preparation boundaries, deferred metadata and
bounded procfs helper contracts remain covered alongside the Rust owners.
The remaining Python total is 34 files / 7,675 lines: 3,553 production/helper and
4,122 test/fixture lines, including the newly added reader/gate regressions.

A parallel run additionally exposes colliding timestamp-only memory-policy test
directories. A process-local sequence, as used by the other test owners, gives
each fixture its own tree even when clock readings coincide. This is test
infrastructure only and does not change live policy capture.

The corrected frozen code `71d951e` passes all fourteen final checks: 206 native
tests (177 unit / 29 CLI), all 246 Python tests, strict tooling Clippy, Linux
tooling compilation, 519 top-level codec tests, ten doctests, codec-only no_std,
the 200-configuration non-fast compatibility probe, all three audits, actual
extracted-package reconstruction, formatting and diff checks. The 43 existing
codec ignores remain unchanged. The published/local probe verifies exact parity
and cross-decoding in both directions, plus historical field vectors; it does
not certify the older inconsistent fast-one-parity mode. Spelling also passes.
The final documentation commit changes no tested code.

All five workspace members still inherit root dependency declarations, with no
manifest or lockfile delta. Core/WASM allowlists remain 67/6 members and exclude
repository-only tools, Python, tests and archived JSON. All 585 original JSON
members match main. There is no new codec timing result, release approval, tag
or registry upload in this review.


## Current integrated main review

Main `0a85348` combines the earlier process/source ownership work with contiguous
matrix rows and bound original-file replay. Matrix multiplication and elimination
share borrowed row operations; GF8-capable arithmetic uses its existing batch
operator, while other fields retain scalar row traversal. The independent
GF8/GF16 algebra oracle and actual 200-configuration published-9.0.0 parity and
bidirectional cross-decoding probe pass. Reconstruction still publishes only
successfully decoded outputs and preserves late failure and selective recovery.

The new exact source passes 527 all-feature codec tests including 361 library
and 166 integration tests, 239 codec-only no_std, 211 native and 246 Python tests,
strict codec/tool Clippy and Linux tool Clippy. The 43 existing codec ignores stay
ignored. All source inputs remain unchanged during validation. Manifest, workspace,
JSON and actual 67/6-member package-list checks pass. These counts do not relabel
previous checks or imply Linux hardware/timing execution.

The complete independent numerical replay checks both matrix cohorts' 2,208
observations and retains every refusal. The final prototype's local comparison
still has five distribution, fourteen repeat and three regression-case failures.
No new runtime defect requiring another policy branch is established; final
performance approval remains withheld. The running Linux source remains frozen
at `9f935ee`, and its results cannot be relabeled as this later matrix/tool tree.
See the [current readiness review](release-9.1.0-readiness-2026-10-23.md) and
[complete functional record](../benchmarks/release-9.1.0/2026-10-23/integrated-main-functional-review.json).


## Shared preparation projections in native batches

This follow-up starts from main `0a85348`, retaining its matrix-row optimization,
Darwin ownership fixes and original evidence. The codec, build inputs, dependency
manifests and lockfile remain identical to that main. One implementation commit
updates the native model and every bridge consumer together; a separate test
commit freezes complete projected values before this documentation update.

The three Python fixture/workload/RNG projectors are removed. Calibration, sample
selection, ordinary verify, stream and dynamic workers return preparation in
existing validation frames. Directory replay borrows the same Rust model.
Instead of rescanning every record for each case, projection uses two borrowed
passes with ordered case lookups, O(records * log(cases)). No separate preparation
operation, additional validation frame or retained Python implementation is
introduced. Original selected rows still refer to the caller's objects.

Projection preserves the last observed calibration batch for each case, rather
than assuming the largest batch number; it retains original trace order and null
positions when a case has RNG witnesses. All-null traces remain absent. Index-zero
workload selection, completed calibration layouts, sample-only measurement
layouts, nullable placement and exact u64 seeds retain their previous semantics.
Persisted evidence schemas are unchanged; private helper response consumers move
with their source-bound implementation. Historical replay still requires its
matching frozen source and fingerprints.

A differential comparison against the Python functions at `0a85348` agrees on
all 24 profile/role/phase inputs: 756 case groups / 9,072 observations, covering
both calibration roles and every ABBA phase. A permanent Rust digest additionally
covers all 126 workload catalogs in measurement and calibration modes, with no
Python reference or duplicate JSON fixtures retained. Explicit cases cover
interleaved batches, mixed null witnesses, sample/layout selection, u64 values
and unchanged inputs. These are tooling traversal and equivalence checks; they
provide no new codec throughput measurement or release approval.

The frozen implementation `b5ab95e` passes all fourteen complete checks: 214
native tests, 246 Python tests, strict tooling Clippy, Linux tooling compilation,
527 top-level codec tests, ten doctests, codec-only no_std, the 200-configuration
published/local compatibility probe, all three audits, actual extracted core
package reconstruction, formatting and diff checks. The additional digest-only
commit `ccedab1` passes 215 native tests and strict Clippy. The 43 existing codec
ignores remain unchanged. Compatibility checks exact parity and cross-decoding
in both directions, including historical tower-field vectors; the older
inconsistent fast-one-parity mode remains outside this probe's scope.

The inventory remains 34 active Python files / 7,675 lines: 3,547 production/helper
and 4,128 test/fixture lines. Three processing algorithms move out of Python;
remaining modules retain process, file, hardware-observation, explicit historical
replay or integration-test callers. The original 56-file / 13,120-line inventory
is reduced by about 41%. There is no orphaned Python file to delete in this scope.
All 586 JSON files / 3,136,158 bytes match main, including its newly integrated
matrix-row diagnostic. The eleven equal-byte groups retain their original
provenance; none are deleted or silently deduplicated.

All five workspace members inherit the root dependency catalog. Core/WASM
allowlists remain 67/6 members: tools, Python, archived JSON and repository test
harnesses remain GitHub-only. The core package is rebuilt from its actual
extracted contents. There is no new dependency, codec change, release tag or
registry upload in this follow-up.


## Shared original-worker and frequency replay

This follow-up retains merged main `a771c04`, including the preceding native
preparation review and integrated matrix/ownership evidence. Its first functional
commit binds frequency root hashes to their parsed snapshots; its second moves
original-worker replay and complete frequency-worker calculations into shared
Rust models. Codec sources, build inputs and dependency declarations are unchanged.

A deterministic regression replaces the plan or execution journal after the
original bytes are consumed. The former frequency reporter rereads those paths
for its final hashes, allowing parsed old data to be associated with different
new bytes. JSON and frequency roots now share one bounded text snapshot; each
reported hash belongs to the exact bytes used for parsing. The report's plan
also has to agree with the plan that was validated by the controller. This is a
per-member snapshot binding, not a claim of a transactional filesystem snapshot.

Adjacent and frequency replay share `sampling_contract::replay`. It parses the
sample/timing prefixes in one journal pass, compares original JSON values,
validates every raw row, and borrows the native preparation and provenance
models. Full-profile selection and original-worker replay use one stable median
selector. `diagnostics::vm::frequency` then reuses the existing enclosing-counter
and baseline-service models in the same native request. Step, context and replay
identities must agree. No new crate or replacement statistics policy is added.

The original interrupted-final-fragment rule, last-key journal parsing,
integer/float distinctions and signed-zero agreement remain covered. Interval
exports retain integer or floating values as before; mixed numeric enclosure
checks use the shared exact comparator. Reordered/context-mismatched workers,
changed start journals, overlapping/short windows, stale preparation and idle
measuring CPUs remain refused. Python keeps original-file binding, capture and
MSR descriptor ownership; it no longer combines the numerical worker checks.

A differential comparison uses the published Python functions at `ca72d70`.
All 126 adjacent workloads / 1,512 observations agree exactly. The eight-worker
frequency fixture retains all 96 observations and every report value, including
mixed interval types. Its requests fall from 32 to eight and serialized request
bytes from 810,888 to 675,681 after clearing the provenance cache. Every original
member is unchanged after the comparison. These are fixture-specific transport
and correctness results; no codec throughput or release acceptance is inferred.

The complete final file tree, represented by implementation `d1d3402`, passes
all fourteen checks from an independent frozen checkout: 221 native tests,
247 Python tests, strict tooling Clippy, Linux tooling compilation, 527 top-level
codec tests, ten doctests, codec-only no_std, the compatibility probe, all three
audits, actual extracted core package reconstruction, formatting and diff checks.
The main integration preserves the tested file tree exactly. The 43 existing
codec ignores remain unchanged. The 200 non-fast compatibility configurations
check exact parity and cross-decoding in both directions, plus historical
field vectors; they do not certify the older inconsistent fast-one-parity mode.
Spelling also passes.

The inventory is 34 active Python files / 7,690 lines: 3,542 production/helper and
4,148 test/fixture lines. Production Python falls by five lines relative to the
preceding preparation review; the root-binding and single-frame regression adds
20 Python test lines. Remaining modules retain live control, hardware, historical
replay or integration-test callers. All 587 JSON files / 3,140,866 original bytes
match main, including its latest integrated review. Eleven equal-byte groups
retain their provenance and remain repository-only. No obsolete file is removed
without a replacement or caller evidence.

All five workspace members still inherit the root dependency catalog. The core
and WASM allowlists remain 67 and six files, excluding Python, repository tools
and archived evidence. No tag, registry upload, codec-format change or dependency
change is part of this follow-up.


## Native campaign journals and single-read profile evidence

The final integration starts from main `8d59e77`, retaining its scalar-pivot
zero-prefix optimization and new source/evidence checks. This follow-up changes
only repository tooling and its tests. Native identity/journal support and Python
controller integration are separate functional commits; codec sources, build
inputs, dependency declarations and lockfiles match that main.

The previous complete diagnostic read each of its 24 sample members once to
construct worker-journal identities, then reopened them for profile validation.
The new preparation model collects borrowed index-zero identities during its
existing pass. Calibration retains every rehearsal; sample selection retains
original case order. The existing batch responses return these compact identities,
so the journal check reuses the same bound parsed evidence that supplied the
counts, provenance, preparation and medians.

`sampling_contract::journal` matches the two calibration phases and complete ABBA
schedule using the shared profile order. It streams one JSONL event at a time,
checks exact fields/identity, nondecreasing integer timestamps and zero integer
exit status, and returns the original invocation count. The Python identity,
event-order and timestamp loops are removed. Journal verification finishes after
profile validation and before regression programs. Invalid journals cannot
produce a complete diagnostic or start those programs. Persisted schemas and
acceptance thresholds remain unchanged.

Compatibility tests retain CRLF, last-key JSONL behavior, the former numeric
identity equality and exact u64 timestamps. Time and exit fields retain their
separate integer checks. Missing/reordered events, time resets, nonzero exits,
extra keys, boolean times and start events carrying exit status remain refused.
The preparation model separately checks interleaved cases and all rehearsal
starts, rather than returning only the final calibration batch.

A complete comparison against the Python functions at main `ebb60f2` preserves
every report value for 756 workers / 9,072 observations. Content reads fall from
48 to 24 across the same 24 original sample members; the corresponding read bytes
fall from 45,162,120 to 22,581,060. All original files remain unchanged. Three
accepted journal variants and seven refusal variants agree with the former
implementation. The permanent integration regression fails on the old duplicate
read path and passes after the migration; reordered logs are rejected before any
regression subprocess. These are fixture-specific logical-read results, not
physical-disk or codec-throughput measurements.

The frozen final integration `cea646b` passes all fifteen checks in an independent
checkout: 225 native tests (196 unit / 29 CLI), 248 Python tests, strict tooling
Clippy, Linux tooling compilation, 527 top-level codec tests, ten doctests,
codec-only no_std, the 200-configuration non-fast published/local compatibility
probe, all three audits, actual core packaging and reconstruction, extracted
package library tests, formatting and diff checks. The package's 361 library
tests pass from its actual distributable contents; its five existing ignores
and the repository's 43 existing codec ignores remain unchanged. Exact parity,
bidirectional cross-decoding and historical field vectors pass after integrating
the new main matrix path. The older inconsistent fast-one-parity mode remains
outside the compatibility probe. Spelling also passes.

The current inventory is 34 active Python files / 7,688 lines: 3,523
production/helper and 4,165 test/fixture lines. This removes 19 production Python
lines relative to the preceding worker replay, while adding 17 integration-test
lines. Remaining modules still have active control, hardware, historical-replay
or test callers. All 588 JSON files / 3,352,750 bytes match current main, including
its scalar-prefix diagnostic. The eleven equal-byte groups retain their original
provenance; no frozen evidence is deleted or rewritten to reduce the count.

All five workspace members still inherit the root dependency catalog. The core
and WASM allowlists remain 67 and six files. Python, repository tools and archived
JSON stay outside the crates; the extracted-package test confirms the core's
library tests do not require those repository-only inputs. No new dependency,
codec-format change, release tag, registry upload or performance approval is
introduced by this follow-up.


## Native dynamic plans and capture envelope contracts

The final implementation `af1598f` is rebased on main `6a15c5b`. It preserves
main's scalar length validation, scalar unit-pivot optimization and interleaved
GF16 constant-coefficient path. The cumulative follow-up changes only repository
tooling, tests and documentation; codec sources, build inputs, manifests,
dependencies and lockfiles match that main. Native contract implementation and
Python controller migration remain separate functional commits.

Dynamic capture and replay now consume one immutable 516-step native plan,
derived from the shared profile/case catalogs and the original sorted supplement
keys. Python keeps a serialized snapshot and returns fresh caller-owned values
without another native request. The fixed base/sampling environment maps move
into the native protocol module; its existing catalog exports them. Frozen
digests preserve the original plan and environment values without retaining a
second JSON fixture or a second version constant.

The existing diagnostic-ledger frame validates the complete report envelope,
frozen plan, runtime settings, portable/native memory policy and both source
identities together. It streams all 1,032 begin/end execution boundaries, retaining
only the current parsed event. Flat dynamic contexts reuse the shared allocator
key and memory-policy rules while preserving portable empty libc fields. Python
retains bounded original-file reads, Git binding and actual capture/process
ownership. Each validated source is bound once; the duplicate tool/candidate
binding and the later per-role source rechecks are removed.

A comparison against the original Python at `2daca56` preserves every plan and
environment value, all original plan/journal bytes, six accepted variants and
twelve refusal variants. Accepted variants cover portable/native contexts, CRLF,
last-key JSONL objects and recursive numeric equality, including signed zero.
A new negative-integer regression first fails with the initial comparator and
passes after preserving the full signed range. Integer/float identity comparison
remains exact at the i64/u64 boundaries; explicit page/CPU and memory-policy
integer checks remain strict. The comparison measures
three source-identity queries before migration and two afterward at the envelope
boundary. This is a complete synthetic envelope/plan/journal comparison with
mocked Git identities, not a replay of all captured workers or a codec-throughput
measurement. Permanent controller tests additionally require one native frame,
two source lookups and fresh plan values without extra IPC.

The frozen implementation passes all fifteen checks in an independent checkout:
230 native tests (201 unit / 29 CLI), 249 Python tests, strict tooling Clippy,
Linux tooling compilation, 536 top-level codec tests, ten doctests, codec-only
no_std, the 200-configuration non-fast published/local compatibility probe,
all three audits, actual core packaging/reconstruction, extracted package library
tests, formatting and diff checks. The extracted package passes 370 library
tests from distributable inputs. Its five existing ignores and the repository's
43 existing codec ignores remain unchanged. Exact parity, bidirectional
cross-decoding and historical tower-field vectors pass after integrating main's
GF16 path. The older inconsistent fast-one-parity mode remains outside that
compatibility probe. Spelling also passes.

The current inventory is 34 active Python files / 7,658 lines: 3,484
production/helper and 4,174 test/fixture lines. This removes 39 production Python
lines and adds nine integration-test lines relative to the preceding journal
review. Every remaining module has active control, hardware, historical replay
or test callers. All 590 JSON files / 3,464,634 bytes match main, including its
GF16 and scalar unit-pivot diagnostics. Eleven equal-byte groups retain their
provenance; no frozen evidence is rewritten or deleted to reduce the inventory.

All five workspace members still inherit the root dependency catalog. Core and
WASM package allowlists remain 67 and six members, excluding Python, repository
tools and archived JSON. No new CLI command, dependency, codec-format change,
release tag, registry upload or performance approval is introduced.


## Original diagnostic journals in native worker frames

The final implementation `12144ea` starts from main `a773e74`. Codec sources,
build inputs, dependency declarations and lockfiles match that main. Native
journal integration and Python controller migration are separate functional
commits. Shared models and existing request frames own the validation.

Ordinary verify, stream-core and dynamic replay previously scanned each numeric
journal three or four times in Python. They then resent the same observations
for pair-contract checks; dynamic replay additionally expanded the workload
marker again. The existing native worker now receives a journal path and its
declared digest. It reuses the bounded original-member reader, hashes the exact
bytes before UTF-8 decoding/newline normalization, and visits the relevant
prefixes once. Complete samples, timing fields, observer ledgers and stream views
retain their original count and agreement checks. Decoder supplement replay uses
the same reader and scanner in its existing ledger frame.

The shared scanner retains CRLF/CR input, last-key JSONL objects and the former
interrupted-final-fragment rule. Unrelated prefixes are skipped before JSON
decoding. Complete integer `-0` tokens retain Python integer-zero semantics;
floating/exponent negative zero, escaped strings and invalid tokens are
unchanged. The existing Python-compatible numeric comparison moves into a shared
artifact module. Crucially, value equality does not replace
the actual raw samples or views with their stored JSON equivalents. Typed worker
validation still sees the original parsed log values. A regression accepts a
stored float view index paired with an original integer index, and refuses the
reverse pair, matching the former controller/worker path. The native contract
check uses the original rows and returns the first pair reference and dynamic
workload trace from that same validated model.

The obsolete Python dynamic-workload expansion wrapper, its private bridge
operation and two pure Python tests are retired. Their original tests pass before
removal. The marker/formal-profile refusal and complete 516-step plan assertions
now live beside the owning native modules; caller-value isolation remains a
Python binding test. Process, overlay rollback, original private-log checks and
capture ownership remain in the existing controllers.

A complete comparison against the Python verify-scratch replay at `a773e74`
retains every report value for 28 workers / 336 observations and leaves every
original member unchanged. Repeated worker/contract frames fall from 56 to 28;
serialized request bytes fall from 1,552,340 to 1,369,950. Seven accepted variants
and five refusal variants agree, including malformed unrelated prefixes that
retain their ignored behavior and raw float/bool indices that must not bypass
strict worker validation. The permanent full-controller regression fails
on the former Python parsing path and passes with one frame per worker, without
a second observation-contract request. These are synthetic replay/transport
results; they do not measure codec throughput or establish release performance.

A separate portable synthetic encode ledger checks all 625 events and twelve
observations for current/legacy source identities in both sample and calibration
modes. The new bound-journal interface preserves the original interface's complete
summary and preparation for all four combinations. This exercises the native
model, not actual allocator capture or every dynamic workload. Native source-copy
execution remains a Linux CI check.

The final implementation passes all fifteen checks in an independent frozen
checkout: 238 native tests (209 unit / 29 CLI), 250 Python tests, strict tooling
Clippy, Linux tooling compilation, 536 top-level codec tests, ten doctests,
codec-only no_std, 200 non-fast published/local compatibility configurations,
all three audits, actual core package reconstruction, extracted-package library
tests, formatting and diff checks. The extracted package passes 370 library
tests with five existing ignores; the repository retains 43 existing codec
ignores. Exact parity, bidirectional cross-decoding and historical tower-field
vectors pass. The older inconsistent fast-one-parity mode remains outside that
compatibility probe. Spelling also passes.

The inventory is 34 active Python files / 7,653 lines: 3,471 production/helper
and 4,182 test/fixture lines. Production falls by 13 lines; moving the two pure
tests and adding integration regressions increases Python test lines by eight,
so the total falls by five. All 590 JSON files / 3,464,634 bytes match main.
The eleven equal-byte groups retain their original provenance. No historical
member is rewritten or deleted to reduce the inventory.

All five workspace members still inherit the root dependency catalog. Core and
WASM package allowlists remain 67 and six files, excluding repository tools,
Python and archived JSON. No dependency, codec-format change, release tag,
registry upload or performance approval is introduced.


## Native paired-layout directory replay

The final implementation `c2bde58` follows the native journal tree `fffe679`.
Codec sources, build inputs, dependency declarations and lockfiles continue to
match main `a773e74`. Native directory replay and Python controller migration
are separate functional commits. They extend the existing layout analyzer and
shared artifact modules without another command or dependency.

The former Python replay loaded and validated every paired manifest, phase
metadata, sample array and measurement journal, then serialized all four complete
arrays into a second native analysis request. The native replay now receives the
directory, declared manifest digest when supplied, expected tool contract and
current Git source identities. It binds all thirteen original members, validates
context/source/member agreement and invokes the same paired grid analysis on the
owned original rows. Python retains Git and compiler-state binding and capture,
cancellation and overlay ownership.

Strict artifact JSON and historical Python JSON retain explicit decoding paths
inside one bounded snapshot reader. Both paths hash the bytes actually decoded,
apply the same 32 MiB member / 256 MiB snapshot budgets and reject linked members.
Successful replay rechecks every bound original before returning. The shared
`artifact::python_json` module now owns value equality and compatible decoding;
original-output scanning uses the same decoder. Last-key objects, integer `-0`,
float/exponent negative zero and escaped strings keep their prior rules. Audit
JSON remains strict. The original regular-member error prefix is preserved while
identifying a refused symlink, retaining historical baseline replay diagnostics.

Every phase still requires complete runtime policy, canonical reserved CPUs and
matching context, measuring CPU, source and tool provenance. Identical measured
tree/lock values remain valid after rebase changes the commit. Explicit null,
boolean, numeric or mismatched manifest digests remain invalid. The former
first-phase numeric iteration comparison is preserved; later phases retain their
strict integer check. Formal and diagnostic modes keep separate manifest names,
quality limits, attribution fields and release-approval fences.

A complete comparison against Python at `fffe679` retains every report value for
four phases / 3,072 observations. All original files remain unchanged. Twelve
accepted variants and nine refusal variants agree, covering rebase, diagnostic
mode, duplicate keys, CRLF/CR, final fragments, negative integer zero, first-phase
float iterations, changed sources, links, hashes, mode and reservation types.
Repeated native requests fall from two to one; request bytes fall from 1,618,468
to 1,653. The permanent controller regression fails on the old Python reader and
passes with one request below 8 KiB, without Python member/journal parsing. The
compiled existing `analyze-layout` command also replays the complete directory
with an empty PATH, without Python or Cargo. Supplied Git identities remain an
explicit caller binding. These are synthetic replay/transport checks, not a new
performance acceptance or actual codec-throughput measurement.

The final file tree passes all fifteen checks in an independent frozen checkout:
242 native tests (213 unit / 29 CLI), 252 Python tests, strict tooling Clippy,
Linux tooling compilation, 536 top-level codec tests, ten doctests, codec-only
no_std, 200 non-fast published/local compatibility configurations, all three
audits, actual core packaging/reconstruction, extracted-package library tests,
formatting and diff checks. The extracted package passes 370 library tests with
five existing ignores; the repository retains 43 existing codec ignores. Exact
parity, bidirectional cross-decoding and historical tower-field vectors pass.
The older inconsistent fast-one-parity mode remains outside that probe. Spelling
also passes.

The inventory is 34 active Python files / 7,607 lines: 3,403 production/helper
and 4,204 test/fixture lines. Production falls by 68 lines; the two integration
regressions add 22 test lines, so the total falls by 46. Every remaining module
retains an active capture, platform, rollback, trusted historical-replay or test
caller. All 590 JSON files / 3,464,634 bytes and eleven equal-byte groups retain
their original provenance and match the integrated source baseline.

All five workspace members still inherit root dependencies. Core/WASM crate
allowlists remain 67/six files, excluding Python, repository tooling and archived
JSON. No dependency, codec-format change, release tag, registry upload or
performance approval is introduced.


## Native source and overlay fingerprints

The final implementation `a4976bd` follows the paired replay tree `b3296b5`.
Codec sources, build inputs, dependency declarations and lockfiles continue to
match main `a773e74`. Native fingerprint/render support and Python controller
migration remain separate functional commits, extending the existing helper and
fingerprint module without another crate or command.

Native, sampler and stream-view implementation identities now share the existing
Rust fingerprint model. Python no longer concatenates every native source file
into one byte string. Each request reads the current source bytes; identities
are not memoized. Absolute native roots and explicit sampler/view controller
paths retain the original caller-file and template bindings. Relative source
labels use platform separators, matching the original pathlib spelling. Existing
standalone native sampling fingerprints reuse the same model.

Both existing render requests retain their default file-map response and now
support digest-only and combined-plan output. One shared fingerprint hashes the
original sorted filename/NUL/root-normalized text byte stream. Installation
receives its files and digest from the same generated map. Verify/stream replay
requests one digest for the common worker instead of retrieving and hashing two
complete maps. Dynamic and retained-view inventories use the same digest path.
Paired-layout replay computes its source-file contract within that same request;
standalone capture uses the same native contract builder. The existing explicit
contract request mode remains accepted. This avoids a fingerprint prequery and
preserves the single-frame directory replay guard. Generated worker code, source
transforms and report/digest field names remain unchanged.

The retired Python source-byte assembler, duplicate overlay-hash function and
unused imports are removed. Native golden tests cover the original source byte
streams, live source changes, caller-file binding, source-path spelling and the
exact overlay normalization stream. Bridge tests cover default, digest and plan
outputs for formal sampling and four diagnostic renderers. The existing complete
controller regression additionally requires one digest-only render request.

An original-Python comparison at `b3296b5` preserves all three source identities
and all five render variants, plus the paired tool contract. Every generated byte
and digest agrees across files/digest/plan outputs. In a complete synthetic verify
replay, all 28 workers / 336 observations retain the exact report and original
files. Render requests
fall from two to one; their actual replies fall from 443,506 bytes to 67 bytes.
Each individual digest reply is 67 bytes instead of its 221--256 kB file map.
These are specific generated-source/replay transport checks; no codec-throughput
or release-performance conclusion is inferred.

The final file tree passes all fifteen checks in an independent frozen checkout:
245 native tests (216 unit / 29 CLI), 252 Python tests, strict tooling Clippy,
Linux tooling compilation, 536 top-level codec tests, ten doctests, codec-only
no_std, 200 non-fast published/local compatibility configurations, all three
audits, actual core packaging/reconstruction, extracted-package library tests,
formatting and diff checks. The extracted package passes 370 library tests with
five existing ignores; the repository retains 43 existing codec ignores. Exact
parity, bidirectional cross-decoding and historical tower-field vectors pass.
The older inconsistent fast-one-parity mode remains outside that probe. Spelling
also passes.

The inventory is 34 active Python files / 7,596 lines: 3,390 production/helper
and 4,206 test/fixture lines. Production falls by 13 lines; the integration guard
adds two lines, so total Python falls by eleven. Every remaining module retains
an active caller. All 590 JSON files / 3,464,634 bytes and eleven equal-byte groups
retain their original provenance. The core/WASM allowlists remain 67/six files,
excluding Python, repository tooling and archived JSON; all five members inherit
root dependencies. No dependency, codec-format change, tag, registry upload or
performance approval is introduced.


## Native complete sampling campaign replay

The final implementation `b3f7341` follows source fingerprint integration
`2e67ea7`. Codec sources, build inputs, dependency declarations and lockfiles
continue to match main `a773e74`. Native replay and Python integration remain
separate functional commits, using the existing sampling command and helper.

The complete sampling controller previously decoded 24 original calibration and
phase members in Python, submitted each full profile to Rust, projected paired
contracts in chunks and compared preparation catalogs after receiving them back.
The diagnostic journal required a separate transfer of its text and all worker
starts. One native directory request now owns this validation sequence. It reuses
calibration counts, borrowed preparation contracts, fixed-worker grouping, median
selection, release-row normalization and the original journal validator. There is
no second implementation of those models. The source/protocol preflight, Git and
compiler binding, regression programs and statistical refusal wrapper remain in
the controller.

Each original member is decoded once in Rust from the bytes bound to its declared
hash. Legacy Python JSON duplicate keys, integer negative zero and text newlines
retain their behavior. The native reader rejects unsafe paths, wrong hashes and
incomplete raw profiles before comparisons. Before returning, it rereads the
bound originals to verify that they are unchanged; those final hash reads are
additional native I/O, rather than another JSON decode. The fixed 24-member
catalog plus optional journal sets a total budget of 25 times the existing
32 MiB per-member limit. Other snapshot users retain their existing 256 MiB
budget. This avoids narrowing the former complete campaign's per-member limits.
Only two calibration arrays and the current phase array are retained at a time,
plus compact preparation and report catalogs.

Conditional RNG replay checks still compare a pilot trace only when that pilot
used the final common count. All phase workloads, traces and layouts must also
agree. Selected raw medians must normalize to the originally declared summary;
claimed summaries cannot replace fixed observations. The complete diagnostic
returns the original worker invocation count without sending worker arrays back
to Python. Release distribution failures still raise the same EvidenceFailure
details; complete diagnostic refusals retain their report fields.

A source-bound original-Python comparison at `2e67ea7` covers 756 workers / 9,072
observations. Two accepted variants (ordinary and legacy JSON) preserve every
complete diagnostic report field. Five refused variants cover declared hashes,
unsafe declarations, incomplete raw rows, changed preparation and changed
medians. Every original file remains unchanged. With the same fixture source and
compiler bindings, regression subprocess results and cleared pure row/provenance
caches, persistent helper frames fall from 147 to 13 and request bytes from
28,536,879 to 1,002,306. Python sampling-member reads fall from 24 / 22,581,060
bytes to zero. The sampling/journal stage itself is one native request; the other
frames belong to the retained controller stages. These are synthetic transport
and report-equivalence checks, not codec throughput or release-performance proof.
The existing compiled `sampling-contract` command also replays the complete
sampling/journal request with an empty PATH and matches the report fields,
without Python or Cargo.

Native tests cover the complete campaign, all 504 spread rows, 756 journal
invocations, original JSON behavior, unchanged members, forged declarations,
changed medians/counts and journal hashes. A reader test covers inclusive fixed
catalog budgets. The existing Python integration guard refuses any Python raw
sampling-member read and requires one sampling campaign frame smaller than one
tenth of the original sample bytes.

All fifteen final checks pass in an independent frozen checkout: 248 native
(219 unit / 29 CLI), 252 Python and 536 top-level codec tests, ten doctests, strict
tooling Clippy, Linux tool compilation, codec-only no_std, compatibility, all
three audits, actual core packaging/reconstruction, extracted-package tests,
formatting and diff checks. The extracted package passes 370 library tests with
five existing ignores; the repository retains 43 existing codec ignores. All 200
non-fast published-9.0.0/local-9.1.0 configurations pass exact parity and
bidirectional cross-decoding, plus historical tower-field vectors. The older
inconsistent fast-one-parity mode remains outside that probe. Spelling passes.

The inventory is 34 active Python files / 7,527 lines: 3,320 production/helper
and 4,207 test/fixture lines. This round removes 70 production Python lines and
adds one integration-test line, for a net reduction of 69 lines. Three obsolete
Python validation/member adapters are removed; remaining modules retain active
callers. All 590 JSON files / 3,464,634 bytes and eleven equal-byte groups retain
their provenance. The core/WASM allowlists remain 67/six files; repository tools,
Python and archived JSON remain GitHub-only. All five members inherit root
dependencies. No dependency, codec format, tag, registry upload or performance
approval is introduced.


## Original summaries and borrowed campaign statistics

The final runtime `882efff` is rebased onto main `afb9a52`, retaining the newly
merged Darwin owned-process exit fix. Codec sources, build inputs, dependency declarations
and lockfiles match that main. The original Python comparison remains bound to
`580a6df` and is rerun against this final runtime. The native implementation and
Python integration are separate functional commits. The existing sampling command and helper remain
the entry points; no crate or command is added.

The remaining complete-profile flow decoded sixteen declared summary files in
Python and sent their rows to native normalization. It then transferred all
normalized rows into sampling replay and into another statistical request.
The existing campaign snapshot now reads and normalizes those original summaries
alongside the raw calibration, samples and journal. The row validator's persisted
artifact-kind mapping also defines the filenames, avoiding another profile-name
mapping in production. Statistical analysis exposes its existing model over
borrowed ordered rows; both the standalone statistical request and directory
replay invoke that same function. No second arithmetic implementation is added.

The complete directory catalog is sixteen summaries, twenty-four raw members and
an optional journal. Its fixed aggregate budget is 41 times the unchanged 32 MiB
per-member limit. The existing explicitly supplied-summary interface retains its
25-member budget and response fields. Other snapshots retain their 256 MiB
budget. All declared originals are decoded from bound bytes and reread for a final
hash check before returning. This adds summary revalidation reads in Rust while
removing Python summary parsing and repeated transport.

Summary input arrays retain their first-seen case order through normalization and
statistical reports. Median comparison uses the same original numeric equality;
legacy duplicate keys, integer negative zero and newlines retain their behavior.
Formal distribution refusals retain priority over campaign ratio analysis.
The controller receives its report, case counts and the A1 small-file rows used
by the paired-layout binding. Live Git/compiler checks, source/protocol preflight,
regression subprocesses and final approval rules remain in their existing flow.
The obsolete Python path validator and statistical transport wrapper are removed.

An original-Python comparison at `580a6df` covers 756 workers / 9,072 observations.
Four accepted variants preserve every complete diagnostic report field: ordinary,
legacy raw JSON, reversed original summary order and legacy summary JSON. Seven
refused variants cover original summary hash/path declarations, raw hash/path
declarations, incomplete observations, changed preparation and changed medians.
Every original file remains unchanged. Under the same fixture source/compiler
bindings, mocked regression subprocess results and cleared pure row/provenance
caches, persistent helper frames fall from thirteen to four and request bytes
from 1,002,314 to 19,370. Python campaign member reads fall from sixteen summaries /
273,528 bytes to zero. These are synthetic transport and report-equivalence
checks, not codec-throughput or release-performance conclusions.
The existing compiled `sampling-contract` command also runs the complete directory
request with an empty PATH and matches its sample, worker and statistical fields,
without Python or Cargo.

Native tests cover both supplied and original summary inputs, the complete case
counts, original case order, all statistical comparisons, summary path/hash
refusals, forged medians and formal spread-refusal precedence. Python integration
refuses any summary/raw member read, requires one campaign frame without supplied
rows and bounds it below one hundredth of the original sample bytes. The former
path-helper test now exercises traversal and symlinks through complete replay;
the diagnostic approval test also checks the unchanged manifest and one frame.

All fifteen final checks pass in a new independent frozen checkout after rebase:
253 native (224 unit / 29 CLI), 252 Python and 536 top-level codec tests, ten doctests, strict
tooling Clippy, Linux tool compilation, codec-only no_std, compatibility, all three
audits, actual core packaging/reconstruction, extracted-package tests, formatting
and diff checks. The three additional native tests come from
the merged Darwin exit-confirmation fix. The extracted package passes 370 library tests with five existing
ignores; the repository retains 43 existing codec ignores. All 200 non-fast
published-9.0.0/local-9.1.0 configurations pass exact parity and bidirectional
cross-decoding, plus historical tower-field vectors. The older inconsistent
fast-one-parity mode remains outside that probe. Spelling passes.

The inventory is 34 active Python files / 7,495 lines: 3,283 production/helper
and 4,212 test/fixture lines. This round removes 37 production lines and adds five
integration-test lines, for a net reduction of 32 lines. Remaining modules retain
active callers. All 590 JSON files / 3,464,634 bytes and eleven equal-byte groups
retain their provenance. The core/WASM allowlists remain 67/six files, excluding
repository tooling, Python and archived JSON. All five members inherit root
dependencies. No dependency, codec format, tag, registry upload or performance
approval is introduced.


## Owned release rows and native harness fingerprints

The final implementation `adbac76` is based on main `5ca2c20`, after PR #80 merged.
Only the owned-row and harness commits follow that main in this PR. The merged
GF16 norm inverse remains intact; codec sources, build inputs, dependency
catalogs and lockfiles match main. The row ownership and harness migration are
separate functional commits.

The release-row model now returns its validated owned case/row tuples directly
to native summary and median consumers. Input arrays move into the normalizer;
consumers no longer serialize those tuples into generic JSON and immediately
deserialize them. The external `release-rows` response keeps its original flat
record array. Its obsolete private indexed-JSON adapter is removed. Validation
still runs before normalization, and case order, extra fields and captured counts
retain their contracts. A regression test proves that a retained extra payload
keeps its original string allocation across normalization; all four complete
profile catalogs also preserve their record views.

The original harness digest uses the existing Rust fingerprint bridge and
source reader/block selector. Its five file-label/NUL/raw-byte/NUL streams and
eleven first-selected source blocks retain their exact order and separators.
Unit-source text uses universal newline conversion; the five raw files keep their
original bytes. Source identities are read fresh. This recipe remains independent
of the repository tool's implementation, so bare published-baseline checkouts
need only their original harness sources. Python no longer concatenates those
source bytes or performs its own block extraction for this identity.
Native dispatch selects native, sampling, view and harness recipes in one match.
Golden coverage includes Unicode, LF/CRLF/CR, first-match duplicate declarations,
unselected text, changed source bytes and missing source blocks.

An original-Python comparison at `4450ea2` preserves the real-checkout harness
digest. The complete report comparison covers 756 workers / 9,072 observations:
four accepted variants (ordinary, legacy raw JSON, reversed summaries and legacy
summary JSON) retain every report field; seven invalid variants retain refusal
outcomes. All original files remain unchanged. The existing four-frame transport
fixture stays at four frames, with no Python campaign member reads. The compiled
complete directory CLI also matches its fields with an empty PATH, without
Python or Cargo. These checks establish ownership and report/hash compatibility;
codec throughput requires separate controlled performance evidence.

All fifteen checks pass in a new independent checkout of the final main-based
tree: 255 native (226 unit / 29 CLI), 252 Python and 538 top-level codec tests,
ten doctests, strict tooling Clippy, Linux tool compilation, codec-only no_std,
compatibility, all three audits, core packaging/reconstruction, extracted-package
tests, formatting and diff checks. The package passes 372 library tests with
five existing ignores; the repository retains 43 existing codec ignores.
All 200 non-fast published-9.0.0/local-9.1.0 configurations pass exact parity and
bidirectional cross-decoding, plus historical tower-field vectors. The older
inconsistent fast-one-parity mode remains outside the probe. Spelling passes.

The inventory is 34 active Python files / 7,488 lines: 3,276 production/helper
and 4,212 test/fixture lines. This round removes seven production Python lines.
The 591 JSON files / 3,520,171 bytes now include the GF16 norm report
merged on main; this PR changes no JSON artifacts. Core/WASM allowlists remain
67/six files and exclude repository tools, Python and archived evidence. All five
workspace members inherit root dependencies. No dependency, codec-format change,
tag, registry upload or release-performance approval is introduced by this PR.


## Native campaign source and environment bindings

The final runtime `51194e9` is based on main `801f499`, retaining the merged
GF16/GFNI changes and their archived observations. Codec sources, build inputs,
dependency declarations and lockfiles match main. PR #85 is already merged; only the new native binding and Python controller
commits follow this main. These remain separate functional commits.

The complete profile controller previously compared source identities, harness,
resource settings and every environment copy in Python, with a separate native
runtime-validation request. The existing campaign replay now performs those
checks against fresh caller-owned Git/compiler observations in its own frame.
Live source inspection, tracked-cleanliness checks, compiler execution and harness
collection stay in the controller; the Rust model compares those observations to
the recorded metadata before opening original members. The published baseline
anchor and runtime validator are shared with existing replay models.

Full source identifiers, lock digests, published-baseline equality, refusal of
self comparisons, candidate tree/lock binding and harness agreement retain their
rules. A rebased candidate may keep the same tree and lock while changing commit.
The reference context retains strict integer/reserved-CPU and false-flag checks;
JSON truthiness and equality keep the former Python behavior for copied contexts.
All ABBA phases and the final environment snapshot must still match. Compiler
checking remains optional for the same callers. Existing requests without live
bindings retain their previous partial-replay behavior and response fields; no
standalone request can approve a release.

An original-Python comparison at `d0c9265` covers 756 workers / 9,072 observations.
Six accepted variants preserve all report fields, including legacy JSON, original
summary order changes, same-tree rebases and copied-context numeric equality.
Fourteen invalid variants retain refusal outcomes for source/version/harness,
compiler/environment changes, strict reference flags and original-member
integrity. All original files remain unchanged. The synthetic replay requires
three persistent frames instead of four. Carrying the live binding observations
increases total request bytes from 19,390 to 19,537 in that fixture; Python
campaign member reads remain zero. Evidence establishes fewer round trips and
retained contracts, with no codec-throughput or release-performance conclusion.
The compiled directory CLI also replays the bound request with an empty PATH,
without Python or Cargo. The original harness digest still agrees with Python.

Native tests cover rebased commit handling, compiler inclusion/omission, source
and harness refusal, strict flags, runtime data, final-context changes and
unchanged recorded inputs. The existing integration guard now requires bindings
on the single campaign frame and refuses a Python runtime-validation prequery.
Diagnostic approval and manifest-immutability checks remain in the suite.

All fifteen checks pass in an independent frozen checkout: 257 native,
252 Python and 547 top-level codec tests, ten doctests, strict tooling Clippy,
Linux tool compilation, codec-only no_std, compatibility, all three audits,
core packaging/reconstruction, extracted-package tests, formatting and diff checks.
The extracted package passes 379 library tests. Existing ignore counts are
retained. All 200 non-fast published-9.0.0/local-9.1.0 configurations pass exact
parity and bidirectional cross-decoding, plus historical tower-field vectors.
The older inconsistent fast-one-parity mode remains outside that probe. Spelling
passes.

The inventory is 34 active Python files / 7,461 lines: 3,247 production/helper
and 4,214 test/fixture lines. This round removes 29 production lines and adds two
integration assertions, reducing total Python by 27 lines. All remaining modules
retain callers. The 614 JSON files / 6,382,419 bytes include the additional
observations merged on main; this change leaves those artifacts intact. Published
packages still exclude Python, repository tools and archived evidence. Every
workspace member inherits root dependencies. No dependency, codec format, tag,
registry upload or release-performance approval is introduced.


## Native paired and full campaign association

Runtime `0705f21` is based on main `e3a1c1c` after PR #87 merged. The three
functional commits separate the native model from its Python callers. Codec
sources, build inputs, dependency declarations and lockfiles match main,
including the integrated recovery/GF16 work. No additional runtime defect or
supported codec tuning change was found in this tooling diff.

The paired original-directory replay now compares the supplement with the full
campaign in its existing frame. The same model checks frozen source identities,
context, tool tree/lock, both calibrated buffered-verify counts, and the original
backend/features/target through the shared benchmark alias parser. Capture uses
the same bounded-count function. Python no longer implements this association
or requests per-row provenance normalization. Three unused row/provenance adapters
and their unused cache are removed; the active shared transport cache remains
covered by binding tests. The controller passes only the two relevant
original rows and their counts. Optional binding preserves independent layout
replay and all prior report fields. Neither request can approve a release alone.
Live Git/compiler execution, early supplement/path/hash checks, process ownership
and final approval remain with the controller.

An old/new comparison against `2b80d73` covers all 756 synthetic workers and
9,072 observations. Six accepted variants preserve every report field; 21 invalid
variants retain refusal outcomes. These include legacy JSON and numeric context
equality, same-tree rebases, corrupt original members, and independently valid
layouts with different source, context, calibration, backend or target. Original
files remain unchanged. Requests fall from 3 to 2; adding compact association
metadata changes request bytes from 19,537 to 22,023. Python reads no
campaign members in either implementation. This is request/contract evidence,
with no codec throughput claim. The compiled directory CLI also replays the
bound campaign without Python or Cargo on PATH.

All 15 final checks pass in an independent frozen checkout: 260 native and
252 Python tests, 552 top-level codec tests, ten doctests, strict tooling
Clippy, Linux tool compilation, codec-only no_std, all three audits, actual Cargo
package/reconstruction, 384 extracted-package library tests, formatting and
diff checks. The 200 published-9.0.0/local-9.1.0 non-fast configurations retain
exact parity and cross-decoding in both directions, plus historical tower-field
vectors. The older inconsistent fast-one-parity mode remains outside that probe.

The inventory is 34 active Python files / 7,416 lines, split into
3,198 production/helper and 4,218 test/fixture lines. This round removes
49 production lines and adds four integration test lines, reducing total
Python by 45 lines. All remaining production modules have importers; live capture
and resource cleanup remain active. No additional Python file is obsolete.
All 616 JSON files / 6,395,402 bytes remain byte-identical to main.
The eleven identical-byte groups retain phase/digest-bound evidence paths.
The Rust core/WASM package allowlists retain 68/6 members and exclude repository
Python, tools and JSON archives. All five workspace members inherit root Cargo
dependencies. No dependency, registry publication, codec format or release
performance approval is introduced.


## Complete native controllers and Python retirement

This revision removes all 34 remaining Python files, including their old test and
fixture entry points. The 252 behavior tests passed against the Rust models before
removal. Current capture, source-copy observation, validation, original-byte replay,
CPU handshakes, VM/MSR collection, CI JSON and tests are Rust. The source inventory
and CI/Shell guard enforce the interpreter-free boundary. Historical JSON remains
byte-identical; controller filenames recorded in archived data remain evidence.

The implementation shares file/process owners and typed models. Current
controllers call validators in-process. Complete release verification uses the
same regression report functions as the public Rust gates, eliminating sixteen
Cargo subprocesses. Request construction transfers Value ownership, and original
arrays serialize directly without an intermediate JSON clone. These are tooling
structure changes, with no codec throughput claim.

Generated and patched inputs form a whole restoration transaction. Unknown
changes, extra files, symlinks or an unreaped worker preserve the private inputs.
Both ordinary completion and cancellation share this rule. Original CRLF bytes
are restored exactly. CPU measurement threads pin only themselves after the
construction-budget handshake; the parent validates the owned thread and complete
epoch without changing an external TID. MSR reads preserve the original ordered
TSC bracketing and reject short reads.

A build/runtime source stamp prevents stale executables from claiming current
controller or template bytes. Source changes also invalidate a requested passing
release report. Explicit matching source roots support relocation. Adjacent
analysis reuses one bounded original snapshot; explicitly trusted frozen Rust
models build from a private immutable Git checkout. The coordinator records its
own identity separately. No legacy interpreter is executed.

Formal and diagnostic schemas remain separate. A complete diagnostic retains
statistical failures and never grants release approval. Missing supplements are
refused before source observation, all medians remain derived from original
samples, candidate tree/lock rebases retain their rules, and source/compiler
observations are checked again before returning success. CI failure journals
remain available even when the native tool cannot be built: the fallback contains
only fixed negative data and returns failure.

Native tests reuse full campaign/layout factories and cover hashes, source
mutation, numeric equality, schema/approval refusal, interrupted ownership,
modified inputs, frozen transport, ordered/short MSR reads, CLI grammar, strict
release shell gates and stale-code rejection. Final source identities, local/CI
results and package inventories are recorded with the delivery commit below.
Codec source and formats are outside this migration. New/old data compatibility
continues to use exact parity and bidirectional cross-decoding against published
9.0.0; the older inconsistent fast-one-parity mode stays outside that probe.


Delivery validation on 2026-10-24 uses runtime `e7b4b40`, rebased onto
main `544bb06`. Its controller, template, CI, codec, manifest and lockfile bytes
match the independently tested `b86f3fc` runtime on main `727690f`; the final
rebase imports only documentation and two upstream evidence JSON files.
The documentation-only verification commit follows this runtime.

The earlier clean `c521300` controller snapshot passes all 24 checks, including
published-baseline/current sampler input replay, page-controlled layouts, CPU and
Slot worker tests, and real stream views against both sources. The source-copy
checks execute and replay all 28 ordinary-verify workers, 30 stream-owner workers
and 516 dynamic workers. The last group retains 6,192 observations and 1,488,099
events. A real-path check caught a nonportable OsStr JSON representation; the
shared declaration now writes relative string member names and has a regression
test. Candidate-before-baseline dynamic execution remains explicit. Failed
attempts are retained separately from the passing captures.

All 17 post-rebase checks pass in a separate clean checkout: 301 native tests
(262 unit, 29 existing CLI and 10 controller CLI), 557 codec tests, ten doctests,
strict host/Linux tooling Clippy, codec-only no_std, Linux codec/test and generated
CPU-worker compilation, all four repository audits, actual Cargo packaging and
389 extracted-package library tests, formatting and diff checks. The Linux C test
dependency uses the installed Zig cross compiler; these are compile checks, not
Linux runtime or privileged MSR collection. Existing ignored-test boundaries are
retained. The 200 non-fast parity and bidirectional cross-decoding configurations,
plus historical tower-field vectors, pass again after the GFNI main update.

The final inventory has zero Python files and no interpreter dependency in CI or
Shell entry points. All 620 JSON files / 6,556,786 bytes match the updated main;
the eleven identical-byte groups retain their provenance-bound paths. Core/WASM
allowlists contain 71/6 package members and exclude repository tools and archived
evidence. All five workspace members inherit the root dependency catalog.
The branch changes no codec source, format, public API, dependency version or
lockfile relative to that main. Local checks do not imply CI completion, VM/MSR
runtime validation, registry upload or release-performance approval.
