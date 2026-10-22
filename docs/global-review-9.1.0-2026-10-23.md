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
