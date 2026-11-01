# GF16 no_std small-lane scalar butterfly evaluation

Two experimental scalar butterfly implementations were evaluated against
`bd71deb`. Neither is included in the codec: both substantially slowed the
64-byte encoding cases that they were intended to improve. The existing
128-byte AVX2 butterfly cutoff and scalar fallback remain unchanged.

The candidates apply only to little-endian x86-64 `no_std` builds with AVX2
explicitly enabled, for equal-length lanes below the existing cutoff. Both fuse
the multiply-XOR and second XOR into a single lane traversal. The first
[processes one element at a time](../benchmarks/gf16-nostd-small-scalar-fusion/2026-11-01/scalar.patch);
the second [processes four elements per iteration](../benchmarks/gf16-nostd-small-scalar-fusion/2026-11-01/unrolled.patch)
and handles the remainder separately. The modulus sentinel retains its
original path. These are experiment patches, not production changes.

## Encoding results

The baseline and each candidate were compiled at the same source path in
release mode with Rust stable, locked offline dependencies,
`--no-default-features`, and `RUSTFLAGS="-C target-feature=+avx2"`. The
[encoding harness](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/nostd-encode-probe.rs)
checks parity before and after timing. For each candidate, the run used an
A1/B1/B2/A2 sequence with 20 Criterion samples, 0.5 seconds of warmup and a
requested two-second measurement per case. Guest CPUs were partitioned under a
shared reservation, no competing CPU-intensive process was observed, and the
partition was restored. Physical-host isolation was not verified.

Changes compare the average of the two candidate means with the average of the
two baseline means; positive means slower. The baseline drift is A2 relative to
A1 and was recorded without gating the run. [Full Criterion estimates](../benchmarks/gf16-nostd-small-scalar-fusion/2026-11-01/encode-estimates.json)
include all four stages.

| Candidate | Configuration | Shard size | Latency change | Baseline drift |
| --- | --- | ---: | ---: | ---: |
| One element | 10+4 | 64 B | +17.38% | +2.34% |
| One element | 16+8 | 64 B | +26.00% | +0.61% |
| Four elements | 10+4 | 64 B | +18.09% | −1.37% |
| Four elements | 16+8 | 64 B | +22.14% | −1.59% |
| One element | 10+4 | 1 MiB | −0.03% | −0.13% |
| One element | 16+8 | 1 MiB | −0.41% | +0.34% |
| Four elements | 10+4 | 1 MiB | −1.49% | −0.17% |
| Four elements | 16+8 | 1 MiB | −1.79% | −1.68% |

These results are specific to the measured host and workload. The large-shard
changes are small and do not establish a benefit. Although loss of independent
operations was a plausible explanation for the first result, the four-element
variant also regressed; the cause was not established by profiling.

Each experimental build passed 51 GF16-filtered library tests on native x86-64
with `no_std +avx2`, including existing multiplier and boundary checks. This was
an early performance screen, not a complete compatibility validation. Recovery
performance and old/new codec compatibility were not measured for these
rejected candidates.
