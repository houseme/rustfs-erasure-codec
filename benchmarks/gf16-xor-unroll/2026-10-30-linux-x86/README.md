# GF16 AVX2 XOR unroll screen

This experiment is **not included in the production codec**. A short native
x86-64 profile of the optimized GF16 `10+4`, 1 MiB, four-missing-shard Option
workload showed samples in `slice_xor_u16_avx2`. A separate diagnostic build
with frame pointers attributed some of those samples to GF16 reconstruction.
The profile covers the whole benchmark process, including fixture preparation
and allocation; its percentages are not exclusive decoder timings.

The [prototype](prototype.patch) replaces the existing AVX2 XOR loop with a
128-byte loop and preserves the 32-byte and scalar tails. Release x86 assembly
showed four vector XOR operations in the main loop instead of two. The native
AVX2 boundary test covered lengths 0–513, 1,024 and 32,768 elements with offset
buffers; 54 GF16 library tests and all 96 benchmark functional cases passed.
The same `Cargo.lock`, benchmark harness, stable toolchain and feature flags
were used for both builds.

The [measurement data](measurements.json) contains all 96 cases in each of
A1/B1/B2/A2, with ten raw samples and Criterion estimates per case. The
comparison ran on an AMD EPYC 9V45 Linux x86-64 virtual machine using four
Rayon workers and an exclusive guest CPU partition. The controller reported no
competing heavy process in its periodic checks, and the shared lock and CPU
partition were released afterward. Physical-host isolation was not verified.
Warmup was 0.2 seconds and requested measurement time 0.6 seconds per case;
drift did not stop the run at the user's request.

For each case, average the two Criterion mean estimates on each side, then
compute `(prototype / baseline - 1) * 100`. Each row below reports the median
and range for eight cases: 10+4 and 16+8, with one through four missing shards.
Negative values mean lower latency.

| GF16 recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 64 KiB | +0.36% | −7.38% to +2.73% |
| Retained slots | 1 MiB | −0.49% | −2.61% to +1.48% |
| Generic Option | 64 KiB | +0.56% | −7.56% to +4.25% |
| Generic Option | 1 MiB | −2.28% | −4.72% to +1.29% |
| Optimized Option | 64 KiB | −0.34% | −4.00% to +4.09% |
| Optimized Option | 1 MiB | −0.18% | −3.12% to +4.36% |

The observed changes are small and mixed across cases, so this screen does not
support adding the wider loop to production. This is a short virtual-machine
screen, not proof that every workload or CPU has zero benefit. Keep the
existing implementation unless a more targeted experiment establishes a useful
and reproducible improvement without regressions.
