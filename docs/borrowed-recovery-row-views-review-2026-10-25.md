# Borrowed coefficient rows in Classic bulk recovery

## Linear rebase onto main (2026-10-25)

PR #109 is rebased from merge head `5f74fc3d8e70f1fc3887e87a5f5e9961d822b110`
onto main `50f3e82af9bc7d43458edb8d9d39432cd625bbfe`. The original history
contained a main merge; the resulting branch is linear. Three prepended release
record conflicts retain both sides. The merge-only native integration metadata
and shared-path review are restored exactly before adding this rebase record.
At that point every tracked working-tree file matches the pre-rebase PR tree.

Codec, tests, Rust tooling, workflows, build/config/package inputs, WASM and
scripts remain byte-identical to immutable implementation `4bd1208`. All archived
benchmark/native records retain their pre-rebase bytes. The running controlled
capture keeps its original source/tool/lock binding; this task performs no remote
campaign action or source relabeling.

Fresh stable 1.99.1 checks on ARM64 pass:

- 401 all-feature library tests (five existing ignored) and all 23 public recovery
  regressions, covering borrowed coefficient rows, selective absence, validation
  order and resource contracts.
- All 93 workspace and 108 selective cases reproduce the exact original candidate
  request and requested-byte counts: all 201 records match the preserved archive.
- Codec-only no_std: 271 library tests.
- Nine source-plan and four observer tests, including current source bindings,
  original-statement preservation and unsupported/ambiguous-anchor rejection.
- Release/workspace/package audits (71 core / 6 WASM members), JSON audit
  (633 files, zero errors), formatting, spelling and diff checks.

The previously recorded complete suites, native Linux proof and artifact/consumer
checks keep their original executed-source labels. New-head CI is read separately;
these local checks establish no new controlled timing or release approval. The
existing capture, startup failures and outstanding performance/preflight gates
remain source-bound.

## Prior source-bound review

Runtime source `4bd12089c9a1b9aa1f136de6473e3821f9d9b9d8` is based on main
`0225f5c850a47d46ae5b77ee06deeda863391ec3`. The
[complete resource result](../benchmarks/release-9.1.0/2026-10-25/borrowed-recovery-row-views.json)
retains every one of the 201 before/after controls, including unchanged cases.

## Shared execution

Classic bulk recovery previously collected separate data/parity coefficient-row
reference tables. Direct selective parity then concatenated those tables. These
references duplicate information already owned by the validated recovery plan.

A bounded borrowed range now implements the existing static `MatrixRows` access
contract. Data and parity stages borrow their ordered ranges; direct parity uses
the complete range. There is no temporary row-reference array, trait object,
coefficient copy, new public API or new unsafe code. The inverse invariant is
checked before destination initialization. The plan continues to retain its own
inverse, so cache eviction cannot invalidate borrowed rows.

Field arithmetic, native eligibility, scheduling decisions, chunk boundaries,
input/output buffers and stage order remain unchanged. The generic fallible
`ReconstructShard` executor is untouched: per-output late failures and publication
retain their existing behavior. This change targets Option<Vec<u8>> bulk and
workspace execution; stream/generic iterators keep their previous paths.

## Complete resource controls

The original implementation fails the new resource budget after collecting the
complete catalogue. Final-source all-feature execution reproduces every focused
release result, including exact recovered bytes and selective absence.

| Catalogue | Cases | Fewer requests | Unchanged requests |
| --- | ---: | ---: | ---: |
| Prepared public workspace, small and wide layouts | 93 | 45 | 48 |
| Selective suffix targets around 32/64-row boundaries | 108 | 78 | 30 |
| Total | 201 | 123 | 78 |

These are Classic GF8 public APIs, three matrix modes and 65-byte shards. The
workspace control excludes plan/cache construction and policy-environment
queries from its window. Both groups include requested destination-buffer
allocation. Selective checks retain all missing, unrequested outputs as None.
Every final case requests no more calls or bytes than main: improvements remove
one to three requests and up to 4,096 requested bytes. Allocator request bytes
include the full new size of realloc, and are neither retained memory nor RSS.
All 459 earlier encoding/target/survivor-view resource contracts also remain
unchanged in the full combined suite.

A packed 64-entry table was considered and rejected: nine overflow controls
requested 1,024 extra bytes despite nonincreasing call counts. The complete
counterexamples remain in the result. The implemented plan view avoids that
tradeoff and removes row-table storage for every width.

## Compatibility and distribution

Fresh stable validation on the frozen source passes 580 codec parent tests
(401 library, 169 integration and ten doc tests), six separately counted filtered
child executions and 43 existing ignored tests. Codec-only no_std passes 271;
Rust tooling passes 317. Strict codec/tool Clippy, fmt, typos and diff pass.
x86 Darwin all-target and x86 Linux library Clippy are compilation only.

Published 9.0.0 local-default-backend checks pass exact encoding and bidirectional
recovery for 200 normal configurations, 27 custom / 162 cross recoveries,
12 wide GF16 and 270 selective / 2,640 cross recoveries. Fresh release WASM build,
types and 14 Node cases pass with zero skipped (12 actual-module cases, one
result-code contract and one facade unit).

An actual offline core crate has 71 members and clean VCS identity `4bd1208`.
All 68 original source members match the commit exactly; generated Cargo metadata
is distinguished. Its extracted library passes 401 all-feature and 271 no_std
tests. No registry upload or tag is created.

## Observer and release boundary

All 9 dynamic / 4 stream / 3 verify generated members retain valid source
bindings. The only generated change is the complete production reconstruct delta.
The new recovery-plan helper is passed through byte-exact outside those overlay
members, and the complete dynamic overlay compiles in an independent source copy.
The freshly rebuilt tool repeats the generated report. Two tool-test changes
only update exact current-production snapshot assertions; formal sampler inputs,
loops, counts and thresholds are unchanged.

Independent replay checks 45 unchanged sealed inputs, every resource result,
raw test summaries, generated bindings and all original crate bytes with zero
codec calls. The initial zero-test filter and the rejected packed-table prototype
remain preserved separately. No partial or favorable subset replaces a catalogue.

These initial local results establish functional and allocation behavior on this
source. The later Linux execution below adds native compatibility evidence;
controlled throughput, accepted full evidence and release qualification remain
separate.

## Manual Linux compatibility and full validation

The same immutable `4bd1208` implementation passes all 40 native checks and all
44 validation-control regressions with zero skipped. Complete Linux execution
passes 596 codec parent tests, with six filtered child executions counted
separately and 43 existing ignored tests. Its native catalogue contains exactly
639 entries (596 executed parents plus 43 ignored).

Both GFNI widths, direct AVX2 bytes, scalar entry contracts, inverse/cache oracles,
271 no_std tests and all 201 allocator request/byte controls execute positively.
Published-9.0.0 exact encoding and cross-decoding pass the full normal/custom,
wide GF16 and selective catalogues across automatic, scalar, SSSE3, AVX2 and both
GFNI backend conditions. The [complete native check result](../benchmarks/release-9.1.0/2026-10-25/borrowed-recovery-row-views-native.json)
retains all 40 outcomes and their scope.

An independent replay checks every byte of all 167 archived inputs without codec
work, confirms every resource row against the local contract, and verifies the
source/compiler and original resource/service state. The matching ACK is read
back atomically; the native lease closes and its owner exits.

After that gate, the explicitly authorized one-shot full diagnostic actually
starts on `4bd1208` against published 9.0.0. It uses separate frozen tool/candidate
copies with matching complete trees and locks, all 126 cases, phase-major ABBA,
adaptive calibration and complete paired layouts. Protocol 15 keeps its original
loops, three warmups/nine samples, duration/count rules and thresholds. Every
statistical refusal is retained while remaining work continues. At this record's
startup check the job is preheating; there is no completed timing result. A finite
archive verifier can only read and acknowledge this job; it cannot start another
workload. Scheduled follow-up remains paused.

The previous documentation head `f99a22f` has its own 31 successful CI checks and
three actual skips. Those skips do not establish positive release preflight.
Native compatibility, CI and job startup do not establish throughput or RSS
acceptance. Complete accepted evidence, final source/tool/lock binding and actual
strict positive release preflight remain outstanding. Performance/release
approval is withheld; no automatic PR merge, tag or registry upload occurs.

## Preserved startup errors and safe continuation

The initial startup record above is followed by two actual execution failures,
each before full sampling. These are neither statistical refusals nor codec
performance results.

The first private prewarm adapter treated the renderer's logical names as
repository-relative paths. Cargo therefore could not discover the sampling
integration target. Both-source correctness had passed; all four cleanup checks
passed. An independent replay verifies all 81 archived files, ACK and lease
closure. The correction uses exactly the frozen native `Overlay::generated`
mapping into the integration-test entry and support directory. Both actual
source targets then compile in a local no-run check with exact source restoration.

The next controller unit gate deliberately orphaned a short-lived fixture child.
The outer strict guard correctly refused that detached process. No source copies,
partitions or capture existed. Its unverified fence stayed in place until a
narrow recovery of the same failed job verified the original services/CPU state,
unchanged inputs and all 41 original archived files. Independent ACK precedes
closure of that same token; original failure records remain unchanged.

The owner now uses process-local Linux child adoption, so its own descendants
stay attached to their actual top owner. The strict and legacy guards are
byte-unchanged; unrelated work is still refused. All 57 Linux controller checks
pass with zero skipped, including adoption and parent-flag isolation controls.
Completed adopted children are reaped only after tracked commands are quiescent.
A new, separately frozen continuation has actually launched and is preheating.
It keeps implementation `4bd1208`, the same full catalogue, protocol, counts,
loops and thresholds. No completed timing result or release acceptance is
available. Scheduled follow-up remains paused; no unchanged measured campaign
was rerun to select favorable results.

## Main integration and shared-path review

Remote main `50f3e82` includes the externally merged source/CI documentation from
PR #108. This integration retains those records and the newer borrowed-row
results together; all three conflicts concern prepended release-document blocks.
Tracked codec, tests, Rust tooling, workflows, package manifests, build/config,
WASM, scripts and benchmark source remain byte-exact to `4bd1208`. The complete
Git tree changes, so the ongoing Linux capture still retains its original
`4bd1208` tool/candidate label and is not relabeled as this merge.

The shared-path review checks row ordering and empty/data/parity boundaries,
workspace effective-format validation, inverse lifetime across cache eviction,
selective absence and generic late-error publication. The borrowed rows remain
bounded by their plan and encoding-matrix lifetimes. The GFNI row kernel accepts
at most four outputs, matching its inline metadata capacity; every returned
validation error precedes output writes. No additional supported runtime defect
or measured bottleneck is identified. No shape threshold, allocator tuning or
extra runtime patch is added while full evidence is pending.

Before integration PR #109 was reported as conflicting and its latest
record-only head had no CI results. New-head CI is checked independently after
push. Documentation JSON/links/spelling/diff are rechecked; unchanged Rust
suites retain their executed source bindings instead of being rerun for records.
