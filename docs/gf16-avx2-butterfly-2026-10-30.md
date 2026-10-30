# GF16 AVX2 butterfly fusion

The GF16 forward butterfly previously multiplied one lane into the other and
then traversed both lanes again for XOR; the inverse butterfly did the XOR
first and then multiplied. The AVX2 path now performs both operations while the
vectors are loaded, sharing its nibble-multiplication core with the existing
multiply kernels. The modulus sentinel still uses its existing XOR shortcut;
short lanes, non-AVX2 x86, other architectures and `no_std` retain their
existing paths. The safe dispatch refuses mismatched lane lengths before the
raw-pointer kernel.

The native x86 prototype passed an independent scalar comparison for every
non-sentinel multiplier, both butterfly directions, boundary lengths and
unaligned lane offsets. The AVX2 check confirms that these tests execute on
supported hosts and skip on unsupported hosts. The x86 release all-features
library and integration tests passed; the no-`std` GF16 tests and the 9.0.0
compatibility runner also passed, including 200 cross-decoding configurations
and 200 non-fast published-to-candidate cases. AArch64 all-features and no-`std`
GF16 tests passed separately. The production computational code is identical to
the final measured prototype; comments and the exhaustive test were added
afterward and the resulting source was checked and tested on native x86.

## Performance screen

On 2026-10-30 the current codec at `600d990` was compared with the safe fused
prototype on native Linux x86-64, an AMD EPYC 9V45 virtual machine. Both builds
used Rust 1.99.1, identical dependency locks and benchmark harnesses,
`simd-accel`, and four Rayon workers. A shared lock and an exclusive guest CPU
partition covering CPUs 8–15 were held while A1/B1/B2/A2 were captured. Periodic
checks reported no competing heavy process; the partition and lock were
confirmed released. Physical-host isolation was not established.

All 96 benchmark cases completed in every phase, with ten samples per case,
0.2 seconds warmup and 0.6 seconds requested measurement time. The
[raw estimates and samples](../benchmarks/gf16-avx2-butterfly/2026-10-30-linux-x86/measurements.json)
are retained. For each case, the table averages the two Criterion **mean**
estimates on each side and computes `(prototype / baseline - 1) * 100`; each
row gives the median and range of eight cases (10+4 and 16+8, each with one
through four missing shards). Negative values mean lower latency.

| Family and recovery entrypoint | Shard size | Median change | Range |
| --- | ---: | ---: | ---: |
| GF16 retained slots | 64 KiB | −5.89% | −7.83% to −2.88% |
| GF16 retained slots | 1 MiB | −4.01% | −5.64% to −0.68% |
| GF16 generic Option | 64 KiB | −6.43% | −10.16% to −2.10% |
| GF16 generic Option | 1 MiB | −3.99% | −7.64% to −0.23% |
| GF16 optimized Option | 64 KiB | −5.91% | −8.57% to −2.11% |
| GF16 optimized Option | 1 MiB | −5.72% | −7.64% to −1.16% |

The GF8 control-group medians ranged from −1.07% to +1.49%. This short screen
was run at the user's request without rejecting results on drift; it shows the
observed distribution, not a formal release qualification or proof of a
performance guarantee on every machine. Setup and preflight are outside the
reported Criterion operation but the Option and retained-slot entrypoints have
different preparation costs, so compare each entrypoint with its own baseline.
