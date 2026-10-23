# GF16 GFNI tower-plane fusion

Frozen implementation: `07b08c8c9b65127b30e8340358955b68e121b941`.
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
