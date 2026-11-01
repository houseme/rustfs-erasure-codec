# GF16 no_std small encoding plan evaluation

Five experimental changes to GF16 encoding plan storage were evaluated against
the production source at `dbb6e67`. None is included in the codec: the most
promising small-block result also enlarged shared decode-plan structures and
showed a large-block recovery regression, while narrower candidates did not
show consistent enough latency improvements to justify their complexity.

The earlier [phase probe](gf16-nostd-avx2-layout-2026-11-01.md) identified
plan construction as a visible fixed cost for 64-byte shards. A separate
[allocation probe](../benchmarks/gf16-nostd-small-plans/2026-11-01/alloc-probe.rs)
confirmed that 10+4 and 16+8 encoding each requested six allocations per call,
including the work buffer. The probe verifies encoded parity and counts calls
and requested bytes only during the encode; these are not peak-memory values.
The complete [allocation observations](../benchmarks/gf16-nostd-small-plans/2026-11-01/allocation-results.txt)
include larger shard counts.

| Candidate | Change | 10+4, 64 B allocations / requested bytes | 1024+4, 64 B allocations / requested bytes |
| --- | --- | ---: | ---: |
| Baseline | Heap-backed plans | 6 / 1,312 | 266 / 94,400 |
| [All inline](../benchmarks/gf16-nostd-small-plans/2026-11-01/all-inline.patch) | Inline small shared FFT/IFFT blocks and later encoding plans | 1 / 512 | 10 / 110,736 |
| [Outer only](../benchmarks/gf16-nostd-small-plans/2026-11-01/outer-only.patch) | Inline one later encoding plan | 5 / 896 | 267 / 94,608 |
| [Encode IFFT](../benchmarks/gf16-nostd-small-plans/2026-11-01/encode-ifft.patch) | Inline encoding IFFT blocks and later plans; retain decode plan types | 2 / 608 | 11 / 110,832 |
| [Exact capacity](../benchmarks/gf16-nostd-small-plans/2026-11-01/exact-capacity.patch) | Reserve the actual number of later plans | 6 / 1,000 | 260 / 68,088 |
| [Shift capacity](../benchmarks/gf16-nostd-small-plans/2026-11-01/shift-capacity.patch) | Equivalent exact capacity using the power-of-two plan size | not separately measured | not separately measured |

On native AArch64 `no_std`, all-inline increased the size of `FftDit16Plan`
from 56 to 184 bytes and `IfftDit16Plan` from 104 to 184 bytes. In the
1024+4 allocation probe, it reduced allocation calls substantially but
increased requested heap bytes. These are observations for the tested shapes,
not bounds on peak memory or every configuration.

## End-to-end performance

The baseline and candidate binaries were built at the same source path with
Rust stable, locked dependencies, `--no-default-features`, and
`RUSTFLAGS="-C target-feature=+avx2"`. Each comparison ran on an x86-64 Linux VM
in an exclusive guest CPU partition, with A1/B1/B2/A2 phases, 20 Criterion
samples, 0.5-second warmup and a requested two seconds of measurement per case.
The shared reservation was restored and no competing intensive workload was
observed. Physical-host isolation was not verified. Drift was recorded but did
not gate the measurements. Each result compares the average of two candidate
means with the average of two baseline means; positive means slower.

| Candidate | 64 B, 10+4 | 64 B, 16+8 | 4 KiB range, both shapes | 1 MiB range, both shapes |
| --- | ---: | ---: | ---: | ---: |
| All inline | −6.21% | −2.81% | −1.34% to −0.29% | +0.76% to +0.80% |
| Outer only | −2.81% | −0.95% | +1.19% to +1.40% | −0.94% to +0.38% |
| Encode IFFT | −3.20% | +0.66% | +1.91% to +1.99% | −0.33% to +0.80% |
| Exact capacity | −0.06% | −1.35% | −0.78% to +0.49% | −0.38% to +2.12% |
| Shift capacity | +1.12% | +0.24% | +0.11% to +0.38% | −0.43% to +0.65% |

A further exact-capacity comparison for 128+4 and 1024+4 ranged from −1.18%
to −0.31% at 64 B and −0.90% to +0.21% at 4 KiB. The all-inline recovery
screen covered 10+4 and 16+8, one and four missing shards, at 64 B, 4 KiB and
1 MiB. Its 1 MiB range was +0.13% to +2.31%; the +2.31% case had −0.13%
baseline drift. Individual stage estimates, including baseline drift, are in
the [raw data](../benchmarks/gf16-nostd-small-plans/2026-11-01/estimates.json).
These screens do not establish a universal regression or improvement across
machines, nor do they compare the candidates directly against each other.

Each candidate passed 52 GF16-filtered tests on native Linux x86 with AVX2;
the all-inline test build emitted an unused test-import warning. The all-inline
and encode-IFFT candidates also passed 286 no_std library tests on AArch64.
These were early screens, not complete cross-version or cross-platform release
validation. Patches have zero context and can be checked against `dbb6e67` with
`git apply --unidiff-zero --check` in a disposable checkout. The allocation
probe is a standalone `harness = false` benchmark built with the same flags.
