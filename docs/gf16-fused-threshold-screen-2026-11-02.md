# GF16 fused layout threshold investigation

GF16 recovery of short shards still spends CPU time preparing multiplication
tables. The existing x86 GFNI layout conversion fuses multiplication with the
split-layout conversion for buffers of at least 4 KiB. Because the GFNI matrix
cache is now bounded and thread-local, this investigation reexamined that
threshold and several alternatives. No public interface or byte layout was
changed by the experiments.

All percentages below are changes in mean elapsed time: negative is faster.
Each comparison used A1/B1/B2/A2 order, with the candidate mean computed as
(B1 + B2) / 2 and the baseline mean as (A1 + A2) / 2. Baseline drift is
A2 / A1 − 1. Except for the explicitly identified follow-up, runs used 20
Criterion samples, a 0.5 s warmup and a requested 2 s measurement per case.
The warm and cold labels describe reconstruction plan reuse; they do not mean
a fresh operating-system thread or a cold thread-local matrix cache.

The tests ran with rustc 1.99.1, Cargo 1.99.1 and locked dependencies on one Linux x86-64 VM,
using one benchmark worker and a guest CPU partition under a shared lease. The
partition was restored after each run. Physical-host isolation was not verified;
baseline drift is reported rather than used as an exclusion rule. Selecting the
`rust-avx2` GF8 backend prevents GFNI eligibility for this layout path; other
GF16 operations may still dispatch according to detected CPU features. Results
are specific to this VM.

## 64 B alternatives

These screens used runtime source `84e5717` and three recovery cases: 10+4 with
four missing shards, and 16+8 with one or four missing shards. Enabling GFNI
conversion for both input and output at 64 B improved warm recovery by
9.16%–13.83%, but 16+8 with four missing shards regressed 10.43% in the cold
plan case, with −0.38% baseline drift. Applying the threshold only to the input
also regressed that cold case by 8.23%, with +0.73% drift. Applying it only to
output had no consistent improvement: cold cases ranged from −2.56% to +0.33%
and warm cases from +0.15% to +1.90%.

A cache-hit-only GFNI variant regressed warm recovery by 8.38%–16.04%. A
32-entry per-thread nibble-table cache regressed the same warm cases by
6.16%–10.34%, and 64 B encoding for 10+4 and 16+8 by 18.78% and 34.91%.
Neither extra cache was retained. Their exploratory patches and complete
[stage estimates](../benchmarks/gf16-fused-threshold/2026-11-02/estimates.json)
are available for inspection. Recovery probes for the [mid-size screen](../benchmarks/gf16-fused-threshold/2026-11-02/mid-size-probe.rs)
and [threshold boundary](../benchmarks/gf16-fused-threshold/2026-11-02/boundary-probe.rs)
are retained with the data. Encoding used the existing
[encoding probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/encode-only-probe.rs).

An additional screen at 256 B and 1 KiB found that the 16+8 cold plan case at
256 B regressed 8.28%, whereas the four 1 KiB cases improved 2.38%–10.53%.
One of these 1 KiB cases had +6.46% baseline drift, motivating a dedicated
1 KiB threshold experiment rather than a general 64 B threshold.

## 1 KiB threshold

With source `d9574b7`, lowering both GFNI conversion thresholds to 1 KiB
improved GFNI 10+4 recovery by 12.39% cold and 6.42% warm at 1 KiB; 16+8
improved 1.63% cold and 10.43% warm. These used four missing shards. The same
change with `rust-avx2` selected regressed the four 1 KiB cases by 2.18%–3.05%;
its 16+8 1 MiB cold case regressed 5.10% with +0.03% baseline drift. On an
earlier `24da58e` source snapshot, a 40-sample, 1 s warmup, 5 s requested
measurement follow-up showed essentially flat 1 KiB non-GFNI results but a
3.23% regression for the same 16+8 1 MiB cold case, with −0.32% drift. This
spread across snapshots and cases limits any claim that the change is neutral
on non-GFNI workloads.

Reordering the backend eligibility check did not resolve the issue: the
`d9574b7` non-GFNI 1 KiB cases still regressed 2.11%–3.13%, even though the
GFNI 1 KiB cases improved 2.50%–14.61%. The reordering also changes an existing
hot helper used outside layout conversion. This candidate was not retained.
The [threshold patch](../benchmarks/gf16-fused-threshold/2026-11-02/threshold1024.patch)
and [backend-check patch](../benchmarks/gf16-fused-threshold/2026-11-02/backend-check-reorder.patch)
record the tested implementation rather than adding it to production.

## 2 KiB threshold

A narrower [2 KiB threshold](../benchmarks/gf16-fused-threshold/2026-11-02/threshold2048.patch)
was screened on `d9574b7` without changing backend eligibility. In the initial
GFNI run, the four 2 KiB recovery cases improved 3.85%–11.85%, while 1 KiB
cases changed +1.20%–+4.38% and 1 MiB cases +0.17%–+1.06%. The largest drift
in the 1 KiB cases was +3.01%. With `rust-avx2` selected, 1 KiB and 2 KiB
cases ranged from −1.61% to +0.80%, and 1 MiB cases from −2.52% to −1.49%;
the latter had baseline drift up to +3.93%.

A separate [fresh-thread probe](../benchmarks/gf16-fused-threshold/2026-11-02/cold-thread-probe.rs)
prewarmed the reconstruction plan and then measured the first recovery on
300 newly created threads per shape and size. Each thread also performed 16
subsequent recoveries. It checked every recovered shard against the encoded
source; thread creation and fixture cloning were outside the timer. The GF8
backend was selected before the threads were spawned, but the new thread's
GFNI matrix cache was initially empty. The [sample data](../benchmarks/gf16-fused-threshold/2026-11-02/threshold2048-cold-thread.json)
contains elapsed nanoseconds for the first and subsequent 16 calls. Comparing
the average of the candidate stage medians against the average of the baseline
stage medians, 2 KiB first-call changes were −1.14% and +0.89%, and subsequent
calls improved 4.02% and 4.64% for 10+4 and 16+8. At 1 KiB, subsequent calls
regressed 3.37% and 2.82%, with baseline drift of −0.44% and −0.56%.

The initial GFNI encoding control ranged from −1.54% to +6.02% at 64 B and
+0.26% to +3.89% at 1 MiB. In the +6.02% case, baseline drift was −0.50%.
These are control-path observations, not an assertion that the layout kernel is
called during encoding.

A 40-sample follow-up with 1 s warmup and 5 s requested measurement confirmed
the tradeoff: the four 2 KiB GFNI cases improved 5.24%–10.73%, while the four
1 KiB cases regressed 2.65%–6.58%. Baseline drift for those 1 KiB cases ranged
from −2.06% to −0.17%. The 1 KiB calls do not enter the new GFNI branch; the
results establish an observable effect in this compiled benchmark, not a
specific underlying compiler mechanism. Because both sizes matter, the 2 KiB
threshold was not retained. The corresponding 40-sample `rust-avx2` follow-up
ranged from −1.56% to +0.39%, with baseline drift from −0.92% to +1.02%.

## Validation and scope

The exploratory 2 KiB candidate, with [tests around the boundary and final chunk](../benchmarks/gf16-fused-threshold/2026-11-02/threshold2048-tests.patch),
passed the all-feature workspace suite excluding the fuzz package and Clippy on
AArch64, and 291 codec library tests with default features disabled. All-feature
cross-checks compiled for x86-64 Linux and Windows; they were not native Windows
execution. These checks do not substitute for the native x86 GFNI correctness and
release compatibility checks required before shipping a threshold change. No
production code from this investigation was retained, so the public API,
encoded bytes and current dispatch behavior remain unchanged.
