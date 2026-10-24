# GF16 GFNI tower-plane fusion

Current scalar-preserving runtime: `e38a8ad591ca183a6c914b6e939963928575b8a5`;
validated final test source: `7fa4fddeea73ef74ee0b52f600f80fd6bc89c066`.
The earlier width prototype is `0a35491` / `986c1fe`.
The earlier combined implementation is `7914ac450856305f40f0c24eea424af2a153905f`.
The final section records its separate correctness review; the three historical
timing cohorts below remain bound to their original sources.

First frozen implementation: `07b08c8c9b65127b30e8340358955b68e121b941`.
Comparator: `9ddcac9348504d1c9b6b55231ec4596a1e938984`, including the separate
GF8 affine-map candidate. The GF16 norm candidate is not included.

## Scope and execution

For a general tower coefficient `cx*X + cc`, the existing extension polynomial
`X^2 + 2*X + 128` gives these GF8 plane products:

| Output coordinate | Input X coefficient | Input constant coefficient |
| --- | --- | --- |
| X | `cc XOR 2*cx` | `cx` |
| constant | `128*cx` | `cc` |

Both implementations derive these coefficients from the same extension polynomial.
The previous general path writes four stack planes, calls four GF8 slice
operations, and packs the result. The candidate groups eight X bytes and eight
constant bytes inside each 128-bit register lane. One affine map supplies the
diagonal products; a second map after swapping the adjacent planes supplies the
cross products. XOR and a byte shuffle restore the original interleaved layout.

The [GFNI intrinsic's documented eight-byte/matrix pairing](https://doc.rust-lang.org/core/arch/x86_64/fn._mm256_gf2p8affine_epi64_epi8.html)
permits different GF8 maps for the two coordinate groups. The existing
field-derived GF8 matrix table is reused; there is no second field definition.
Two maps on a full interleaved vector replace four maps on half-width planes.
This does **not** halve GFNI arithmetic per element. The intended saving is
intermediate plane traffic, repeated slice dispatch and packing/loading the
accumulator, not a claimed twofold arithmetic speedup. A separate Linux x86 compilation
finds all four fused specializations using affine operations without field-multiply
instructions. The encompassing GF16 entry still reserves its existing portable
fallback stack frame in that build; no stack-reservation or RSS reduction is
claimed.

Both selected GFNI widths have checked complete-vector loads/stores, safe scalar
tails, overwrite/XOR variants and unaligned byte storage. Constant coefficients
(`cx == 0`) keep the existing flattened GF8 path. Scalar, other SIMD and no_std
fallback arithmetic retain their existing algorithms. Dispatch consults the
already selected backend, so an explicit scalar/other-backend override cannot
silently enter GFNI. Public APIs, polynomial/element representation, format
identity, parallel policy and recovery publication order are unchanged.

Normal small GF16 Vandermonde matrices can remain inside the GF8 subfield and
therefore use the constant-coordinate path. This change targets general tower
coefficients, including custom matrices; it does not claim to accelerate every
GF16 encoding or the GF8 codec itself.

## Compatibility and actual execution

The portable scalar check covers all 65,536 coefficients and all sixteen input
basis bits. Native Linux positively executes the AVX2 and AVX-512 tower kernels:
all coefficient/basis maps, all 65,536 inputs for four representative general
coefficients, and twenty lengths across vector/chunk boundaries with three
input/output byte-offset pairs for seven selected coefficients. Overwrite/XOR
and buffer sentinels are checked.
Both explicit execution markers are present; a skipped hardware test is not
counted as positive execution.

Published 9.0.0 compatibility includes the existing 200 normal-format cases and
a new [general-coefficient custom-matrix test](../scripts/release-compatibility/tests/tower_slice_compatibility.rs).
For 2+2, 4+2 and 10+2, nine vector-boundary lengths preserve exact parity and
162 old/new cross recoveries across three erasure patterns. Linux repeats those
27 cases under auto, explicit GFNI AVX2 and explicit GFNI AVX-512: 81 custom
configurations / 486 recoveries, with both versions' backend names checked.

On ARM64/stable, 372 codec library, 168 integration and ten documentation tests
pass, with 43 existing ignored; 245 codec-only no_std tests also pass. Strict
codec/compatibility Clippy, Linux x86 library compilation/Clippy, fmt and typos
pass. Cross-compiling all tests needs an unavailable external C cross-compiler;
the retained environment error is separate from the successful native Linux
kernel checks. Actual core/WASM package lists remain 67/6 files.

## Bounded source-change comparison

The [frozen component plan](../benchmarks/release-9.1.0/2026-10-24/gfni-tower-fusion-plan.json)
covers all four existing general-tower cases, explicit GFNI-width counterparts,
four controls and four custom encoding/warmed-recovery cases. Both roles use the
same fixed two-parity custom matrices before measurement. The warmed recovery
case loses two data shards so its inverse actually requires general coefficients.
Existing inner timing loops are preserved; these additional matrix cases are
explicit new component workloads, not replacements for formal release cases.

The plan has 20 cases / 80 phase-major ABBA workers / 960 observations, with
three warmups and nine measurements. Independent power-of-two pilot calibration
on both roles chooses the larger count. Limits remain 10% central width,
5% same-version repeat, 12% regression across all four pairings, and 100 ms
minimum measured batches. All refusals are collected before analysis. Forced
backend names and deterministic payload/expected-output witnesses must match;
those witnesses do not prove matching dynamic or physical layouts.

Pre-sampling native tests, all forty one-iteration payload checks, and rebuilding
the std+simd sampler without metrics have completed. The controlled run launched
once and then retains the complete plan, sources and failures. Generated runner
executables are outside source checkouts; build and run paths were checked to
match before launch. The old GFNI campaign's source-clean failure is unchanged.

The complete [first component result](../benchmarks/release-9.1.0/2026-10-24/gfni-tower-fusion-component-review.json)
retains all twenty cases and eighty cross-version comparisons. All eighty
central-width checks pass and no sample is short, but ten of forty repeats fail
(maximum 15.1672551%). The scalar overwrite control has one above-limit pairing:
A2/B2 is 1.123204 against the original 1.12 limit. Its four comparisons are all
slower (1.084191–1.123204), while its same-version repeats remain within limits.
This is a real refusal and an investigation priority, not proof of a unique cause.
All target and control numbers are retained, including the much lower unqualified
GFNI/custom-workflow latencies; those favorable numbers do not qualify the cohort.

An independent raw replay checks all 2,938 inputs, 1,960 parent events, forty
functional workers, 860 calibration workers and eighty measurement workers. All
reports reproduce, no input changes and no codec is executed during replay.
Three sources and all four cleanup conditions restore, and both original
observer services are active. The component comparison is not accepted.

The selected-scalar path now includes an extra backend probe, and static assembly
still reserves the fallback stack frame before fused dispatch. These concrete
extra mechanisms motivate a follow-through that retains the original portable
executor and selects its implementation once. This is a proposed source change,
not a retroactive explanation or erasure of the first cohort's refusals. This plan is not formal full-release geometry
or 9.0.0 throughput acceptance. CPU/wall batch clocks do not represent exclusive
API cycles, and faults are unmeasured by this harness. Historical full-catalog
refusals and the old affine component remain preserved. Final frozen-source
accepted full evidence and strict positive release preflight are still required;
no tag, publication or automatic PR merge is performed.

## Native lint follow-through

The initial `07b08c8` P0 job rejects the x86-only test callback signature under
strict Clippy `type_complexity`. Follow-through `25c5e5d` gives that callback a
type alias without changing a kernel, test assertion or frozen measurement.
The old failure remains retained. Source review checks the exact test-only
replacement and unchanged runtime items; the new head requires its own CI.

## Cached execution and portable-frame follow-through

Source `678ddd60c28f6a198be58397a0603a5ff1c43fad` retains the same fused vector
kernels and restores the original portable executor body, apart from naming its
existing polynomial constants. Builds without GFNI alias that executor directly.
GFNI-capable builds resolve one field-specific function pointer from the same
immutable GF8 backend, instead of probing/rejecting GFNI on every general call.
The constant-coordinate path remains outside that general dispatch.

A separate static Linux x86 compilation checks the actual `Field::mul_slice` and
`Field::mul_slice_add` entries: each reserves 24 bytes; the portable executor
retains its original 4,096+120-byte frame. Thus the portable planes are no longer
reserved on fused/constant entry paths in that build. The original generic body
and loop operations remain preserved; these source/code-generation facts do not
prove that all observed scalar refusal or drift has one cause.

The new source repeats all 550 local codec/integration/doc tests, 245 no_std
tests, the 200 published configurations and 27 custom / 162 cross-recovery cases.
Strict ARM64 and x86-64 all-target Clippy, Linux library and Linux GFNI/no_std
Clippy, fmt and typos pass. An independent execution of the public wide oracle
from [PR #83](https://github.com/houseme/rustfs-erasure-codec/pull/83) also passes
all twelve 2+255 GF16 Vandermonde/Cauchy cases against published 9.0.0 on ARM64,
including data-only and full cross-recovery. The temporary reference test is
removed afterward; this is a new execution on this runtime, not a relabeling of
that PR's source or measurements. Source `678ddd6` ordinary CI completes
31 SUCCESS / 3 SKIPPED; those skips are not positive strict preflight.

The [new dispatch plan](../benchmarks/release-9.1.0/2026-10-24/gfni-tower-dispatch-plan.json)
retains all twenty cases, identical harness bytes, count calibration and limits.
Its candidate is the actual dispatch/frame change; original baseline remains
`9ddcac9`. The first cohort and initial zero-sample resource wait remain intact.
The new plan launches only after the earlier owner fully restores resources and
the original observers. It requires its own native cached-selector, kernel and
published-version checks before eighty ABBA workers / 960 observations. No
sample is replaced by another source or selected from a favorable phase.

PR #83 independently changes portable accumulation/interleave and overlaps the
thin-entry work. Its different baseline/candidate/catalog cannot validate this
source. That portable optimization is not included here; integrating both needs
an explicit source review, conflict resolution and new final evidence.


Native checks on cached source `678ddd6` subsequently pass as well: nine GFNI
units with both tower markers, the new selector mapping, coefficient/matrix,
encoding dispatch and owned-buffer reuse tests, the existing 200 published cases,
and all 81 custom configurations / 486 cross recoveries under the three backend
settings. Both twenty-case one-iteration payload catalogs agree. The fresh
sampler is rebuilt without metrics before its fixed eighty-worker comparison;
at that checkpoint the comparison had not completed. Its complete refusal
is recorded immediately below.

## Complete cached-dispatch comparison

The [second complete result](../benchmarks/release-9.1.0/2026-10-24/gfni-tower-dispatch-component-review.json)
retains all twenty cases / eighty comparisons and exact independent replay of
all 960 observations. All eighty distributions pass, but nine of forty repeats
and two regression cases refuse the limits; short samples remain zero. The
unchanged GF8 encode control has candidate B1/B2 drift of 17.6002400% and ratios
0.995581–1.210021. Scalar GF16 accumulation has ratios 1.045500–1.147319;
its same-version repeats remain below five percent. Both are real refusals.
The previous scalar overwrite refusal is absent in this cohort, but that is not
proof that one code change uniquely explains the earlier result. All favorable
and unfavorable pairs remain recorded; this cohort is not accepted either.

The second audit binds the externally retained predeclared plan and original
preheat digest, checks all 2,938 input files unchanged, and covers forty functional,
859 calibration and eighty measurement workers / 1,958 parent events. All sources
and four owned cleanup conditions restore; both original observers are active.
A later unrelated controlled owner already holds the shared reservation when
post-run checks occur; its resources are not taken over or reset.

The remaining scalar path still crosses an indirect general-executor pointer.
Keeping only optional native callables in the cache would allow all other
backends to call the preserved portable executor statically, retaining its
compiler optimization boundary. That mechanism warrants a small follow-through,
not a claim that it explains the GF8 control drift or every scalar observation.

## Optional native cache and static fallback

Source `2f16f8b284badb2406e51ad762e1dc1426b86f28` makes the cache optional and
native-only. Non-GFNI choices cache `None` and retain a statically named call to
the original portable executor, instead of making that executor an indirect
function target. The selection test checks all non-GFNI IDs return no native
callable. Empty general operations retain the original no-backend-selection
behavior; constant-coordinate empty operations keep their existing GF8 call.
No new unsafe block or numerical/kernel change is added.

Static Linux x86 compilation confirms two zero-local-stack trait shims forwarding
to a 16-byte general wrapper with a named jump to portable; the portable frame
remains 4,096+120 bytes. A GFNI-build-only no-inline hint prevents that large
frame returning to its caller. Other builds retain the prior inlining policy.
This identifies the call boundary, not a measured cure of every refusal.

The source repeats all 550 local codec/integration/doc and 245 no_std tests,
200 normal published cases, 27 custom configurations / 162 recoveries and twelve
wide published-version cases. Strict ARM64/x86 all-target and Linux library/no_std
Clippy, fmt and typos pass. The wide program is again executed independently on
this source and removed afterward; it is not another implementation of PR #83.

A [third source-bound plan](../benchmarks/release-9.1.0/2026-10-24/gfni-native-only-plan.json)
uses the exact same twenty cases, harness bytes, calibration and thresholds,
with fresh independent source copies. It has launched once after resources
become free. Neither a favorable previous subset nor the previous source's
native checks qualify this source. Native prechecks precede the eighty-worker
ABBA collection; new performance results remain pending. All first and second
cohort refusals remain unchanged, and no further source change occurs during it.


Current source `2f16f8b` subsequently completes its own native prechecks: nine
GFNI units with both new markers, selector, coefficient/matrix, dispatch/reuse,
200 published normal cases and all 81 custom / 486 cross recoveries. Both full
one-iteration input catalogs agree. The sampler is rebuilt without metrics and
is completing its fixed post-build idle before calibration. These checks do not
qualify performance; its complete third result remains pending.

## Final complete native-only comparison

The [complete native-only result](../benchmarks/release-9.1.0/2026-10-24/gfni-native-only-component-review.json)
retains all twenty cases, eighty comparisons and forty repeats. Independent
replay exactly reproduces all 960 observations, eighty distributions and every
refusal. It binds the externally retained plan and original preheat state and
checks all 2,945 input files unchanged: forty functional, 862 calibration and
eighty measurement workers / 1,964 parent events, all exiting successfully.
Three sources and four cleanup conditions restore, the two original observers
are active, and no owned partition remains at the post-run audit. Replay runs
zero codec calls.

| Implementation source | Width refusals / 80 | Repeat refusals / 40 | Regression cases / 20 | Short samples |
| --- | ---: | ---: | ---: | ---: |
| `07b08c8` | 0 | 10 | 1 | 0 |
| `678ddd6` | 0 | 9 | 2 | 0 |
| `2f16f8b` | 0 | 5 | 0 | 0 |

The latest scalar overwrite four-pair ratio range is 0.998285–1.018329;
accumulation is 0.992322–1.030358. Both control repeats pass. The prior scalar
refusals are absent in these conditions; this does not uniquely attribute the
older observations or prove that every remaining drift is a codec defect.
All eighty regression comparisons now remain within the declared 12% limit.

The five retained repeat refusals are:

| Case | Pair | Drift percent |
| --- | --- | ---: |
| auto GF16 multiply, 1024 | A1/A2 | 11.5060171 |
| auto GF16 multiply, 1025 | B1/B2 | 7.5252478 |
| auto GF16 accumulate, 1024 | A1/A2 | 7.1616461 |
| explicit GFNI AVX2 multiply, 1025 | A1/A2 | 5.1382041 |
| explicit GFNI AVX-512 multiply, 1025 | B1/B2 | 11.2434309 |

The entire latest cohort therefore remains **not accepted**. Favorable direct
kernel/custom encode/recovery ratios remain in the complete JSON alongside all
controls; they are not promoted to an approved speedup. No unchanged-source
retry or threshold change is justified by these results. All three
source-bound comparisons and original failures remain intact.

The implementation/compatibility work is reviewable in PR #84. A combined final
candidate still needs integration review with the separate candidates, source/
lock/tool freeze, accepted full ABBA/layout evidence and actual strict positive
release preflight. No release tag, registry upload or automatic merge is recorded.


## Integrated native and portable execution

The earlier documentation head `ce9eda7` completes 31 ordinary CI checks, with
three release steps skipped. That result stays bound to its native-only source;
the combined head requires its own CI, and skips are not strict positive preflight.

Source `7914ac450856305f40f0c24eea424af2a153905f` explicitly integrates the
portable accumulation/layout source `287c9b5` from PR #83, retaining both commit
histories. This is a new combined implementation, not a relabeling of any prior
measurement or a GitHub PR merge. The GF16 norm candidate remains separate.

| Input/backend condition | Execution | Storage |
| --- | --- | --- |
| Constant coordinate `cx == 0` | Existing flattened GF8 multiply/XOR | Caller spans |
| Empty general input | Return after equal-length validation | None |
| Selected GFNI AVX2 / AVX-512 | Cached optional native tower callable | Register planes and checked scalar tail |
| Other general input | Statically named, compile-time overwrite/XOR executor | Four initialized, 64-byte-aligned stack planes, 4 KiB total |

The portable executor computes the same four GF8 products for both modes.
Accumulation XORs them directly into initialized destinations during final
interleave; it no longer deinterleaves the old destination first. Checked AVX2
layout batches cover 32 elements, retaining SSSE3, NEON and scalar paths and
all tails. The arithmetic override continues to select the GF8 backend; it does
not disable feature-selected byte-layout conversion, as with the earlier SSSE3
layout path. No new unsafe block is introduced by resolving the shared entry.
All ten layout functions exactly match the reviewed portable source; its general
executor differs only in naming the existing polynomial constants. The native
kernel file, GF8 module, manifests, lockfile and build configuration exactly match
`2f16f8b`. Core recovery, error/publication order, public APIs and format identity
are unchanged. Mismatched empty/general spans retain panic validation.

The [complete source-bound functional result](../benchmarks/release-9.1.0/2026-10-24/gf16-integrated-execution-functional-review.json)
records twenty native Linux commands, all passing. Actual execution includes both
GFNI widths and the AVX2 layout oracle, plus explicit scalar, SSSE3 and AVX2
fallbacks. The full Linux suite passes 388 library, 165 integration and ten doc
tests, with 43 existing ignored; 249 codec-only no_std tests pass. ARM64 separately
passes 376 library, 168 integration and ten doc tests, with 43 existing ignored,
and 249 no_std tests. Strict ARM64/x86 all-target, Linux library/no_std and
published-compatibility Clippy, fmt and typos pass.

Published 9.0.0 retains exact bytes and bidirectional recovery for the existing
200 normal configurations. Six backend settings each pass 27 custom-matrix
configurations / 162 cross recoveries (162 / 972 total) and twelve wide 2+255
Vandermonde/Cauchy configurations (72 total), including data-only and full
cross-recovery. Every raw command output, required execution marker and positive
summary is checked independently against the frozen plan; all 35 inputs remain
unchanged and replay runs no codec. The first reader's tuple-versus-JSON-list
comparison error remains retained separately from the corrected read-only review.
Both source copies, all four cleanup conditions and original observers restore.

A Linux client compilation explicitly instantiates both public slice entries.
Each has a 24-byte explicit stack subtraction and a statically named general
fallback; the two fallback specializations separately reserve 4 KiB scratch
(with 4,032+256-byte explicit subtractions and 64-byte stack alignment in this
build). This avoids confusing absent inline instantiations with a verified entry.
Assembly is not hardware timing or an RSS measurement. Core/WASM package
membership remains 67/6.

No new timing samples are collected for this integration. The three rejected
component cohorts above remain immutable, including the latest five repeat
refusals; their favorable ratios do not approve the combined source. PR #84
remains a reviewable draft. Final-source/tool/lock binding, accepted complete
ABBA/layout evidence and strict positive release preflight are still outstanding.
No tag, registry upload or automatic PR merge is performed.


## Integrated-source component plan

The [new predeclared plan](../benchmarks/release-9.1.0/2026-10-24/gf16-integrated-component-plan.json)
compares the actual combined runtime `7914ac4` with its native-only predecessor
`2f16f8b`. It retains the earlier twenty cases and adds complete scalar/SSSE3/AVX2
arithmetic-backend counterparts for general overwrite/XOR and custom-matrix
encoding/warmed recovery at 1,024 and 1,025 elements. The full catalog has
42 cases / 168 phase-major ABBA measurement workers / 2,016 observations
(504 warmups and 1,512 measurements), plus all 84 one-iteration payload checks
and both independent power-of-two calibrations.

The private harness bytes, inner operation loops, three warmups/nine samples,
200 ms calibration target, conservative common counts and original 10% width /
5% repeat / 12% regression / 100 ms minimum-batch limits remain unchanged. All
statistical refusals are evaluated after every planned worker; actual execution
errors remain distinct. A complete independent reader is prepared separately
from the controller. The existing exact-source twenty-command Linux correctness
review is bound and reused; it is not rerun or relabeled as timing. Both fresh
measurement binaries must be rebuilt with metrics disabled outside the source
checkouts. Sources, plan, harness and guard are frozen before the single launch.

The backend label identifies GF8 arithmetic, not the portable layout instruction
stream. Payload witnesses do not track dynamic destinations or physical pages.
The warmed-recovery workload retains the codec/inversion cache but drops two
missing Option outputs each iteration and includes allocation/reconstruction/drop;
it is not the separate owned-buffer zero-allocation resource test. Wall/calling-
thread CPU batch windows do not measure exclusive API cycles; faults are
unmeasured by this component harness. No allocator tuning, profiler, hardware
counter or timing normalization is added.

This source-changing comparison preserves every prior rejected cohort. It is a
bounded unpublished-source diagnostic, not complete published-9.0.0 release
geometry or approval. The resource guard requires the previous owner's complete
cleanup, original observers and free resources; an empty cgroup is insufficient.
Actual launch and terminal checks are recorded separately from this immutable
plan. Final accepted full-release evidence and strict positive preflight remain
outstanding; the user's automation stays paused.


The integrated-source plan has launched once after the previous controlled owner
completed and restored its resources. Its sole-owner entry preflight passes with
zero codec observations; the first identity/heartbeat review is in preheat.
At that launch checkpoint no timing row had completed. Frozen sources,
counts, controller and guard remain unchanged while it runs; statistical
refusals must not stop the remaining planned workers.


## Complete integrated-source result

The [complete 42-case result](../benchmarks/release-9.1.0/2026-10-24/gf16-integrated-component-review.json)
retains all 168 cross-source comparisons, 84 repeats and 84 calling-thread
CPU/wall repeat summaries. All 168 workers / 2,016 observations complete with
zero execution errors; 84 payload workers and 1,724 pilot workers also complete.
Independent raw replay checks all 5,874 inputs unchanged and 3,952 parent events,
with zero codec calls. The reader's initial mistaken 4,216-row assertion is
retained; correcting that reader to the declared 2,016 rows reproduces every
original report/refusal without changing or resampling any input. Three sources,
four owned cleanup conditions and both original observers restore.

The entire cohort is **not accepted**: one width refusal, eleven repeat refusals,
five regression cases and zero short samples. These are statistical refusals,
not execution failures. The following table retains all groups and every pair's
range, rather than selecting favorable targets:

| Complete group | Cases | Unqualified candidate/baseline ratio range | Width / repeat / regression refusals |
| --- | ---: | --- | --- |
| native direct controls | 12 | 0.849912–1.225258 | 0 / 8 / 3 |
| gf8 ordinary encode control | 1 | 1.012501–1.138013 | 0 / 1 / 1 |
| gf16 ordinary encode control | 1 | 0.988422–1.014036 | 1 / 0 / 0 |
| portable direct products | 12 | 0.676113–1.240974 | 0 / 2 / 1 |
| native custom codec controls | 4 | 0.974751–1.069480 | 0 / 0 / 0 |
| portable custom codecs | 12 | 0.653988–1.007099 | 0 / 0 / 0 |

The five regression cases include auto tower overwrite at 1,025 elements,
auto accumulation at 1,024, explicit GFNI AVX-512 overwrite at 1,024, the
unchanged-GF8 encode control, and SSSE3 overwrite at 1,025. The unchanged GF8
control has candidate-repeat drift of 11.9521219%; the largest repeat refusal
is 24.3846690%. These counterexamples prevent assigning the entire result to
portable arithmetic or declaring the lower AVX2 codec ratios an accepted gain.
Faults and dynamic/physical layout equivalence remain unmeasured.

The SSSE3 overwrite case is a distinct investigation priority: all four ratios
are 1.144317–1.240974, while all its phase widths pass (maximum 5.3224931%) and
both repeats pass (4.0630596% / 4.0404256%). Calling-thread CPU median drift
tracks its wall drift, without proving a unique physical cause. Source/assembly
review shows capability-only AVX2 layout selection can surround narrower legacy
SSSE3 arithmetic; wide-layout VZEROUPPER sites and legacy shuffle arithmetic
are present. Those sites are not a measured transition count or sole root cause.
A backend-width-coordinated, cached layout choice is therefore a mechanism worth
testing, without shape thresholds or allocator changes; it does not explain
unchanged GF8/native-control drift. Any follow-through needs a new source freeze,
compatibility proof and its own complete comparison. The frozen `7914ac4` run
and all earlier refusals remain intact. Strict release acceptance is withheld.


## Cached arithmetic/layout width coordination

The stable SSSE3 overwrite counterexample motivates runtime `0a35491`: AVX2
layout eligibility requires both CPU capability and a wide immutable arithmetic
backend (AVX2, AVX-512 or GFNI). Scalar/SSSE3 selections keep the existing narrow
layout; SSSE3 may still perform byte conversion with scalar arithmetic, so the
label is not a whole-program ISA profile. The new eligibility choice is resolved once; hot chunks do not query the
backend/environment again. Constant/empty handling, numerical
kernels, tails, initialized planes, public options, core recovery and format
bytes are unchanged. No shape threshold, allocator change or unsafe block is
added. The capability-only policy first fails the new mapping constraint; the
coordinated policy then passes. This tests the policy contract, not an old byte
correctness failure or a proven unique explanation of every timing refusal.

The [complete final-source functional proof](../benchmarks/release-9.1.0/2026-10-24/gf16-layout-width-functional-review.json)
binds `986c1fe`, whose only difference from the runtime source is test-marker
framing. The first source's twenty test commands all exit successfully, but
expected-panic hooks split two dynamic stderr markers and strict admission fails.
That failed capture, source and four successful cleanup checks remain retained.
Preformatting one short record before a single write fixes framing; no numerical
assertion or marker requirement is relaxed. A new independent directory repeats
all twenty commands and raw review, with all 35 inputs unchanged and zero codec
calls during replay. Explicit scalar/SSSE3 selections record narrow layout;
AVX2/GFNI selections record wide layout, alongside positive native kernel markers.

The final Linux source passes 390 library / 165 integration / ten doc tests,
with 43 existing ignored, and 250 no_std tests. ARM64 runtime-source validation
passes 377 / 168 / ten and 250 no_std; strict ARM64/x86 all-target and Linux
library/no_std Clippy pass. The test-only framing follow-through additionally
passes x86 all-target Clippy. Published 9.0.0 compatibility retains the 200 normal
configurations, six-backend custom 162 configurations / 972 cross recoveries,
and 72 wide Vandermonde/Cauchy configurations. These are correctness results,
not performance acceptance.

The [new source-bound plan](../benchmarks/release-9.1.0/2026-10-24/gf16-layout-width-component-plan.json)
compares integrated `7914ac4` against final source `986c1fe` (runtime `0a35491`).
It retains exactly the same 42 cases, harness bytes, 168 ABBA workers, 2,016
observations, conservative calibration and all original limits. The twenty-command
exact-source correctness proof is bound and reused; both fresh binaries must
pass all 84 payload checks without metrics. It has launched once after cleanup;
no frozen source/controller/guard/count is changed and there is no new timing
conclusion yet. The earlier 1/11/5 refusal remains unchanged. Two descriptive
strings in the frozen private plan inherited predecessor backend/comparison
wording; source refs, affected scope, harness and counts correctly bind the new
comparison. This review records that passive metadata correction separately,
without editing the running plan or reinterpreting any sample. Formal-release
geometry and acceptance remain outstanding; automation stays paused.


## Complete width-coordination comparison

The [complete result](../benchmarks/release-9.1.0/2026-10-24/gf16-layout-width-component-review.json)
retains all 42 cases, 168 comparisons and 84 CPU/wall repeat summaries. All 168
workers / 2,016 observations, 84 payload checks and 1,722 pilots complete without
execution errors. Independent replay covers 5,867 unchanged inputs and 3,948
parent events, runs no codec and reproduces every report/refusal. Sources,
owned cleanup and original observers restore. The frozen plan's two inherited
passive descriptions are documented separately; it was never edited in flight.

The complete cohort remains **not accepted**: one width, seventeen repeat and
six regression-case refusals, with zero short samples. All groups are retained:

| Complete group | Cases | Unqualified candidate/baseline ratio range | Width / repeat / regression refusals |
| --- | ---: | --- | --- |
| native direct controls | 12 | 0.768616–1.205871 | 0 / 12 / 5 |
| gf8 ordinary encode control | 1 | 0.952985–0.962082 | 0 / 0 / 0 |
| gf16 ordinary encode control | 1 | 0.938294–0.984470 | 0 / 0 / 0 |
| portable direct products | 12 | 0.873639–1.166869 | 0 / 4 / 1 |
| native custom codec controls | 4 | 0.945220–1.040329 | 0 / 1 / 0 |
| portable custom codecs | 12 | 0.890176–1.074055 | 1 / 0 / 0 |

The former SSSE3 1,025-element overwrite counterexample has four ratios
0.875150–0.921599 versus `7914ac4`, with its own widths/repeats passing. That is
useful bounded evidence for the narrower SSSE3 choice, not whole-cohort or
published-9.0.0 approval. Other controls still drift, and scalar arithmetic is a
counterexample to applying the same choice universally: all eight scalar cases
have every pairing slower (1.008190–1.093431), while all their own widths/repeats
pass. Scalar table arithmetic can retain wide memory permutation independently
of arithmetic width. Restoring that predecessor choice while keeping SSSE3
narrow is a source-level follow-through worth validating; no shape cutoff,
allocator change or explanation of all native-control drift follows from it.
Both complete 42-case cohorts and all original refusals remain immutable.


## Preserve scalar permutation and isolate backend tests

Runtime `e38a8ad` retains wide byte permutation for scalar table arithmetic on
capable CPUs, while preserving the narrower legacy SSSE3 decision and all wide
SIMD paths. The completed eight-case scalar cost is the reason to avoid a
universal width rule; no sample/shape cutoff or allocator tuning is introduced.
The former narrow-scalar mapping fails the new retention constraint before the
single-arm policy change passes. Numerical/layout kernels, constant/empty
behavior, public options, field bytes, core recovery and one-time caching remain
unchanged. Old ratios from different campaigns are not multiplied into a new
cross-version speedup.

The first native full suite on that runtime exposes a separate real test race:
`test_active_backend_metadata` observes a temporary GFNI AVX2 override while
expecting its own GFNI AVX-512 override. The original scoped helper changed a
process-global environment without isolating other parallel tests and removed
rather than preserving prior values. The failed capture and successful cleanup
remain retained. Test-only follow-through `7fa4fdd` installs override environments
before fresh child processes initialize their cached backend. Children check
requested names, IDs and actual cached selections; parent values remain intact.
Positive child test counts prevent a zero-test success. Compile-time backend
availability and unsupported override cases remain tested. Unsupported process
platforms retain the earlier non-spawn path. No runtime production code changes,
thread-count reduction or numerical assertion relaxation is used. This test race
is not an explanation of standalone timing drift.

The [complete final-source functional proof](../benchmarks/release-9.1.0/2026-10-24/gf16-scalar-layout-functional-review.json)
records 21 positive native Linux commands, including twelve fixed parallel
backend rounds of fifteen parent tests each, all required to pass. Independent
review checks all 37 inputs unchanged, every raw output/marker and all twelve
round summaries, with zero codec calls during replay. Full Linux counts remain
390 library / 165 integration / ten doc, 43 existing ignored, and 250 no_std;
ARM64 has 377 / 168 / ten and 250 no_std. Strict broad/narrow target Clippy and
default-std metadata checks pass. Published 9.0.0 exact bytes and bidirectional
recovery pass for 200 normal, six-backend custom 162 / 972 recoveries, and
72 wide configurations. All previous failed functional captures are retained.

The [final scalar-retention plan](../benchmarks/release-9.1.0/2026-10-24/gf16-scalar-layout-component-plan.json)
compares `986c1fe` with final source `7fa4fdd` (runtime `e38a8ad`) using exactly
the same 42 cases / 168 ABBA workers / 2,016 observations, unchanged harness,
calibration and limits. The fresh 21-command proof is bound and reused; all 84
benchmark payload checks remain required. It has launched once after cleanup;
no result is assigned before every planned worker completes and independent raw
replay. Every older refusal remains unchanged. This is unpublished-source
component geometry, not full published-version release acceptance. Final-source
accepted ABBA/layout evidence and strict positive preflight remain outstanding.
