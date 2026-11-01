# GF16 paired butterfly investigation

Small GF16 recovery and encoding spend a substantial portion of CPU time in
butterflies. A radix-4 step performs two independent radix-2 butterflies with
the same `log_m02` coefficient. The existing GFNI matrix cache makes the second
lookup inexpensive but does not eliminate it. This investigation tested whether
sharing matrix preparation across those two butterflies improves small shards.
None of the prototypes was retained in production code.

All comparisons used runtime source `d9574b7`, which is identical in these
runtime files to `87de22b`. The prototypes preserve the modulus sentinel's
XOR-only semantics and fall back when the GFNI backend is not already selected;
they do not initialize the GF8 backend. The candidate was selected through a
preselected `rust-gfni-avx2` GF8 backend. Tests used rustc 1.99.1 and locked
dependencies on a Linux x86-64 VM, one benchmark worker and a guest CPU
partition under a shared lease. The partition was restored after each run;
physical-host isolation was not verified.

Each completed run used A1/B1/B2/A2 order, 20 Criterion samples, a 0.5 s warmup
and a requested 2 s measurement per case. Percentages compare the mean of the
two candidate phases with the mean of the two baseline phases: negative means
lower latency. Baseline drift is A2 / A1 − 1 and is reported rather than used
as an exclusion rule. [Complete stage estimates](../benchmarks/gf16-paired-butterflies/2026-11-02/estimates.json)
are retained. The existing [encoding probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/encode-only-probe.rs)
and [recovery probe](../benchmarks/gf16-fused-threshold/2026-11-02/boundary-probe.rs)
were used; cold and warm recovery labels describe reconstruction-plan reuse,
not a newly created thread.

## Prototypes and measurements

- The [outlined helper](../benchmarks/gf16-paired-butterflies/2026-11-02/outlined.patch)
  shared matrices across two calls but changed 64 B encoding by +8.26% for
  10+4 and +10.55% for 16+8; baseline drift was −0.82% and −1.32%. The
  compiled candidate contained a separate `run_prepared` function where the
  baseline had a single butterfly function. This is a code-generation observation,
  not proof that the extra call alone caused the regression.
- The [macro-expanded version](../benchmarks/gf16-paired-butterflies/2026-11-02/macro.patch)
  avoided that separate function, but 64 B encoding changed +14.63% and
  +36.35%, with −1.36% and −1.49% drift. The change in structure therefore did
  not establish a useful optimization.
- The [64 B-specific paired loop](../benchmarks/gf16-paired-butterflies/2026-11-02/short.patch)
  kept the original single-butterfly kernel and shared setup only for two
  32-element lanes. Its 64 B encoding changed +1.33% and +3.95%, with +1.23%
  and −2.93% drift. For 10+4 and four missing shards, 64 B recovery changed
  +4.16% cold and +2.35% warm, with −0.24% and +0.61% drift. The 16+8 cold
  and warm changes were −1.00% and −4.91%; the latter had +8.45% drift. The
  mixed recovery results and regression in 10+4 do not justify the added
  dispatch and kernel.

The 1 MiB encoding controls for these versions ranged from −5.43% to +2.27%;
they are included in the estimates but do not establish a general large-shard
improvement. The first two recovery screens and the third non-GFNI screen were
stopped after completed control measurements had already disqualified the
candidate; partial stages are not included in the retained estimates.

Each candidate passed a native x86 GFNI comparison against scalar butterflies
for both directions and several coefficients. The short-loop candidate was
checked at its 32-element boundary; the other two also covered tails and larger
lengths. The existing subprocess test verified that GF16 recovery does not
initialize the GF8 backend. These exploratory tests are not a full release
compatibility assessment. No production code, public API, or encoded layout was
changed.
