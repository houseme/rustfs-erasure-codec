# Scalar unit-pivot follow-through

Reviewed baseline: `8d59e774dd4b3f75eebbd48afdcc2e05d6614d07`.
Executed implementation: `9e5ee94eadded05946cd1352e691e37082178d2d`.
This candidate reduces known scalar products in Classic matrix setup, used by
construction and cold reconstruction. It is not a claim about steady-state
encoding throughput or accepted release performance.

## Arithmetic and compatibility boundary

Upstream already omits scalar zero prefixes and retains complete GF8 batch rows.
This follow-through keeps that executor and its GF8 bounds. At a normalized pivot
`p`, the source element is one and the destination coefficient is `a`; the pivot
update is `add(a, mul(a, one)) = add(a, a)`. The field addition remains invoked,
while the multiplication by one is omitted. When a nonzero scalar pivot `d` is
normalized by `div(one, d)`, its diagonal is one; only its following elements
need products. The byte-field normalization and batch spans remain unchanged.

Pivot search/order, row swaps, disjoint borrows, matrix storage and cache policy
retain their existing boundaries. Invalid/singular inverses return the same
errors. Generic reconstruction still decodes before publication and preserves
late errors and selective outputs. There is no new scratch, unsafe code, backend
selection, shape cutoff, sampling-loop change or allocator policy.

## Deterministic work and vector counterexamples

A thread-local field spy uses the same 2x2 matrix and independently checks its
inverse using the determinant formula. Counters are test-only, without a timer.

| Fixed fixture | Baseline | Candidate |
| --- | --- | --- |
| Scalar-path field products | 10 | 7 |
| Byte-path normalization products | 4 | 4 |
| Byte-path batch widths | `[4, 4]` | `[4, 4]` |
| Byte 32x32 inverse batch span | 64 | 64 |

The scalar regression fails on unchanged baseline arithmetic and passes on the
candidate; the owned local test overlay is restored exactly. Existing independent
GF8/GF16 scalar algebra oracles cover inverse identities, row swaps, errors and
156 multiple-right-hand-side systems. The original first prototype shortened byte
batches, but a new full-width counterexample rejects it: existing GFNI AVX-512
rounds its input down to 64-byte chunks and executes the remainder as scalar
work. Keeping complete GF8 spans avoids that source-level discontinuity. This
counterexample does not claim measured GFNI cycles or a new throughput ratio.

The earlier all-field/byte-normalization prototypes retain their own logs and
source labels; their work counts are not relabeled as this narrower candidate.
Upstream's two complete local timing cohorts and every refusal remain in the
[scalar-prefix review](matrix-scalar-prefix-review-2026-10-23.md). No favorable
case or operation count replaces their rejected performance classification.

## Source observations

The nine generated observer members and all byte bindings are independently
compared. Reversing only the intended matrix production input delta makes all
nine outputs identical to baseline. The two source-inclusive snapshot expectations
are updated afterward; observer edits and validation bounds are retained.

## Validation and release status

On unchanged source `9e5ee94`: 364 all-feature library tests and 166 integration
tests pass (530 total / 43 existing ignored), as do 239 codec-only no_std tests,
16 debug matrix tests, 221 native tests (192 unit / 29 CLI) and all 247 Python
tests. The real published-9.0.0 comparison passes all 200 normal-format parity
configurations and cross-decoding in both directions, plus historical GF16 vectors.
Strict codec/tool Clippy, Linux codec-library and all-target tool Clippy, and
formatting pass. Inputs remain unchanged throughout the twelve-step validation.

The [complete functional record](../benchmarks/release-9.1.0/2026-10-23/scalar-unit-pivot-functional-review.json)
keeps the work counters, successful checks, source and platform limits explicit.
Later documentation commits do not relabel the executed source.

A local x86_64 Linux all-target codec check on the earlier merged prototype was
blocked by the missing cross C compiler for a test dependency. The original
failure remains retained. Library-only Linux codec Clippy and all-target Linux
tool Clippy are separate compilation checks; native Linux CI must supply the
full-target result. Local ARM64 checks do not establish x86 ISA execution,
virtualized counter fidelity or accepted Linux performance.

The running complete diagnostic remains frozen at earlier `9f935ee` versus
published 9.0.0. This candidate is not deployed into its reservation. Final source
and lockfile freeze, accepted complete ABBA/paired layouts and strict positive
release preflight remain required. The performance approval stays withheld;
keep this follow-through in draft until the applicable review and evidence close.
No tag, registry upload or automatic merge is performed.
