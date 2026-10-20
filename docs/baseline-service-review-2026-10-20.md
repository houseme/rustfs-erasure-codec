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
cache or remaining internal-layout variability. The current VM lacks a usable CPU cycles PMU. Read-only APERF/MPERF/TSC
MSRs were subsequently found exposed; their virtualized fidelity and relationship
to the service drift still require observation, so a unique attribution is absent.
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

## Interpreting adjacent timing evidence

The original execution journal bounds process invocation, not an inner API start
or end. A 48 ms worker-start gap must not be described as an actual API gap. After
complete frozen replay and resource restoration, the existing analyzer reports
all 126 service distributions and conservative outer workload-window separation
bounds. It subtracts each worker's recorded integer wall windows from its parent
elapsed interval and assigns all remaining time to the between-worker gap. This
also includes unrelated setup/inter-observation/teardown costs, so it is an upper
bound, not a precise timestamp. A second bound adds all A2 warmup windows before
its first measured sample; the full two-worker envelope also bounds every window
in the pair. Floating-point aggregate API durations are not
converted into invented boundary times.

The [analysis command](release-evidence.md#analyze-complete-adjacent-service-windows)
uses an explicitly trusted frozen checkout, preserves the original identity and
refuses incomplete/failed captures or changed members. It adds no timing gate and
cannot make the active diagnostic into qualification or release evidence. The
ongoing campaign and formal sampler remain unchanged.


## Completed adjacent control and targeted execution-rate diagnosis

All 252 unchanged workers and 3,024 original observations completed under frozen
controller `d53e16b144c85ccb253dfc530bb1797b749bfd52`. Independent complete
replay and restored-resource checks passed. The full service analysis has no
short samples, distribution failures or caller-geometry mismatches, but retains:

| Original case, both 9.0.0 | A1 ns/iteration | A2 ns/iteration | Signed change |
| --- | ---: | ---: | ---: |
| `encode:10:4:524288` | 387,412.46 | 336,299.98 | -13.1933% |
| `reconstruct:4:2:65536` | 9,491.32 | 10,253.67 | +8.0321% |

All 18 measured observations of each failure have zero minor/major faults.
Maximum runnable wait/wall is about 0.0147%; calling-thread run time tracks wall.
Parent gaps are about 39/38 ms and pair envelopes about 71.21/63.43 s. Thus long
phase separation alone is insufficient to explain or repair the variation.
Passing the previous three failed cases in this campaign does not repair either
failed complete control or establish a candidate improvement.

The existing VM diagnostic entry point can now observe read-only guest-CPU
APERF/MPERF/TSC around original workers, without adding Rust telemetry or changing
the original binary, prepared fixture or allocator lifetime. It derives **all**
failed cases from trusted complete adjacent replay, uses the original iteration
budgets and exactly four fresh workers per case. There is no case/count override,
new calibration or favorable retry. The current cohort has two cases with counts
8,192 and 262,144. Original journals, context and counter reads remain private.

Counters cover entire worker invocations, including setup, all warmups/samples,
export and guest activity. Per-CPU non-atomic read skew and TSC interval bounds
are retained; short reads, reset/wrap, invalid brackets and no active counters
are refused with raw failures preserved. Virtualized MSRs are not certified as
physical execution cycles. Ratios are not GHz, exclusive API cycles, per-sample
hardware evidence or timing normalization. These observations cannot satisfy the
5% gate or replace full final candidate/layout ABBA and positive release preflight.

See the [private counter diagnostic](release-evidence.md#whole-worker-execution-rate-diagnostic).


## Existing worker evidence separates wrapper cost from busy service cost

The completed fixed cohort uses frozen controller
`d7f920f9ea9dfab21b5419e30db40e418dbad199` and published 9.0.0. It contains
eight workers and 96 observations; these figures replay that same private
snapshot, not a new timing campaign.
The original baseline worker binary was byte-identical before and after capture.
All counts, input replay, caller geometry, placement and original work loops match.

| Observed cost or signal | Encode 10+4 / 512 KiB | Reconstruct 4+2 / 64 KiB |
| --- | ---: | ---: |
| Invocation time outside all original wall windows | 0.2763–0.3084% | 0.2852–0.3100% |
| Wall minus original inner measured time, relative to wall | 0.0000593–0.0000621% | 0.0000476–0.0000665% |
| Calling-thread run / wall, all twelve windows | 99.9689–99.9765% | 99.9716–99.9793% |
| Formal-sample process minor / major faults | 0 / 0 | 0 / 0 |
| Four formal medians, maximum / minimum minus one | 15.3440% | 7.7108% |
| Whole-worker APERF / MPERF, maximum / minimum minus one | 2.1665% | 1.2157% |

The residual variation is inside the retained original operation windows. It is
too large to attribute entirely to measured wrapper cost, runnable wait or page
faults. The encode worker retains its original data/parity Vec storage throughout
all iterations; this is not a stream-writer lifecycle experiment. Original
reconstruction includes caller clone/allocation/recovery work, so its inner timing
must not be relabeled as exclusive decoder cost.

For encode workers two to three, whole-worker elapsed divided by the same total
original operation count increases 12.18%, while APERF/MPERF increases about 0.37%.
Observed execution-rate variation alone does not explain this change. Counters
remain whole-worker, guest-CPU and non-atomic; physical fidelity is unproven. These
are bounds and scope comparisons, not an exact additive CPU-time partition,
exclusive API cycles or proof of a particular cache mechanism.

The existing Linux/x86 VM exposes neither a usable CPU/uncore PMU nor resctrl
L3/MB information. Guest CPU partition isolation does not establish exclusive
physical cache or memory bandwidth. Cache/memory stalls, remaining internal
layout/state and physical-host interference therefore cannot yet be uniquely
separated. That requires a usable distinguishing observation or a controlled host
condition; repeating the unchanged matrix or adding speculative runtime changes
cannot provide it. All release gates and historical failures remain unchanged.
