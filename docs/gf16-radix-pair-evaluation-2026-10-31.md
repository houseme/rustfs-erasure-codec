# GF16 paired-butterfly and basis-log evaluation

This is an experimental performance and compatibility record for baseline
`bf04fc4`. None of the prototypes in this document is active production code.
The patches in the accompanying benchmark directory are independent alternatives
against that baseline; they are not intended to be applied on top of one another.

A radix-4 operation applies the same multiplier to two independent lane pairs.
The first prototype builds the AVX2 nibble table or GFNI affine matrices once
and reuses the result for those pairs. On native x86, direct paired-butterfly
tests compared both directions and every non-sentinel multiplier with scalar
arithmetic under explicitly selected AVX2 and GFNI backends. GF16 recovery
preflights passed for both backends. These tests are functional evidence, not
a performance qualification or a complete release-compatibility run.

## Performance screens

All completed x86 screens used an AMD EPYC 9V45 Linux VM, Rust stable 1.99.1,
identical dependency locks and four Rayon workers. A shared lock and exclusive
guest CPUs 8–15 were held; monitoring reported no competing heavy process during
the completed screens, and the partition and lock were released. Physical-host
isolation was not established. A1/B1/B2/A2 used ten samples, 0.2 seconds warmup
and 0.6 seconds requested measurement per case, except for the longer follow-up
noted below. Per case the two Criterion means on each side are averaged before
calculating the percentage change; rows show the median and range across four
cases, covering 10+4 and 16+8 with one or four missing shards. Negative means
lower latency.

| Prototype and backend | Entrypoint | Shard size | Median | Range |
| --- | --- | ---: | ---: | ---: |
| [Unbounded pair, GFNI](../benchmarks/gf16-radix-pair/2026-10-31/pair-unbounded-gfni.json) | retained slots | 64 B | −9.20% | −11.09% to −8.29% |
| Same | retained slots | 64 KiB | +2.82% | +0.42% to +5.83% |
| [2 KiB cutoff, GFNI](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-gfni.json) | retained slots | 64 B | −10.94% | −11.20% to −9.99% |
| Same | retained slots | 1 MiB | +2.38% | −0.91% to +4.94% |
| [Cutoff, GFNI longer follow-up](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-gfni-focused.json) | retained slots | 1 MiB | +0.71% | +0.32% to +2.95% |
| Same | optimized Option | 1 MiB | +0.80% | −1.36% to +1.49% |
| [Stage-level dispatch, GFNI](../benchmarks/gf16-radix-pair/2026-10-31/pair-stage-gfni.json) | retained slots | 64 KiB | +20.50% | +18.68% to +22.93% |
| [AVX2-only pair, GFNI fallback](../benchmarks/gf16-radix-pair/2026-10-31/pair-avx-only-gfni.json) | retained slots | 64 B | +1.80% | +1.51% to +2.10% |
| [AVX2-only pair, AVX2](../benchmarks/gf16-radix-pair/2026-10-31/pair-avx-only-avx2.json) | retained slots | 64 B | −7.25% | −7.80% to −7.00% |

The longer GFNI follow-up used 20 samples, 0.5 seconds warmup and two seconds
requested measurement. The other completed records include the [unbounded AVX2
screen](../benchmarks/gf16-radix-pair/2026-10-31/pair-unbounded-avx2.json),
[cutoff AVX2 screen](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-avx2.json),
[cutoff without GF8 preselection](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-unselected.json),
[stage AVX2 screen](../benchmarks/gf16-radix-pair/2026-10-31/pair-stage-avx2.json),
and cutoff encode screens for [AVX2](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-encode-avx2.json)
and [GFNI](../benchmarks/gf16-radix-pair/2026-10-31/pair-limited-encode-gfni.json).
The cutoff encode screens showed small-shard gains; they do not resolve the
large-shard recovery tradeoff. Raw Criterion samples and estimates are retained.
At the user's request, baseline drift did not stop the screens. Results are
workload-specific and do not guarantee performance on other CPUs.

The unbounded and stage variants were rejected because of large GFNI regressions.
The cutoff variant improves small shards, but its 1 MiB GFNI and unselected
recovery screens showed small positive median latency changes. The AVX2-only
variant improves small AVX2 shards while slowing the GFNI fallback. Given the
requested compatibility and high-performance boundary, no variant was promoted.

Two AVX2 encode attempts were stopped by the resource monitor after competing
CPU activity appeared; their samples are excluded. A later idle-server retry
completed. One stage-dispatch run stopped at an ambiguous process-list check
immediately after the candidate had emitted all 32 results. Its samples are
excluded. The temporary controller was amended to accept an empty cgroup only
when the child had actually exited, while continuing to reject any unexpected
nonempty process list; the subsequent stage screens completed.

## Basis-log candidate

The separate [basis-log prototype](../benchmarks/gf16-radix-pair/2026-10-31/basis-log.patch)
precomputes the 16 fixed basis logarithms in the GFNI table state, adding 32
bytes to that internal state and avoiding scattered log-table reads during
matrix construction. Native GFNI tests compared all multipliers and basis
vectors with scalar arithmetic; the matrix-reference, unaligned-tail and
full-field-value tests also passed. AArch64 GF16 tests and an x86 all-features
library Clippy cross-check passed.

Its first isolated performance attempt was stopped when other CPU-intensive
processes appeared. Resources were restored, and that incomplete run is not
used for a performance claim. The server was being used by another task, so no
additional benchmark was started. The candidate remains unmerged until an idle
host can validate performance, allocation behavior, old/new byte compatibility,
and the complete feature matrix.
