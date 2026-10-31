# GF16 x86 nibble table construction

For a fixed field multiplier, multiplication distributes over XOR. On
little-endian standard-library x86-64, each of the four nibble positions now
computes the four basis products and expands the 16 entries by XOR. The old
implementation computed each entry through the log/exp tables. The expansion
body is shared and deliberately not inlined: an unscoped prototype that
expanded the body repeatedly produced a confirmed 18%–19% regression for
64 KiB GFNI recovery. A first attempt to mark the whole builder `inline(never)`
did not remove that regression. The shared expansion restores the large-shard
results while retaining the small-shard benefit. The old implementation remains
in use for AArch64, `no_std`, and other targets.

The tables are stack values; no public API, field polynomial, backend selection,
or error/publication semantics change. The existing generic `ShardSlot` path
still stages outputs to preserve its observable failure and unwind behavior.
A warm single-worker allocation probe with four missing shards reported four
shard-sized allocations on both baseline and candidate, across 64 B–1 MiB,
with and without GF8 backend preselection.

A unit test compares all 64 table entries against the direct formula for all
65,536 multipliers. Native x86 full `all-features`, GF16 `no_std`, Clippy,
and separate `std,simd-ssse3` and `std,simd-avx2` feature builds passed. The
feature builds ran on an AVX2-capable host; a physical SSSE3-only CPU was not
exercised. AArch64 GF16 tests passed with its unchanged table construction.
With the new codec explicitly selecting `RustGfniAvx2`, the release
compatibility tool passed 200 parity and bidirectional cross-decoding
configurations plus 200 published-to-candidate non-fast cases.

## Performance screens

The x86 screens compared `9b45ecb` with the scoped candidate on an AMD EPYC
9V45 Linux VM using Rust stable 1.99.1, identical dependency locks and four
Rayon workers. They selected either `rust-avx2` or `rust-gfni-avx2` before
measurement. A shared lock and exclusive guest CPUs 8–15 were held; monitoring
reported no competing heavy process, and the lock and partition were released.
Physical-host isolation was not established.

A1/B1/B2/A2 each used ten samples, 0.2 seconds warmup and 0.6 seconds
requested measurement per case. For each case, the two Criterion mean
estimates on each side were averaged before calculating the percentage change.
The recovery rows below show the median and range over four cases: 10+4 and
16+8, each with one or four missing shards. Negative values mean lower latency.

| Backend and entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| AVX2 retained slots | 64 B | −36.18% | −38.73% to −32.57% |
| AVX2 optimized Option | 64 B | −35.77% | −39.64% to −32.03% |
| AVX2 retained slots | 2 KiB | −12.68% | −13.15% to −11.76% |
| AVX2 optimized Option | 2 KiB | −12.15% | −12.66% to −11.34% |
| GFNI retained slots | 64 B | −9.12% | −10.52% to −6.29% |
| GFNI optimized Option | 64 B | −10.35% | −11.76% to −8.16% |
| GFNI retained slots | 2 KiB | −3.05% | −5.67% to −2.81% |
| GFNI optimized Option | 2 KiB | −3.81% | −5.43% to −2.91% |

At 64 KiB, the two AVX2 recovery medians were −1.55% and −1.45%, and the two
GFNI medians were −0.53% and −0.64%. At 1 MiB, they ranged from −0.40% to
+0.79% across backend and entrypoint medians. All cases, ranges and samples are
retained in the [AVX2](../benchmarks/gf16-nibble-basis/2026-10-31/recovery-avx2.json)
and [GFNI](../benchmarks/gf16-nibble-basis/2026-10-31/recovery-gfni.json)
records.

The [encode benchmark source](../benchmarks/gf16-nibble-basis/2026-10-31/encode_bench.rs)
was copied unchanged into the same benchmark target on both sides. AVX2 encode
medians across the two geometries were −35.80% at 64 B, −10.60% at 2 KiB,
+0.02% at 64 KiB and −1.33% at 1 MiB. Initial GFNI encode medians were +1.92%,
−0.26%, +1.60% and +1.14% respectively; individual 1 MiB changes ranged from
−3.74% to +6.02%. A [longer GFNI follow-up](../benchmarks/gf16-nibble-basis/2026-10-31/encode-gfni-focused.json)
used 20 samples, 0.5 seconds warmup and two seconds requested measurement,
and measured medians of −0.87% at 64 B, −0.60% at 64 KiB and −0.40% at 1 MiB.
The [initial AVX2](../benchmarks/gf16-nibble-basis/2026-10-31/encode-avx2.json)
and [GFNI](../benchmarks/gf16-nibble-basis/2026-10-31/encode-gfni.json)
samples are also retained.

The [rejected unscoped x86 prototype](../benchmarks/gf16-nibble-basis/2026-10-31/rejected-inline-gfni.json)
was retested with the longer sampling settings and was slower for 64 KiB GFNI
recovery. An unscoped AArch64 prototype also produced uncertain 1 MiB results
in an [exploratory local screen](../benchmarks/gf16-nibble-basis/2026-10-31/arm-exploratory.json);
that screen had no established CPU isolation and substantial baseline change.
The final implementation retains the prior AArch64 code. At the user's request,
baseline drift did not stop these screens. These short VM measurements do not
guarantee performance on other CPUs or workloads.
