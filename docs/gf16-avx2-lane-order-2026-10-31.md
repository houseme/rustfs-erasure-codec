# GF16 AVX2 lane-order simplification

The AVX2 fixed-multiplier core previously reordered byte planes after `packus`
and reordered the output again after `unpack`. The nibble tables are broadcast
identically to both 128-bit lanes, so the packed per-lane order can be kept
through the shuffles. `unpacklo` then yields the first contiguous 16 elements
and `unpackhi` the next 16. This applies to the existing AVX2 multiply and
forward/inverse butterfly instantiations; the SSSE3, NEON, scalar and `no_std`
paths are unchanged. On the observed Rust 1.99.1 x86 cross-build, each AVX2
instantiation lost two `vpermq` and one `vperm2i128` instructions. Generated
code is compiler- and target-dependent.

The native AVX2 test compares multiply and multiply-XOR against scalar output
for every 16-bit field element and representative multipliers. Existing native
butterfly tests cover every non-sentinel multiplier and both directions. The
complete native all-features suite, GF16 `no_std` library tests, 200 old/new
cross-decoding configurations and 200 published-to-candidate non-fast cases
passed. AArch64 GF16 tests, x86 `no_std` compilation and x86 all-features
Clippy passed independently. The production computation is identical to the
measured prototype; the test module name was changed afterward.

## Performance screen

The screen ran on an AMD EPYC 9V45 Linux x86-64 virtual machine using Rust
stable 1.99.1, `simd-accel`, identical dependency locks and four Rayon workers.
The baseline was the existing AVX2 butterfly implementation at `8659458`;
the candidate changed only the lane-order computation and added a test. All
96 benchmark cases passed functional checks on both sides. A shared lock and
an exclusive guest CPU partition covering CPUs 8–15 were held; periodic checks
reported no competing heavy process, and the lock and partition were released.
Physical-host isolation was not established.

Each A1/B1/B2/A2 phase completed 96 cases with ten samples per case, 0.2 seconds
warmup and 0.6 seconds requested measurement time. The
[screen](../benchmarks/gf16-avx2-lane-order/2026-10-31-linux-x86/screen.json)
retains every estimate and raw sample. Per case, the table averages the two
Criterion mean estimates on each side and computes `(candidate / baseline - 1)
* 100`. Each row reports the median and range across eight cases: 10+4 and 16+8,
each with one through four missing shards. Negative values mean lower latency.

| GF16 recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 64 KiB | −11.78% | −15.31% to −9.46% |
| Retained slots | 1 MiB | −8.55% | −15.55% to −6.56% |
| Generic Option | 64 KiB | −10.85% | −12.81% to −6.17% |
| Generic Option | 1 MiB | −6.35% | −11.48% to +5.15% |
| Optimized Option | 64 KiB | −10.45% | −13.18% to −8.39% |
| Optimized Option | 1 MiB | −4.58% | −13.90% to +7.15% |

Three potentially slower 1 MiB cases were repeated separately with 30 samples,
one second warmup and eight seconds requested measurement per case. Their
four-phase mean changes were −0.37%, −2.21% and −7.68%; the
[focused samples](../benchmarks/gf16-avx2-lane-order/2026-10-31-linux-x86/focused.json)
retain all four phases. GF8 control-group screen medians ranged from +0.70% to
+2.21%. The user requested that drift not stop the run. These are short
virtual-machine screens, not a formal release qualification or a guarantee for
every CPU and workload. Option and retained-slot entrypoints have different
preparation costs and should each be compared with their own baseline.
