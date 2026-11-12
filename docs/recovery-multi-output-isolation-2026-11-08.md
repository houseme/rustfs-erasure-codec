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

## GFNI selector ablation

A separate [ablation experiment](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-gfni-ablation-11d-05/) compared identical `11d7d03c` sources and build settings except that the experimental GFNI multi-output selector declined input counts above 16. This is the previously rejected broad cutoff, used only as a diagnostic perturbation. The matrix included generic and `opt` recovery for 10+4 and 32+16, 1 MiB, missing `[0, 2]`, on AVX2, GFNI/AVX2 and GFNI/AVX-512. Every probe checked recovered bytes and the requested backend. Same-source controls preceded the full ABBA comparison.

All 12 calibration cells passed, with maximum drift 4.99% and central spread 6.91%. One of 12 comparison cells failed: the AVX2 10+4 `opt` B2 stage had 22.90% central spread, above the 10% limit. All stages and monitor records were retained and the guest partition was restored. The complete comparison cohort is unqualified; it cannot establish a causal performance result or justify dispatch changes. No sustained competing guest process was recorded, and the available observations do not establish why the spread failed.

A later [independent ablation run](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-gfni-ablation-11d-06/) used the same 12-cell matrix and added once-per-second cumulative guest CPU counters, including `steal`, to the monitor. All 12 same-source calibration cells and all 12 ablation cells passed the same prespecified limits; maximum drift and central spread were 1.01% and 4.80% in calibration and 3.41% and 5.15% in the comparison. The guest CPU's reported `steal` counter stayed at zero; this cannot prove physical-host exclusivity. No sustained competing guest process was recorded, and the partition was restored. This distinct run does not erase or explain the previous failed cohort.

The complete ablation comparison follows. Values are `(B1 median + B2 median) / (A1 median + A2 median) - 1`, where A uses the original selector and B applies the experimental cutoff. A negative value means the ablation took less time. All cases use 1 MiB shards and two missing data shards.

| Backend | 10+4 generic | 10+4 opt | 32+16 generic | 32+16 opt |
| --- | ---: | ---: | ---: | ---: |
| rust-avx2 | -0.38% | +0.10% | +0.45% | +0.32% |
| rust-gfni-avx2 | +0.03% | -0.07% | +0.87% | -57.12% |
| rust-gfni-avx512 | -0.04% | +0.28% | +1.14% | -34.64% |

On this AMD guest, declining the wide GFNI row selector substantially reduced elapsed time for the 32+16 `opt` case, while the same perturbation had much smaller changes in the measured generic and AVX2 controls. That single-variable result implicates the selector for this particular workload; it does not establish a safe size or processor-independent crossover. Earlier small-encoding regressions still rule out the broad cutoff, and the prior failed ablation remains unqualified. Further work needs a recovery-specific design, boundary measurements, and other processor coverage before any production dispatch change.

A follow-up [boundary calibration](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-gfni-boundary-11d-07/) covered 32+16, 64 KiB through 2 MiB, two and four missing data shards, and both GFNI backends. Eight of 24 same-source cells failed the drift limit; the maximum drift was 13.26%, while the maximum central spread was 4.74%. The runner stopped before any selector comparison and restored the guest partition. The reported pinned-vCPU `steal` counter stayed at zero and no sustained competing guest process was observed; neither observation establishes the cause of the drift or proves host isolation. These data cannot establish a crossover threshold, and no passing subset was promoted as a result.

## Recovery-only boundary experiment

An isolated experimental [patch](../benchmarks/classic-recovery-analysis/2026-11-08/recovery-only-ablation-f120.patch), based on `f120faa8448c3a6cfd7711757bd58a2f826f0a72`, changes the serial Classic recovery path to bypass the GFNI row selector for 32 inputs and two through four outputs. The archived zero-context patch applies with `git apply --unidiff-zero` and was verified to reproduce the tested source files exactly. The guard intentionally has no size threshold: it is an ablation for measuring boundaries, not a proposed production policy. Normal encoding still calls the original selector. Parallel recovery chunks and the `experimental-pr1-kernels` path are outside this ablation's scope.

The intended [boundary matrix](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-only-boundary-f120-08/) covered 32+16, sizes 64 KiB, 128 KiB, 256 KiB, 512 KiB, 1 MiB and 2 MiB, two, three and four missing data shards, and GFNI/AVX2 and GFNI/AVX-512. Both revisions used the same locked Rust 1.99.1 build settings and byte-checking probe. Nine of 36 same-source calibration cells failed the 5% drift limit, with a maximum drift of 18.78%; maximum central spread was 5.45%. The runner therefore did not execute the experimental comparison. All 144 calibration stages and monitoring records are retained; selecting only passing cells would not qualify the experiment. The pinned vCPU reported no steal ticks, and no sustained competing guest process was observed; the cause of the drift remains undetermined. The guest partition was restored.

On this AMD guest, the experimental source passed the existing two focused generic-versus-`opt` recovery tests and one GFNI row-versus-scalar test. An additional [scope check](../benchmarks/classic-recovery-analysis/2026-11-08/recovery-scope-check.rs) compared generic and optimized recovery for two, three and four missing shards at 64 KiB, 128 KiB and 1 MiB, and verified encoding parity, on scalar, AVX2, GFNI/AVX2 and GFNI/AVX-512. Its [output](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-only-boundary-f120-08/scope-check.log) records those 36 recovery fixtures and 12 encoding checks. These focused checks do not replace the full compatibility suite. The no-default-features build also passed on aarch64.

No Intel processor was available. Neither a recovery size threshold nor a production dispatch change is justified by the failed calibration; the earlier broad-cutoff encoding regressions remain relevant. Validation on a stable AMD window and an independently supplied Intel host remains necessary.

## Same-process drift diagnostic

To investigate the failed boundary calibration, a [same-binary diagnostic](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-process-variation-f120-09/) compared four separately launched processes with four sequential stages inside one process that retained the codec and present input buffers. Both modes used source `f120faa8`, the same [probe](../benchmarks/classic-recovery-analysis/2026-11-08/process-variation-probe.rs), 15 samples of at least 300 ms per stage, the same warmup before each stage and identical output byte checks. The probe's `PROBE_STAGES` is `1` for separate processes and `4` for one process. Mode order alternated across the six cases, which were selected from earlier failures; this is a targeted diagnostic rather than a representative sample or a candidate comparison. Reusing the process changes both setup and elapsed time, so the results cannot isolate allocation layout from CPU or host changes.

Cells show maximum A1/A2 or B1/B2 median drift and maximum stage central spread. The previous 5% and 10% limits are shown only for evaluating the same-source controls; no release gate was changed.

| Backend | Size / missing | Four processes | One process |
| --- | --- | ---: | ---: |
| rust-gfni-avx2 | 512 KiB / 2 | 1.31% / 7.16% | 0.18% / 1.35% |
| rust-gfni-avx2 | 1 MiB / 3 | 0.96% / 4.03% | 1.91% / 3.33% |
| rust-gfni-avx2 | 512 KiB / 4 | 2.46% / 3.48% | 4.07% / 5.66% |
| rust-gfni-avx512 | 128 KiB / 2 | 18.10% / 4.66% (failed) | 2.16% / 5.28% |
| rust-gfni-avx512 | 512 KiB / 3 | 10.63% / 4.52% (failed) | 4.53% / 6.19% |
| rust-gfni-avx512 | 2 MiB / 4 | 1.70% / 2.66% | 1.05% / 12.11% (failed) |

Two of six four-process groups exceeded the drift limit; one of six same-process groups exceeded the distribution limit. The latter included a stage with 12.11% central spread, so retaining the process and input buffers did not eliminate all instability. Both modes passed for some cases and failed for others; the small targeted set and time ordering do not prove a cause. The guest monitor recorded no sustained competing process and no change in reported pinned-vCPU steal ticks; the partition was restored. Physical-host isolation remains unverified.

In the earlier failed 512 KiB, two-output GFNI/AVX2 cell, the median reconstruction time changed from approximately 1.024 ms in A1 to 0.844 ms in A2. The median per-iteration thread running-time estimate for the surrounding sample loop changed similarly, while the median reported thread waiting time was below 0.03 ms per roughly 300 ms sample. These counters cover different scopes and do not identify the cause; they make guest-thread queueing alone an insufficient explanation for that observation. A later same-process 2 MiB cell also had a slow sample with a corresponding increase in per-iteration thread running time. None of these results justifies removing failed samples or treating a stable subset as the full boundary result.

A [second guest run](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-process-variation-3c4-10/) used the same processor model, probe and protocol, with source `3c4d346e`; that revision adds only diagnostics and documentation relative to the first guest's source. This is an independent VM observation, not coverage of a different CPU model. Results use the same drift/spread format:

| Backend | Size / missing | Four processes | One process |
| --- | --- | ---: | ---: |
| rust-gfni-avx2 | 512 KiB / 2 | 22.33% / 1.07% (failed) | 0.19% / 0.91% |
| rust-gfni-avx2 | 1 MiB / 3 | 1.13% / 0.57% | 0.23% / 0.60% |
| rust-gfni-avx2 | 512 KiB / 4 | 0.52% / 0.82% | 0.45% / 0.64% |
| rust-gfni-avx512 | 128 KiB / 2 | 8.25% / 1.93% (failed) | 1.62% / 2.38% |
| rust-gfni-avx512 | 512 KiB / 3 | 5.89% / 1.54% (failed) | 0.23% / 1.44% |
| rust-gfni-avx512 | 2 MiB / 4 | 0.39% / 1.25% | 0.31% / 0.69% |

Three of six four-process groups exceeded the drift limit and all six same-process groups were within both limits on this guest. The first guest's same-process failure remains part of the record. Combined, these observations make process and setup effects worth investigating, but they do not rule out timing, CPU state or host effects; no cause has been established. The pinned-vCPU steal counter did not change, no sustained competing guest process was observed, and the guest partition was restored.

## Same-process paired prototype

A [paired prototype](../benchmarks/classic-recovery-analysis/2026-11-08/paired-process-experiment/) links the baseline and recovery-only experimental implementation into one executable. To distinguish the same package and version in Cargo's lockfile, the experimental copy changes its package version to `9.1.0+recovery.ablation`; the source and that metadata change are experimental build differences and may affect code generation or binary layout. This executable is a diagnostic, not an equivalent replacement for the release build. The manifest is archived as `Cargo.toml.txt`; the relative `../base` and `../variant` paths require separate source directories outside the repository workspace. Apply the archived zero-context recovery patch to `variant`, change only that variant's package version metadata, copy the manifest and lockfile into the paired directory, and compile the probe with locked dependencies.

Within each case, the probe validates that both implementations encode the same bytes, then shares the same present input buffers between four stages. Same-source calibration uses the baseline in all four stages; in the intended candidate phase only B1 and B2 would use the experimental implementation. Output recovery is byte-checked after every stage. This setup controls some process and input-layout variation but does not prove host isolation or replace separate-process qualification.

On the second AMD guest, six targeted same-source controls ran before any candidate phase. [Raw results](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-paired-control-3c4-11/) contain all 24 stages. One of six controls failed the unchanged gate: GFNI/AVX2, 32+16, 512 KiB, two missing data shards had 5.28% A1/A2 drift. The maximum central spread across the controls was 9.32%. The runner stopped before the experimental candidate phase, with no sustained competing guest task or change in reported vCPU steal ticks, and restored the partition. None of the passing controls is promoted to a candidate result; this does not establish a recovery crossover or justify changing the production selector.

## Read-only frequency-counter diagnostic

On the second AMD guest, `perf` did not expose cycle or instruction events and the guest did not expose CPU frequency controls. The pinned CPU's APERF and MPERF MSRs were accessible for reading and increased under a brief CPU-bound check. A [diagnostic probe](../benchmarks/classic-recovery-analysis/2026-11-08/process-frequency-probe.rs) added read-only counter deltas around each approximately 300 ms sample; the reads and scheduling counters include the surrounding sample loop and are outside the timed reconstruction calls. The probe requires Linux, read permission for the selected CPU's MSR device, and external affinity to that CPU. In a VM these counters may be virtualized and the ratio must not be assumed to be a direct physical-core frequency measurement.

Using source `5e36525c`, the same six previously failing workload shapes were measured in four-process and one-process modes, with byte checks and the same drift/spread rules. The [full record](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-frequency-diagnostic-5e-12/) includes all 720 samples and raw APERF/MPERF deltas. Three of six four-process groups and two of six one-process groups exceeded the limits; this is a targeted diagnostic, not a candidate evaluation. All observed sample APERF/MPERF ratios ranged from 1.5508 to 1.6529. The guest monitor recorded no sustained competing process and no change in pinned-vCPU steal ticks; the partition was restored.

For example, in the same-process GFNI/AVX-512, 512 KiB, three-output case, A1 and A2 reconstruction medians were approximately 1.065 ms and 0.949 ms, while their sample APERF/MPERF median ratios were 1.6022 and 1.6044. In a separate-process 128 KiB, two-output case, A1 and A2 medians were approximately 0.135 ms and 0.148 ms while the ratios were 1.6059 and 1.6060. The counters' scope and virtualized semantics prevent attributing a cause, but the observed ratio alone does not explain these stage differences. No failed samples were removed and no dispatch threshold was selected.

## Thread fault-counter diagnostic

A separate [diagnostic](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-fault-diagnostic-5e-13/) retained the same six targeted cases, both process modes and read-only APERF/MPERF sampling, and added per-thread minor and major fault deltas from `/proc/thread-self/stat`. The [probe](../benchmarks/classic-recovery-analysis/2026-11-08/process-faults-probe.rs) reads counters around each sample loop, so the counts also include setup and instrumentation and are not restricted to timed reconstruction. The extra reads make this a separate instrumented run, not a direct continuation of the earlier results.

Four of six four-process groups exceeded the stability limits; all six same-process groups were within them. Across all 720 samples there were zero recorded major faults and 120 minor faults: 690 windows recorded zero and 30 windows recorded four. The reported APERF/MPERF ratios ranged from 1.6124 to 1.7447. No sustained competing guest process or increase in reported vCPU steal ticks was observed, and the partition was restored. These are targeted same-source observations, not evidence for an optimization or physical-host isolation.

For example, the four-process GFNI/AVX2 512 KiB, four-output case had A1 and A2 reconstruction medians of approximately 2.178 ms and 1.860 ms. Each stage recorded four minor faults across its 15 samples and zero major faults; sample APERF/MPERF medians were 1.6320 and 1.6261. Those counters do not by themselves explain the stage difference, but they also cannot exclude host scheduling, physical memory placement, cache effects, or virtualized counter limitations. The output buffers are allocated and released during sampling, and same-process mode retains surviving input buffers, so process reuse alone does not hold every memory factor constant. No sample or failed cohort was discarded.

## Buffer-layout diagnostic and output lifetime

A targeted same-source diagnostic compared two simultaneously allocated `Option<Vec<u8>>` input sets with identical encoded bytes and equal total reported capacity. Each present shard reserved zero to 960 extra bytes in 64-byte steps; the order of these padding amounts was reversed between the sets. Six earlier problematic cases were selected, and every stage verified recovered bytes. Both sets and the original fixture stayed live, so allocator history and working-set size differ from the earlier one-fixture probe. This is an experimental layout perturbation, not a candidate implementation or a production benchmark.

The initial [probe](../benchmarks/classic-recovery-analysis/2026-11-08/layout-retained-probe.rs) and [complete results](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-layout-diagnostic-7b-14/) exposed a confound: recovered outputs from one set remained allocated when the other set began. All six same-set controls passed, but two of six layout-comparison groups failed; maximum comparison drift was 15.13% and maximum central spread 13.74%. The run recorded 8,937,036 minor faults and no major faults, with the large fault counts concentrated in stages using the second set. The retained outputs and allocator behavior prevent attributing any timing differences to input placement. The entire comparison remains unqualified.

A [revised probe](../benchmarks/classic-recovery-analysis/2026-11-08/layout-no-retained-probe.rs) drops all recovered outputs after every stage, outside timed samples, and records only aggregate virtual-address bucket matches, not addresses. Its independent [results](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-layout-no-retained-7b-15/) also contain all 720 samples: six of six same-set controls passed, while two of six layout comparisons failed, with maximum comparison drift 6.44% and central spread 13.01%. The revised run recorded 48 minor faults and no major faults in total. This reduction is consistent with the output-lifetime confound, but the independent runs and additional instrumentation do not establish its sole cause. The reported pinned-vCPU steal counter did not change, the monitor saw no sustained competing guest process, and both partitions were restored.

In all six revised comparison fixtures, corresponding selected inputs had equal virtual 4 KiB-offset buckets for 32 of 32 shards and equal virtual 64 KiB-offset buckets for zero of 32 shards. These are aggregate virtual-address observations, not physical-cache-set measurements: guest and host mappings, output allocation, CPU state and the altered allocation history remain uncontrolled. Because two comparison groups failed, none of the layout timing differences is promoted as causal evidence or used to choose a dispatch threshold. Earlier failed calibration and prior diagnostic records remain unchanged.

## Output-lifetime diagnostic attempt

Classic `Option<Vec<_>>` bulk recovery allocates missing output buffers; the generic `ShardSlot` path can retain buffers but uses a different coding route. A targeted [same-API probe](../benchmarks/classic-recovery-analysis/2026-11-08/output-lifetime-probe.rs) was prepared to compare immediately dropping the prior recovered outputs with keeping those outputs alive during the next recovery. Both modes use the same `reconstruct_data_opt` API and identical inputs; moving or dropping the previous outputs is outside the timed reconstruction call. The retained mode also increases the live output-buffer footprint, so a result would describe that complete lifetime change rather than isolate a single allocator or cache mechanism.

The [calibration record](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-output-lifetime-5558-16/) uses source `5558f8a3`, the six earlier targeted cases, read-only frequency and fault counters, and the unchanged 5% drift and 10% central-spread gates. One of six same-source, immediate-drop controls failed: GFNI/AVX2, 32+16, 512 KiB, four missing shards had 29.02% central spread in A2. The runner stopped before executing any retained-output stage. All 24 calibration stages are retained; the guest monitor observed no sustained competing process or change in reported pinned-vCPU steal ticks, and the partition was restored. A separate functional smoke used the same probe with its sample window shortened from 300 ms to 1 ms, on 64 KiB shards with two missing outputs. It verified the non-retained/retained stage sequence and output bytes on GFNI/AVX2 and GFNI/AVX-512; its [summary](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-output-lifetime-5558-16/functional-smoke.log) excludes timing. This attempt provides no performance comparison of the two lifetimes and cannot justify a production change or an allocation-performance claim.

## Two-output GFNI prototype

The existing GFNI row executor visits the inputs separately for each output row. An isolated [prototype patch](../benchmarks/classic-recovery-analysis/2026-11-08/gfni-pair-prototype-df8.patch), based on `df8b12ed`, computes exactly two output rows together and shares each vector input load. This is an experimental kernel, not a production change: the prototype hooks a shared encoder/recovery executor, so it is broader than a recovery-only policy. Its zero-context patch requires `git apply --unidiff-zero`; applying it was verified to reproduce the tested source files. It has not been shown to be faster and would need scope redesign, comprehensive compatibility tests and encoding regression checks before consideration.

On the AMD guest, focused GFNI tests passed for two outputs across all 256 seed values and the existing dimensions, chunk boundaries, shifted views and scalar tails, alongside invalid-view rejection and two generic-versus-`opt` recovery tests. These checks do not constitute full compatibility coverage or Intel validation.

The intended [performance experiment](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-gfni-pair-df8-17/) used 10+4 and 32+16, 64 KiB, 512 KiB and 1 MiB, two missing data shards, and GFNI/AVX2 and GFNI/AVX-512. Three of 12 same-source calibration cells failed the unchanged stability rules, with maximum drift 15.17% and maximum central spread 20.71%. The runner stopped before the prototype comparison; all 48 calibration stages are retained. No sustained competing guest process or reported vCPU steal increase was observed, and the guest partition was restored. No speedup or regression can be inferred from this attempt.

## Recovery-scoped two-output GFNI prototype

A separate [experimental patch](../benchmarks/classic-recovery-analysis/2026-11-08/gfni-recovery-pair-scoped-5cd.patch), based on `5cd12744`, routes serial Classic recovery with exactly two outputs through the paired GFNI kernel. Normal encoding continues to invoke the original row-by-row executor; a focused mock test verifies this dispatch distinction. Generic destinations, parallel recovery chunks and the `experimental-pr1-kernels` route do not acquire a new bulk path. The shared helper is refactored, so unchanged encoding behavior does not prove unchanged encoding performance. The zero-context patch requires `git apply --unidiff-zero` and was checked against the exact experimental source files.

On an AMD guest, the candidate passed the recovery-only routing test, invalid-input-before-write test, and the GFNI row-versus-scalar dimensions, coefficients, shifted views, chunk boundaries and tails test. The existing [scope probe](../benchmarks/classic-recovery-analysis/2026-11-08/recovery-scope-check.rs) passed nine recovery fixtures and three encoding parity checks on each of scalar, AVX2, GFNI/AVX2 and GFNI/AVX-512; its [output](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-scoped-pair-5cd-18/scope-check.log) is retained. The commands `cargo check --no-default-features --features simd-accel` and `cargo check --all-features` also passed in the workspace. These focused checks do not replace full compatibility testing, architecture coverage or encoding performance guards.

The intended [performance comparison](../benchmarks/classic-recovery-analysis/2026-11-08/results/recovery-scoped-pair-5cd-18/) covered 10+4 and 32+16, 64 KiB, 512 KiB and 1 MiB shards, two missing data outputs, GFNI/AVX2 and GFNI/AVX-512. Six of 12 same-source calibration cells failed the unchanged stability rules; maximum drift was 23.79% and maximum central spread was 64.42%. The candidate phase did not run. All 48 calibration stages and monitor data are retained. The guest monitor saw no sustained competing process or reported vCPU steal increase; the partition was restored. The experiment supplies no candidate speedup or regression result and does not justify a production dispatch change.

## Expanded paired-coefficient audit

The recovery-scoped prototype's prior tests varied each seed across 256 values but did not independently exhaust both output-row coefficients. A [complete alternative experimental patch](../benchmarks/classic-recovery-analysis/2026-11-08/gfni-recovery-pair-exhaustive-5cd.patch) adds a scalar-oracle test for all 65,536 combinations of two independently chosen coefficients. It also checks shifted views, the 65-byte vector/tail boundary and output sentinels. This is a complete replacement for the earlier scoped patch, not an additional patch to stack on it. The zero-context patch applies with `git apply --unidiff-zero` and was verified against the experimental source.

On an ARM64 host, `cargo test --locked --offline --all-features` completed with 736 passed, 43 ignored and 45 filtered across 77 test-result summaries, including doc tests; x86 GFNI code is not executed by this run. The x86-64 Linux library checked successfully with `simd-accel`, both with and without default features. The x86 test targets also cross-checked successfully with a Zig C cross-compiler. Cross-checking is compilation only: the new exhaustive test has **not** run on x86 GFNI hardware, and it reports how many supported backends executed when run with output visible. The earlier AMD checks apply to the previous patch, not to this newly added test.

Both previously authorized AMD servers timed out during this audit. No performance experiment or Intel test ran, no candidate performance conclusion was obtained, and no production dispatch was changed. Before adoption, run the exhaustive test on supported GFNI/AVX2 and GFNI/AVX-512 hardware, complete cross-version compatibility and encoding guards, and satisfy stable performance controls on the required processor coverage.

## Current-lockfile recheck

On 2026-11-12, main was `d991fecf663999a7a63a7f41461c405126af5984`. The recovery and GFNI source files touched by the exhaustive prototype were unchanged since its original base, and the archived zero-context patch still applied cleanly. The dependency manifest and lockfile had changed, including `smallvec`, `toml` and `wasm-bindgen`; previous binary and performance results therefore do not qualify the new build.

In a separate clone of `d991fecf` with the complete prototype patch applied, Rust 1.99.1 `cargo test --locked --offline --all-features` completed on ARM64 with 736 passed, 43 ignored and 45 filtered across 77 result summaries, including doc tests. This does not execute x86 GFNI code. The x86-64 Linux test targets passed `cargo check --locked --offline -p rustfs-erasure-codec --tests --target x86_64-unknown-linux-gnu --features simd-accel` using Zig as the C cross-compiler; the x86-64 library also passed the same-target check with `--no-default-features --features simd-accel`. These are compile checks, not x86 executions. The host could not launch the x86-64 Darwin feature probe, so it did not supply substitute GFNI execution.

Both previously authorized AMD servers timed out over SSH. The exhaustive two-row coefficient test was not executed on GFNI hardware, and no new performance comparison or Intel validation was performed. This recheck changes no production code and does not approve a release or a dispatch strategy.

## Compatibility coverage

The [compatibility reports and output](../benchmarks/classic-recovery-analysis/2026-11-08/results/compatibility-11d/) cover the same two source revisions. The ARM logs are excerpts containing the final harness results; the JSON files record resource cases. On x86-64 the scalar, AVX2, AVX-512, GFNI/AVX2 and GFNI/AVX-512 backends each completed 200 exact parity and cross-decode cases, plus the harness's large GF8, 6+2 and small-block error-detection checks and 45 resource cases. On aarch64, scalar and NEON completed the same compatibility harness. The aarch64 checks were functional, not isolated performance measurements. All x86 tests used one AMD processor model; no Intel processor was tested.

The [plain probe](../benchmarks/classic-recovery-analysis/2026-11-08/generic-recovery-probe.rs) and [Linux telemetry probe](../benchmarks/classic-recovery-analysis/2026-11-08/generic-recovery-telemetry.rs) are retained alongside raw stage and monitor records. The telemetry probe reads `/proc/thread-self/schedstat` and therefore requires Linux. Copy the desired probe into each revision's `examples/`, compile with `--release --locked --features simd-accel`, set the `PROBE_*`, `RSE_BACKEND_OVERRIDE`, and `RSE_EXPECT_BACKEND` variables, and use an independent resource reservation and the full ABBA gate to reproduce an experiment. Raw records include all samples, not only passing stages.
