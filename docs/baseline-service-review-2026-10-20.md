# Protocol-15 baseline service review

This review compares published **9.0.0 with itself**, not 9.0.0 with the release
candidate. It does not approve a release or justify a production algorithm change.

## Complete control result

The frozen campaign retained 5,808 calibration records and 1,512 records in each
of A1 and A2: 8,832 original records in total. Independent replay checked all 126
cases, raw backend/I/O/placement/input contracts, duration floors, distributions
and resource restoration. There were no short samples or distribution failures;
maximum central widths were 5.2426% in A1 and 5.4811% in A2. Three same-version
repeat checks exceed the retained 5% threshold:

| Original case | A1 ns/iteration | A2 ns/iteration | Signed change |
| --- | ---: | ---: | ---: |
| `reconstruct_data:4:2:65536` | 9,729.44 | 9,087.86 | -6.5942% |
| `reconstruct_opt:4:2:65536` | 10,169.87 | 10,853.85 | +6.7256% |
| `verify:4:2:524288` | 81,375.64 | 85,605.82 | +5.1983% |

For every one of these cases, all 18 measured observations have zero process
minor and major faults. Maximum calling-thread runnable wait/wall is 0.0147%.
Calling-thread run time changes by approximately -6.5737%, +6.7176% and +5.2002%,
respectively, alongside the timed/wall changes. These counters bracket the whole
original operation; they are not exclusive API CPU cycles. The evidence narrows
the residual to execution service cost rather than measured page-fault or runnable
wait growth. It does not distinguish frequency, cache/memory effects or internal
layout. All 15 stream repeat checks passed, so more stream first-touch edits do
not address the three remaining failures.

The original capture uses published source `9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`, collector `2809182050f7198beb470fea954507c9d0a454dd` and
protocol 15; independent completion replay uses `b4b5101d6f71ec0d21d2a97afd6ec2723ce37687`. All raw records remain
under those exact identities. The owned child partition, controller and parent
CPU masks were restored. No candidate timing or new full-matrix retry was started.

## Native stream observations and a concrete provenance gap

A separate local Docker Linux/aarch64 execution of both actual codec sources
retained all 15 stream cases, 360 observations, 720 API calls and 95,760 structured
boundaries. It remains diagnostic, not native ARM throughput or x86 VM attribution.
It confirms retained verification scratch after the first block and the existing
recovery-owner capacity reduction. Observer-only full-store checks found no new
faults while filling the already resident recorder; extra prefill was unnecessary.

Equal 4 KiB base pages did not mean equal fault granularity: the local kernel
exposed enabled 16 KiB mTHP while the current VM exposed it as disabled. This is a
confirmed environment difference, **not evidence that THP caused the VM drift**.
The missing THP/mTHP policy envelope is repaired separately. Linux documents that
[mTHP can reduce fault counts by backing multiple base pages per fault](https://docs.kernel.org/admin-guide/mm/transhuge.html).
Minor-fault counts must not be multiplied by base page size to infer touched bytes.

Release evidence schema 4 and new qualification/layout envelopes bind the kernel
release, anonymous/shmem global and per-size interfaces and process THP-disable
mode. Policy is read outside API timers; unavailable or changed required state
is rejected. Historical controls are not retrofitted with missing fields. See the
[complete evidence contract](release-evidence.md#kernel-memory-policy-is-part-of-comparability).

## Next meaningful work

The policy-envelope fix is merged; retain the failed control. Before another full
campaign, require a specific, observable change that addresses execution-rate,
cache or remaining internal-layout variability. The current VM lacks usable
hardware-cycle evidence, so a unique hardware/cache attribution is still absent.
Do not retry unchanged measurements, select favorable cases/counts, relax limits,
change original allocation semantics or rewrite VerifyWorkspace/Leopard delivery
from allocation counts alone. Generic reconstruction publication, late-failure and
selective-recovery semantics remain unchanged. Final candidate/layout ABBA and a
strict positive preflight are still required before a 9.1.0 tag.

## Predeclared temporal-adjacency control

Read-only guest topology exposes no sibling thread on CPUs 24 through 31 and
places all eight in one NUMA node. This rejects two guest-topology hypotheses;
it establishes neither physical host isolation nor a frequency/cache cause.

Summing only the original recorded wall windows between each failed A1 worker
and its A2 counterpart gives lower bounds of 4,866.51 s, 4,862.94 s and 4,875.82 s,
respectively. Each of these workers contributed about 29 to 34 s of windows.
These are lower bounds, excluding startup and unrecorded gaps, not absolute-clock
measurements. Temporal separation is therefore a specific testable condition,
not a proven explanation of the three failures.

The next control pairs A1/A2 immediately for **every one of the 126 cases** using
published 9.0.0 and the original calibrated counts. It changes case scheduling,
not the worker, allocation lifetime, data input, backend, CPU budget, warmups or
samples. Each observation still uses the original protocol-15 duration floors.
It records the full kernel policy before/after each worker and requires identical
legacy context and caller geometry. A pair start gap above 10 s, short sample or
contract mismatch stops capture and preserves raw files; statistical failures are
retained across the entire predeclared cohort without trimming or retries.

This is `adjacent-baseline-diagnostic-v1`, always `release_approval=false` and
`formal_qualification=false`. Even a stable complete result cannot certify the
existing phase-major prerequisite or approve candidate timing. It can only inform
an explicit future scheduling-contract evaluation. The failed original control
and the existing 5% repeat/10% distribution gates remain unchanged. See the
[private diagnostic commands](release-evidence.md#adjacent-same-version-diagnostic).
