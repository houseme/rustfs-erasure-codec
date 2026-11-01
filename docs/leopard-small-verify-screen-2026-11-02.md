# Leopard small verification scratch investigation

Ordinary Leopard verification allocates a vector of parity buffers and one
buffer per parity shard on each call. On warmed AArch64 GF8 and GF16 codecs,
4+2 verification made three allocation requests per call at 64 B, while 10+4
and 16+8 made five and nine. These are calling-thread allocator request counts,
not elapsed time, peak live memory or a guarantee for other allocators.
Caller-supplied buffer and workspace APIs have different scratch lifetimes.

This investigation tested several variants on the runtime source of `7846e24`
(which matches `d9574b7` for the files under test). The
[allocation probe](../benchmarks/leopard-small-verify/2026-11-02/alloc-probe.rs)
and [observed counts](../benchmarks/leopard-small-verify/2026-11-02/allocations.json)
are retained. Encode and verify fixtures were initialized and warmed before
counting. Each allocation candidate kept one `Field::zero()` call per parity
shard; a custom byte field checked the count and valid/corrupt results.

## Rejected contiguous scratch

The [contiguous small-buffer experiment](../benchmarks/leopard-small-verify/2026-11-02/contiguous-rejected.patch)
placed up to 512 byte elements for at most eight parity shards inline and made
zero calling-thread allocation requests for 64 B 10+4 and 16+8 in the measured
warmed cases. Its initial x86 GFNI screen showed 64 B GF16 verification changes
of −18.89% and −29.38% for 10+4 and 16+8, respectively; baseline drift was
−6.05% and −0.05%. These timings do not justify using the candidate because it
changes behavior for a safe external `Field` implementation.

A custom byte field in the [adversarial probe](../benchmarks/leopard-small-verify/2026-11-02/adversarial-probe.rs)
returns `None` for a mutable byte view whose address is near the current stack.
On the measured AArch64 process, existing heap-backed parity buffers yielded
`Ok(true)` for GF8 and GF16, whereas the inline-scratch experiment returned
`Err(UnsupportedCodecFamily)` for both. The field intentionally declines some
views; the codec's checked adapter accepts safe implementations and reports
such failures. The concrete stack-address heuristic is platform-specific, but
the counterexample shows why moving buffers across storage classes cannot be
assumed compatible for external implementations. Results are recorded
[here](../benchmarks/leopard-small-verify/2026-11-02/adversarial-results.json).
A [288-case changing-input-view comparison](../benchmarks/leopard-small-verify/2026-11-02/changing-view-results.json)
also matched results and read counts between the baseline and this prototype;
that check did not detect the separate output-view incompatibility. Its
[probe](../benchmarks/leopard-small-verify/2026-11-02/changing-view-probe.rs)
is retained alongside the results.

## Independent heap buffers with an inline index table

To retain each output buffer's `Vec` representation and its separate heap
allocation, the follow-up changed only the outer table of buffers. A
[direct SmallVec collection](../benchmarks/leopard-small-verify/2026-11-02/outer-collect.patch)
removed one request for up to eight parity shards, but at nine parity shards
its growth strategy requested 168 extra bytes on the measured 64-bit allocator.
An [explicit-capacity variant](../benchmarks/leopard-small-verify/2026-11-02/outer-capacity.patch)
removed that extra request volume. A third
[fixed-array variant](../benchmarks/leopard-small-verify/2026-11-02/outer-array.patch)
retained the exact original path for more than eight parity shards.

The 20-sample GFNI and AVX2 screens used the retained Criterion probes for
[contiguous](../benchmarks/leopard-small-verify/2026-11-02/criterion-initial.rs),
[collection](../benchmarks/leopard-small-verify/2026-11-02/criterion-collect.rs)
and [capacity/array](../benchmarks/leopard-small-verify/2026-11-02/criterion-probe.rs),
rustc 1.99.1, locked dependencies, one worker and a guest CPU partition under
a shared lease on one Linux x86-64 VM. Each comparison ran A1/B1/B2/A2 with
0.5 s warmup and a requested 2 s measurement per case. Percentages compare
mean candidate latency against mean baseline latency; negative means faster.
Physical-host isolation was not verified, and drift is reported without
excluding runs. The [stage estimates](../benchmarks/leopard-small-verify/2026-11-02/estimates.json)
retain completed phases and results.

The collection variant's GFNI 20+9, 64 B GF8 case changed +3.97%, with +0.28%
baseline drift. For the explicit-capacity variant, GFNI 128 B GF16 changed
+4.52% for 10+4 and +3.71% for 16+8, with +2.31% and +0.31% drift. AVX2
64 B GF16 at 4+2 changed +3.38%, with +2.43% drift. Other small cases ranged
from modest improvements to regressions; fewer requests did not consistently
reduce elapsed time.

The fixed-array version also showed mixed results. GFNI 128 B GF16 at 16+8
changed +3.36%, with −2.05% baseline drift; AVX2 128 B GF8 at 4+2 changed
+6.69%, with −4.18% drift. The 20-sample runs are screens rather than a broad
performance guarantee.

A GFNI follow-up used 40 samples, a 1 s warmup and a requested 5 s measurement
per case. For 4+2 GF8, 64 B and 128 B changed −0.71% and +0.16%; for 16+8
and 64 B, GF8 and GF16 changed +1.83% and +2.10%. The 16+8 GF16 128 B case
changed +1.22%. Baseline drift across these five cases was −1.16% to −0.27%.

The corresponding 40-sample `rust-avx2` follow-up changed +2.08% and +3.42%
for 4+2 GF8 at 64 B and 128 B; 16+8 at 64 B changed +2.46% for GF8 and
+3.19% for GF16, while 16+8 GF16 at 128 B changed +2.24%. Baseline drift in
these cases ranged from +0.01% to +5.73%. These results are specific to the
measured VM. The reduced allocation count did not produce a consistent latency
improvement, so none of these variants was retained in production.

## Validation scope

Native x86 and local AArch64 focused tests covered valid and corrupted Leopard
GF8/GF16 parity, changing input views and several small-size boundaries. The
fixed-array candidate also passed a local no-default-features focused test and
a test comparing parity and verification against the published 9.0.0 codec.
These are exploratory checks, not a complete release qualification of any
candidate. Production code and public APIs were not changed.
