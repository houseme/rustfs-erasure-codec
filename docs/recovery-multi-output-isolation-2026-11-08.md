# Multi-output recovery isolation and backend coverage

This diagnostic compares published 9.0.0 source `9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6` with 9.1.0 source `11d7d03ca040cb9cfd218b917eba62e22b630858`. It does not approve the 9.1.0 release or replace the 126-case and paired-layout release gates. The compared revisions differ in more than one respect, so the differences below cannot independently identify a causal code change.

## Method and controls

On a Linux x86-64 virtual machine with an AMD EPYC 9V45 processor, binaries used Rust 1.99.1, locked dependencies, `simd-accel`, one worker, and the same checked-in probe source. The probe checks the requested active backend and verifies recovered bytes before and after measuring. For each case, missing indices start at 0 and advance by 2; the selective case removes four shards and restores indices 2 and 6. The timer includes reconstruction and excludes clearing recovered `Option` values. Each stage reports 15 samples of at least 300 ms of wall time, with eight warmup iterations; results are the mean reconstruction time per iteration within a sample.

Stages run A1, B1, B2, A2. The diagnostic stability gate requires both within-version median drifts to be at most 5% and each stage's central spread `(second largest - second smallest) / median` to be at most 10%. Same-source calibration runs before the complete version comparison, rather than interleaved with each case. The Linux guest reserved eight CPUs and pinned the benchmark to one, checked its CPU partition and monitored competing CPU activity; physical-host isolation was not established. Scheduling counters cover the entire sample loop, including setup and counter overhead, and cannot establish physical-host scheduling behavior.

The [first calibration](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-isolation-11d-01/) failed one of 30 cells: scalar, 32+16, 1 MiB, two missing data shards, with 24.46% central spread in B1. It correctly stopped before the version comparison. A separately recorded [second diagnostic](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-isolation-telemetry-11d-02/) used a different reserved CPU and added Linux thread scheduling counters; all 30 same-source and all 30 version-comparison cells passed the diagnostic gate. The maximum drift and central spread were 4.03% and 6.33% in calibration, and 4.56% and 8.78% in comparison. Changing both CPU and instrumentation does not identify the reason for the first failure. Both records are retained.

## Version comparison

The following table contains every measured version-comparison cell. Values are `(B1 median + B2 median) / (A1 median + A2 median) - 1`: positive means 9.1.0 took longer. This is a bounded diagnostic on one virtualized processor model, not a general estimate or a confidence interval.

| Backend | 10+4, 64 KiB, 2 data | 32+16, 64 KiB, 2 data | 10+4, 1 MiB, 2 data | 32+16, 1 MiB, 1 data | 32+16, 1 MiB, 2 data | 32+16, 1 MiB, 4 missing, 2 selected |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| scalar-rust | +3.51% | +4.21% | +4.58% | +4.52% | +5.34% | +1.12% |
| rust-avx2 | +0.37% | +1.91% | +3.01% | -0.35% | +39.71% | +1.21% |
| rust-avx512 | -3.98% | +6.45% | +11.43% | +0.19% | +53.28% | +3.68% |
| rust-gfni-avx2 | -9.08% | -7.03% | +2.62% | -9.97% | +21.96% | -7.39% |
| rust-gfni-avx512 | -0.20% | +8.85% | +17.10% | -2.11% | +51.34% | +1.62% |

At 32+16 and 1 MiB, recovering two data shards is slower on each measured backend, including non-GFNI backends. This isolates a workload worth investigating, not a GFNI-specific cause. The previously observed generic single-destination work structure and the GFNI multi-row cutoff screens are described in the [earlier investigation](diagnostic-recovery-hotspot-followup-2026-11-08.md). A direct same-version API comparison would still mix several implementation differences and cannot on its own establish the effect of a dispatch branch.

## Same-version API comparison attempt

A further [independent calibration](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-api-crossover-11d-03/) prepared an ABBA comparison between generic `reconstruct_data` and serial `reconstruct_data_opt` in source `11d7d03c`. The intended matrix used both 10+4 and 32+16 at 1 MiB and all five x86 backends, with missing indices `[0, 2]`. The source, fixture, backend assertions, timer and stability limits were the same as the telemetry experiment above; both variants were to use concrete `Option<Vec<u8>>` inputs.

The mandatory same-source generic calibration failed one of 10 cells: AVX2, 32+16, with 7.04% A1/A2 median drift, although the reservation and competing-process monitor reported no violation. Consequently the runner stopped before comparing the two APIs. The partition was restored and all 40 calibration stages are retained. An unobserved physical-host effect or another cause remains possible; the available counters do not establish a cause. Do not select the nine passing calibration cells or interpret this attempt as evidence for either API.

## Completed same-version API comparison

A subsequent independent [same-version run](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-api-crossover-11d-04/) repeated the entire intended matrix. Both sides used the same freshly built `11d7d03c` binary, `Option<Vec<u8>>` inputs, one worker, 1 MiB shards and missing indices `[0, 2]`. A is generic `reconstruct_data`; B is `reconstruct_data_opt`. The probe checked reconstructed bytes, the active backend, source and binary consistency, and the guest partition; it used the same sample and stability rules described above. `opt` and generic differ in more than just dispatch or loop ordering, so this comparison does not by itself isolate either change.

All 10 same-API calibration cells passed before any API comparison. All 10 API-comparison cells then passed; maximum drift and central spread were 3.55% and 5.26% in calibration, and 3.67% and 5.26% in comparison. The guest monitor recorded no sustained competing process and the partition was restored. Physical-host exclusivity and other processor models remain unverified. The earlier failed attempts remain part of the record and this later run does not explain their failures.

The table reports all comparison cells as `(B1 median + B2 median) / (A1 median + A2 median) - 1`; negative values mean the `opt` API took less time than the generic API. This is a single-machine diagnostic, not a confidence interval or evidence of a generally optimal dispatch threshold.

| Backend | 10+4, two data missing | 32+16, two data missing |
| --- | ---: | ---: |
| scalar-rust | +1.63% | -1.09% |
| rust-avx2 | -1.73% | -28.13% |
| rust-avx512 | -11.45% | -31.93% |
| rust-gfni-avx2 | -42.15% | +67.98% |
| rust-gfni-avx512 | -48.01% | +0.29% |

The wide case responds differently to the API choice on AVX2, AVX-512 and GFNI/AVX2. This supports further isolating their execution paths; it is not enough to alter generic `ReconstructShard` publication semantics or the GFNI selector. No runtime implementation or release threshold was changed by this run.

## Compatibility coverage

The [compatibility reports and output](../benchmarks/classic-recovery-analysis/2026-11-08/results/compatibility-11d/) cover the same two source revisions. The ARM logs are excerpts containing the final harness results; the JSON files record resource cases. On x86-64 the scalar, AVX2, AVX-512, GFNI/AVX2 and GFNI/AVX-512 backends each completed 200 exact parity and cross-decode cases, plus the harness's large GF8, 6+2 and small-block error-detection checks and 45 resource cases. On aarch64, scalar and NEON completed the same compatibility harness. The aarch64 checks were functional, not isolated performance measurements. All x86 tests used one AMD processor model; no Intel processor was tested.

The [plain probe](../benchmarks/classic-recovery-analysis/2026-11-08/generic-recovery-probe.rs) and [Linux telemetry probe](../benchmarks/classic-recovery-analysis/2026-11-08/generic-recovery-telemetry.rs) are retained alongside raw stage and monitor records. The telemetry probe reads `/proc/thread-self/schedstat` and therefore requires Linux. Copy the desired probe into each revision's `examples/`, compile with `--release --locked --features simd-accel`, set the `PROBE_*`, `RSE_BACKEND_OVERRIDE`, and `RSE_EXPECT_BACKEND` variables, and use an independent resource reservation and the full ABBA gate to reproduce an experiment. Raw records include all samples, not only passing stages.
