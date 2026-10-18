# Rejected 9.1.0 controlled campaign — audit data only

This data-only branch is not a release or an accepted performance baseline. Do not merge it into main or configure it as `RSE_RELEASE_EVIDENCE_REF`.

The complete published-9.0.0/candidate A1/B1/B2/A2 campaign contains 126 cases per phase, 504 distributions and 4,536 formal samples. All calibration probes, warmups, raw samples, normalized summaries, frozen counts and source/runtime metadata are retained without numeric redaction. Every copied evidence member is byte-for-byte identical to the original capture; SHA256SUMS lists the published files.

The current validator rejects 10/504 sample distributions. Independent same-version diagnostic replay also found 11/252 repeat drifts above 5%. No baseline/candidate performance attribution or release approval follows from this snapshot. The distribution rejection happens before candidate regression comparisons.

The measured candidate commit is recorded in the manifest. Main 8f775a2dff51740e766ea51eaa628cc78d795d08 has the identical complete source tree. Sampling protocol and implementation hashes remain bound by the manifest. Both versions use the same explicit combined mmap/trim policy; results do not establish default-allocator or downstream throughput.

The snapshot intentionally excludes ancillary build/launcher logs, SSH/controller scripts and unrelated process/host telemetry containing internal paths or identities. Those private diagnostic files do not participate in release-evidence validation; all validator inputs and raw performance observations are present. The small isolation summary contains restoration outcomes only. No user objects, customer data or credentials are included.

Use main's `scripts/release_evidence.py verify --manifest <checkout>/release-evidence/manifest.json` to reproduce the rejection. Manual workflow dispatch with `release_preflight=true` and this immutable data commit tests online rejection only; it never publishes. A fresh accepted complete campaign and positive strict preflight are still required before tagging 9.1.0.

## Additional complete protocol-3 baseline diagnosis

`diagnostics/protocol3-baseline` retains all JSON and frozen counts from two complete published-9.0.0-only phases: 126 cases per phase, 2,268 formal samples, 101 short samples, 21 unstable distributions and 12/126 repeat drifts above 5%. It has a diagnostic schema and no candidate attribution. The captured tool commit and protocol identify the exact historical implementation; current source changes require new measurements.

These rejected observations exposed the missing calibration warmup rehearsal. They are preserved without numerical edits, including all calibration probes and three warmups per measurement case. Logs and environment-management scripts remain private. This diagnostic data cannot satisfy the strict release evidence schema.

## Subsequent rejected rehearsal and independent CPU control

`diagnostics/protocol4-initial-rehearsal` preserves the complete first 126-case baseline phase from tool d8cf3f6: nine short samples and 14 unstable distributions. The predeclared quality stop prevented the repeat and any candidate attribution. This historical implementation still had different count-loading paths and between-sample export activity; it does not validate later worker changes.

`diagnostics/independent-cpu-control` contains the standalone source and all three warmups/nine samples on each of eight pinned guest CPUs in two waves. It performs integer computation without codec calls or heap allocation in the work loop. Maximum same-CPU median drift was below 0.1164%, maximum between-CPU median spread below 0.1464%; no major faults were observed. This control does not establish SIMD/memory-path stability or codec performance. It cannot satisfy release-evidence validation.

All copied numerical observations remain byte-for-byte unchanged. Build/launcher logs and environment management remain private. These experiments are diagnostic records, not accepted release inputs.

## Complete basic-telemetry baseline prerequisite

`diagnostics/protocol4-basic-baseline` retains the complete 126-case first phase using the final worker (tool 877a27a; main 7c2c232 has the identical source tree) and the same numeric basic telemetry as formal capture. All 1,134 samples met minimum intervals; five distributions remained unstable. The predeclared quality stop prevented A2 and any candidate comparison. The driver, source bindings, calibration, three warmups and all nine samples remain available. It is not release evidence. The default allocator result is retained regardless of any separately controlled allocation-policy experiment.

## Quality-stopped retained-arena attempt and layout attribution

`diagnostics/retained-arena-quality-stop` retains both complete calibrations, the complete A1 phase, all subsequently produced partial candidate data and extracted numerical timing/sample journals. The controlled allocator uses one arena, disables malloc mmap and retains top chunks. A1 still has one buffered 1KiB verification distribution above 10%; the owned controller was stopped and all resources restored. B1 completed and B2 started before cancellation; the cohort is incomplete, has no candidate attribution and cannot approve release. Default-allocator failures remain above.

`diagnostics/independent-gfni-control` preserves all AVX512/GFNI register-control observations and guarded standalone source. It does not call codec APIs or allocate in its work loop. Same-CPU median drift stayed below 0.13%, while this does not establish memory-path stability.

`diagnostics/verify-layout-first` and `diagnostics/verify-layout-counterbalanced` retain every same-version persistent-buffer layout observation, page-offset metadata and safe Rust source. The latter tests all 16 source/scratch offset combinations in forward and reverse order using the same underlying buffers. Same-layout reverse-order median differences are at most about 0.32% (1KiB) and 1.17% (4KiB); different-layout medians span about 30.5% and 53.6%. These slice-view diagnostics establish layout sensitivity and are not a candidate comparison or a replacement release case. The original Vec fixture layout was not captured, so not every historical outlier can be attributed retroactively. Matching and recording layout in the release harness remains necessary.

Only complete numerical journal lines are public; incomplete console fragments and build/launcher/environment-management logs remain private. No failing case is dropped to approve a release.

## Protocol 5 retained Vec controls and complete layout grid

`diagnostics/protocol5-retained-vec-and-grid` contains all scoped baseline calibration and repeated Vec observations, all complete slice-grid records, and an earlier premeasurement unit-count stop. All 11 Vec cases match their retained layouts and pass the scoped timing/distribution/repeat checks. One of the 64 full-grid distributions still fails quality and remains visible. The full original release campaign and positive strict preflight are outstanding; there is no candidate attribution or release approval. See that directory for exact counts, original numerical files and matched controller sources.

## Protocol 6 cross-version structural qualification

`diagnostics/protocol6-cross-version-fixture-inventory` retains both complete four-phase structural inventories: the main/protocol-5 source has 24 mismatching comparisons across six cases, while the candidate/protocol-6 source has all 66 comparisons matching, including OS base page provenance. Every inventory keeps all 528 raw observations. They use one iteration for geometry inspection and cannot establish performance or release acceptance. The earlier initialization stop and controller sources remain available.

## Fresh complete main/protocol-6 baseline prerequisite

`diagnostics/protocol6-main2fb-full-baseline` retains all 126 A1 cases and complete calibrations: four unstable distributions and nine short observations cause the predeclared stop before A2 or candidate collection. The complete raw data, matched controller and actual timing-validator replay remain public. This rejected default-allocator control is distinct from later fixture changes and never approves release.
