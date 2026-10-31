# GF16 GFNI matrix construction

The GFNI butterfly constructs four 8-by-8 binary matrices for each field
multiplier. The previous code wrote each matrix bit individually. The new code
packs the 16 basis products into four column matrices, transposes each matrix
with three fixed bit-swap stages, and reverses the rows into GFNI order. This
reuses the same GF16 multiplication tables and does not change the field or
encoded bytes.

With the faster construction, the GFNI butterfly is used for every lane already
eligible for the existing AVX2 butterfly: at least 32 u16 elements (64 bytes).
GFNI and AVX2 CPU detection and the already-selected compatible GF8 backend
remain required. GF16 alone still does not initialize the GF8 backend. Scalar,
other SIMD and `no_std` paths retain their existing dispatch and arithmetic.

A hardware-independent x86 unit test compares the transposed matrices with the
original bit-by-bit reference for every non-sentinel multiplier. Native GFNI
tests also cover all multipliers and 16 basis vectors, all field values for
representative multipliers, both butterfly directions, unaligned buffers and
partial vectors. Native full `all-features`, `no_std`, Clippy and release
compatibility tests were run separately. The compatibility run selected the
GFNI backend and checked 200 old/new configurations in both decoding
directions, as well as 200 published-to-candidate non-fast cases. A warm,
single-worker `ShardSlot` probe with four missing shards reported four allocations
of one shard size each on both sides, for 64 B through 1 MiB, with or without
GF8 preselection. The generic path deliberately retains these temporary buffers
to preserve its observable behavior on decode failure or unwind; see the
[publication compatibility review](recovery-direct-and-gf16-fusion-2026-10-28.md).

## Performance screens

The screens used an AMD EPYC 9V45 Linux x86-64 VM, Rust stable 1.99.1,
`simd-accel` and four Rayon workers. A shared lock and guest-exclusive CPUs
8–15 were held during each screen; monitoring reported no competing heavy
process, and the lock and partition were released. Physical-host isolation was
not established. A1/B1/B2/A2 each used ten samples, 0.2 seconds warmup and
0.6 seconds requested measurement per case. For each case, the two Criterion
mean estimates on each side are averaged before computing the percentage
change; each table row reports the median and range over four cases (10+4 and
16+8, with one or four missing shards). Negative means lower latency.

The first three screens determined the cutoff, comparing adjacent versions of
the candidate under a preselected GFNI GF8 backend. For lengths below the
baseline cutoff, the baseline uses the original AVX2 butterfly.

| Screen and comparison | Shard size | Retained slots median (range) | Optimized Option median (range) |
| --- | ---: | ---: | ---: |
| [64-byte vs 512-byte cutoff](../benchmarks/gf16-gfni-matrix-transpose/2026-10-31-linux-x86/cutoff-64.json) | 64 B | −12.03% (−12.83% to −11.26%) | −13.23% (−13.95% to −11.93%) |
| Same | 256 B | −10.90% (−12.56% to −7.88%) | −11.90% (−13.03% to −11.02%) |
| [512-byte vs 2 KiB cutoff](../benchmarks/gf16-gfni-matrix-transpose/2026-10-31-linux-x86/cutoff-512.json) | 512 B | −12.30% (−13.90% to −10.71%) | −12.18% (−12.59% to −11.01%) |
| Same | 1 KiB | −14.70% (−15.96% to −12.33%) | −15.16% (−16.14% to −14.47%) |
| [Transpose vs production](../benchmarks/gf16-gfni-matrix-transpose/2026-10-31-linux-x86/initial.json) | 2 KiB | −18.94% (−20.32% to −17.44%) | −19.57% (−20.63% to −18.55%) |
| Same | 4 KiB | −23.59% (−24.65% to −22.53%) | −23.40% (−24.61% to −22.41%) |
| [Final candidate vs production](../benchmarks/gf16-gfni-matrix-transpose/2026-10-31-linux-x86/large.json) | 64 KiB | −10.54% (−10.95% to −9.03%) | −10.69% (−10.81% to −10.48%) |
| Same | 1 MiB | −7.80% (−10.56% to −2.85%) | −4.26% (−6.52% to −2.03%) |

The first transpose screen also measured 64 KiB medians of −3.22% and −3.26%.
Its harness included GF8 fixture preflights; the final large-shard screen used
GF16-only fixtures. These short screens have different preparation and should
not be combined into a single universal speedup claim. The measured 64-byte
prototype contained an explicit length check already guaranteed by its caller;
the final code removes that redundant condition.

The original context labels for the 64-byte cutoff screen were accidentally
reversed. Its controller stage roles and binary identities confirm that A used
the 512-byte cutoff and B the 64-byte cutoff; the saved sample metadata records
the correction. All four stages completed successfully.

A separate [GF16-only screen without GF8 preselection](../benchmarks/gf16-gfni-matrix-transpose/2026-10-31-linux-x86/unselected.json)
compared the production code with the final candidate over 64 B–1 MiB. The
four-case medians ranged from −1.67% to +0.66%; individual case changes ranged
from −7.05% to +3.11%. All estimates and individual samples are retained in the
linked JSON files. The user requested that baseline drift not stop the runs;
these VM measurements are not a guarantee for other CPUs or workloads.
