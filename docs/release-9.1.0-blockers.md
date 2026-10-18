# 9.1.0 publication blockers and completion criteria

Audit date: 2026-10-19 (Asia/Shanghai). Main includes PR #18 at
`19037b31f74de4081e41ec48baf06fce9dad420c`. PR #19 contains the shared
protocol-10 measurement corrections described below. This document is a current
readiness assessment, not release approval; historical evidence keeps its original
source and protocol bindings.

## Implementation and acceptance are separate

The shared sampler now prepares one codec and caller fixture per worker for all
88 small-file and 15 stream cases. Caller byte storage precedes codec construction;
the generated inner kernels reuse the original benchmark loops. Original timed
per-iteration clones, missing-output allocations and I/O remain present. Each
stream writer receives its own reservation. Ordinary procfs telemetry uses bounded
stack storage, and hotspot result patterns use bounded inline metadata with deferred
formatting. Separate native probes check the original formatter's exact bytes and
zero replacement heap activity. Production algorithms, matrices, APIs, dependencies
and benchmark source files are unchanged by PR #19.

The [caller contract](release-caller-fixtures.md) records concrete API entries,
lifetimes, missing/required masks and source/scratch geometry. All 103 caller cases
have matching structural inventories under both declared allocator contexts:
4,944 observations and 618 comparisons per context. These use one iteration and
cannot qualify sample duration, repeat drift or throughput. Dynamic destinations,
internal scratch and smoke/hotspot source layouts remain outside this tracked scope.
The separate 16-offset 1/4 KiB diagnostic is not four-phase paired acceptance.

## Current release gates

| Layer | Verified state | Required next result |
| --- | --- | --- |
| Functional compatibility | The normal-format 200-case published-9.0.0 parity/roundtrip probe and core tests pass. PR #19 changes no production algorithm. | Preserve the declared compatibility boundary: the normal-format probe disables `fast_one_parity` and does not promise recovery of historically inconsistent fast/custom/mixed-entry codewords. |
| Workload comparability | The 103 caller cases share an explicit preparation contract; dynamic/internal buffers and full per-layout coverage remain partial in backlog #2897. | Complete the agreed dynamic/lifetime and per-layout acceptance scope; caller matching alone is insufficient. |
| Baseline stability | Both complete protocol-10 9.0.0 qualifications failed their original gates; details below. | Qualify the entire baseline before candidate attribution. Preserve every failed case and allocator context. |
| Quantitative evidence | No accepted complete campaign for the final source tree and shared harness exists. | Fresh baseline/candidate A1/B1/B2/A2, both calibrations and all 126 original cases, with every duration/distribution/drift/regression gate passing. |
| Workflow input | `RSE_RELEASE_EVIDENCE_REF` was absent at the last live check. Rejected audit data is not accepted evidence. | Verify accepted `release-evidence/manifest.json` on a data commit, then supply its full immutable SHA through the variable or manual `evidence_ref`. |
| Strict preflight | Ordinary CI skips tag/manual-only evidence and release-preflight jobs. The historical real-data negative run proves rejection only. | Run `release_preflight=true` against the final main and accepted evidence, and verify actual success before tagging. |
| Upload | The repository has a secret named `CRATES_IO_TOKEN`; its value was not read. Package dry-runs are in ordinary CI. | After all gates pass, tag the validated source and verify actual publication; neither a secret name nor a dry-run is an upload. |

The automated chain in `.github/workflows/ci.yml` requires source-bound immutable
evidence in `release-inputs`, then strict `release-preflight`, before a tag can
reach `publish`. GitHub does not read issue checkboxes; those describe the agreed
additional acceptance scope. No new blocking algorithm defect has been established
by these baseline controls. An unstable published-9.0.0 control cannot establish a
9.1.0 regression or speedup.

## Latest complete baseline controls

Frozen measured tool: `63b6fa0a3d0b0dc6a7d206c9dbff82f5f729eea8`, protocol 10.
Both measured codecs are published 9.0.0. Allocator contexts were predeclared in
order: default, then retained arena (one arena, malloc mmap disabled, 1 TiB trim
threshold, child-process environment only). No cross-context speedup is inferred.

- Default: 4,800 calibration observations and complete A1 (1,512 observations,
  including 1,134 measured samples). Zero short samples or caller mismatches, but
  seven distributions exceed 10%; the declared stop prevents A2 and attribution.
- Retained arena: 4,980 calibration observations and complete A1/A2 (3,024
  observations, including 2,268 measured samples). All 252 distributions pass
  10%, with zero short samples/caller mismatches; seven of 126 repeat drifts exceed
  5%, maximum 28.181878%. This context also fails the baseline prerequisite.

[Complete qualifications and inventories](https://github.com/houseme/rustfs-erasure-codec/tree/75456a77a57953f8b069bab3d2eb788643ca21d1/diagnostics/protocol10-qualification-controls),
[default raw data](https://github.com/houseme/rustfs-erasure-codec/tree/75456a77a57953f8b069bab3d2eb788643ca21d1/diagnostics/protocol10-default-baseline),
and [retained-arena raw data](https://github.com/houseme/rustfs-erasure-codec/tree/75456a77a57953f8b069bab3d2eb788643ca21d1/diagnostics/protocol10-retained-arena-baseline)
retain all calibration, warmup, measured values, failures and source bindings.
The audit branch has 671 verified members at that commit; all 1,767 isolation
checks and resource restoration were recorded. It is a data-only branch and must
not be merged into main or configured as accepted release evidence.

## Reproducible variability diagnosis

Run the offline analyzer against either complete baseline capture with its matching
protocol/renderer. It starts no workers and changes no sampling or acceptance policy:

```sh
python3 scripts/analyze_baseline_variability.py \
  --capture /path/to/complete-baseline-control \
  --output /path/to/new-variability-report.json
```

The analyzer validates all calibration and fixed warmup/sample records through the
shared validators, binds input hashes, and retains every case and full scheduling
trace. It reports incomplete repeat coverage explicitly, preserves width/duration/
layout failures and refuses to overwrite reports. `release_approval` is always
false and `candidate_attribution` is always null. It supplements original
provenance/isolation verification; it is not another release gate.

Retained-arena replay groups all 126 cases by their nine measured CPU endpoints:

| Endpoint observation | All cases | Repeat failures over 5% |
| --- | ---: | ---: |
| One identical endpoint CPU in A1 and A2 | 16 | 2 |
| One endpoint CPU per phase, different between phases | 100 | 4 |
| Multiple endpoint CPUs in at least one phase | 10 | 1 |

The first group includes ordinary encode 10+4/512 KiB (CPU 29, 15.827029% drift)
and optimized recovery 4+2/64 KiB (CPU 31, 18.164593% drift). The maximum observed
main-thread runnable-wait/wall ratio across these two failed cases is 0.019055%.
Changing endpoint CPUs or recorded main-thread runqueue delay alone therefore
cannot explain all failures. Equal endpoints do not exclude intermediate migration;
main-thread counters omit Rayon worker scheduling. Counter reads bracket the wall
timer, so run/wall ratios slightly above one are retained without clamping.

Next diagnosis must distinguish actual worker placement/execution paths and
physical/page/cache state while preserving real API and allocation contracts.
Do not simply taskset the entire worker to one CPU: both codec versions read
`std::thread::available_parallelism()` during construction and cache that budget,
which can silently turn a parallel workload into a serial one. No existing result
uniquely assigns the residual variability to the CPU, allocator or physical host.

## Merge and release decisions

PR #19's measurement correctness and architecture can be reviewed and merged
independently of performance approval. A green ordinary CI supports that code
review, not a tag. The failed controls above measure 9.0.0 itself and do not by
themselves block merging an otherwise correct, fail-closed measurement tool.

Backlog #2897 stays open: only condition 4 is checked within its declared normal-
format scope; conditions 1/2/3/5 are partial and unchecked; condition 6 is incomplete.
Merging PR #19 changes pending implementation status, not those completion marks.

Complete the remaining contracts and baseline qualification, freeze the final
complete candidate tree/lock/harness, then collect and verify the entire paired
campaign. Keep repeat drift at 5%, central spread at 10%, all minimum intervals
and per-operation regression limits. Code/document changes after capture require
new source binding and evidence; an unchanged complete tree after rebase merging
is accepted under the existing policy. Only a successful strict positive preflight
permits evaluating the tag and upload.

Native ARM and downstream/S3 throughput remain independently tracked in #2853.
The existing release checklist allows a correctness/API release to retain that
P2 task if actual release gates pass and notes make no unsupported throughput
claims. Open P2 tasks are not automatic tag gates; ARM Docker and ordinary CI
cannot replace native throughput acceptance.
