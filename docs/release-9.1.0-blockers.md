# 9.1.0 publication blockers and completion criteria

Audit date: 2026-10-22 (Asia/Shanghai). Reviewed base main:
`e671c0ac24f9deaf2ee2f1e5ebbd8358cd4d6133`, including the source-copy observation
implementation in [PR #47](https://github.com/houseme/rustfs-erasure-codec/pull/47).
The complete native inventory below is bound to candidate `26aa094` and published
9.0.0; the PR changes no production source, manifest, lockfile or formal sampler.
The manifests already declare 9.1.0. No accepted final timing campaign, positive
strict release preflight, tag or publication is recorded. This assessment keeps
historical measurements bound to their original sources.

The [current drift analysis](drift-analysis-9.1.0-2026-10-22.md) records the
Criterion fixture/output-consumption repairs, owned/in-place sparse/dense cases,
and lazy differential scratch that skips identical chunks. These are verified
mechanism/resource improvements, not a unique cause or cure of the Linux drift.
They require a new final candidate freeze; rejected timing campaigns remain
rejected under their original source bindings.

## Production progress and actual version differences

Recent production fixes include excess-shard error classification before stream
I/O, Leopard-compatible payload splitting, validation before progressive XOR
merges, and bounded GF8/GF16 exponentiation. The accompanying join fix permits a
small requested prefix when repeated borrowed shards exceed the platform's
aggregate `usize` range. These are functional changes, not just benchmark changes.
Normal field definitions, matrices and compatible encoded bytes are preserved.
Generic reconstruction still keeps decode-before-publication where required,
late failures and selective-output behavior.

The [version delta](9.0.0-to-9.1.0-codec-delta.md) records a source-bound normal-
format 200-case byte/roundtrip comparison and 15 allocation scenarios. It excludes
historically inconsistent fast/custom/mixed-entry codewords. Recorded allocation
requests per API invocation include:

| Scenario | Published 9.0.0 | Measured 9.1.0 source |
| --- | ---: | ---: |
| Classic warmed encode | 0 | 0 |
| Classic cached single-output recovery | 1 | 1 |
| Classic cold recovery | 6 | 2 |
| Leopard GF8 / GF16 warmed encode | 7 / 19 | 0 / 0 |
| Leopard GF8 / GF16 warmed recovery | 27 / 40 | 2 / 2 |
| Multiblock stream encode / verify / recovery | 41 / 40 / 52 | 8 / 10 / 10 |

These count allocation plus reallocation requests, not throughput, latency, RSS
or peak memory. The report names its measured source; later production fixes
must not be relabeled as that measurement. Independently downloaded
[resource counts from main 4f2799e](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39394001627/artifacts/12251702244)
confirm the same complete 15-case, 90-record request counts on that source. The
same [release-tooling job](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39394001627/job/118532102062)
passed all 200 normal-format byte/roundtrip cases and the actual package dry-run.
The real generated WASM Node/Chromium job also passed; linked #2852 is already
closed. These completed functional checks should not be repeated as missing
implementation. No accepted percentage throughput change between published
9.0.0 and the final 9.1.0 source is currently available.

## What the remaining drift actually establishes

The [complete service review](baseline-service-review-2026-10-20.md) retains the
sources, full catalogs and diagnostic scope. All retained baseline controls
compare **9.0.0 with itself**, never a candidate with its baseline:

| Complete control | Repeat failures above 5% | Retained failure |
| --- | ---: | --- |
| Protocol-15 phase-major A1/A2, all 126 cases | 3 / 126 | -6.5942%, +6.7256%, +5.1983% |
| Adjacent diagnostic A1/A2, all 126 cases | 2 / 126 | Encode 10+4/512 KiB: -13.1933%; recovery 4+2/64 KiB: +8.0321% |
| Fresh 4f2799e-tool A1/A2, all 126 cases | Not executed | A2 `reconstruct_opt:4:2:16384` exceeded the original 10% distribution limit |

The first two controls have no short samples or central-width failures. The
newest qualification retained all calibration/A1/A2 records, then refused the A2
distribution before its final repeat gate. Empty `repeat_checks` is not a passing
drift result. The adjacent schedule is diagnostic only and cannot replace formal
qualification; passing an older failure there does not repair any complete control.
[Current execution and refusal status](https://github.com/rustfs/backlog/issues/2897#issuecomment-6291797590)
keeps full raw measurements private and all historical failures intact.

These are historical controls. The subsequent [complete qualification on
2026-10-21](https://github.com/rustfs/backlog/issues/2897#issuecomment-6316335129)
retains 8,832 records: all 252 distributions pass, but 8/126 repeat checks exceed
5%, with a maximum of 10.7504%. It is rejected. The [forced diagnostic capture
ending 2026-10-22](https://github.com/rustfs/backlog/issues/2897#issuecomment-6316688014)
retains 13,104 calibration/A1 records. A1's `reconstruct:4:2:65536` distribution
is 20.6528%, exceeding 10%; B1/B2/A2 and layouts never start. Its complete repeat
and candidate regression gates are unexecuted. Neither report establishes a
9.1.0 regression or release acceptance. Reported resource restoration passed;
neither campaign has been rerun. The later authorized read-only replay of all
12 original observations for the failed `small_file` case reproduces 20.6527515206%
exactly. Calling-thread run-time central spread is 20.6357%, process faults are
zero, maximum runnable wait/wall is 0.0097354%, and wall-minus-timed gaps are below
0.000033%. These constrain the residual to execution service inside the original
operation, including caller clone/allocation/recovery/drop costs. The excerpt
does not revalidate complete calibration or external isolation, identify a unique
frequency/cache/host mechanism, or approve any candidate timing. See the
[read-only replay](drift-analysis-9.1.0-2026-10-22.md#linux-失败-case-的授权只读复算).

A later [Linux native control](drift-analysis-9.1.0-2026-10-22.md#linux-原生功能与有界服务验证)
on merged main `c687e24` passes eight incremental regressions, 200 normal-format
compatibility cases and 45 resource cases. Its fixed four-worker 9.0.0-only
control retains all 48 observations: distributions span 1.9517% to 2.6447%, and
unobserved U1/U2 repeat drift is 0.1113%. The two observed workers exceed their
predeclared 5% counter-alignment uncertainty bound (5.3021%/5.3169%); per-marker
execution-rate interpretation is refused. Zero-codec kernel-marker checks also
do not establish byte-count closure. All raw failures remain retained and owned
resources are restored. This single-case window neither repairs the failed full
controls nor supplies final baseline/candidate qualification.

In the failed cases from the first two controls, formal observations have zero
process minor/major faults and at most about 0.0147% calling-thread runnable wait
relative to wall time.
A fixed eight-worker diagnostic further bounds work outside original wall
windows below 0.311%; the four encode medians still span 15.344%, versus 2.167%
for whole-worker APERF/MPERF. Those virtualized, non-atomic guest counters cannot
serve as exclusive API cycles or normalize timings. The residual lies inside
original operation windows; measured wrapper cost, faults, wait and observed
execution-rate changes do not individually explain its magnitude.

Some earlier test fixtures and metadata contracts were defective and have been
repaired. Residual variability also occurs in unchanged 9.0.0 with unchanged
worker binaries and retained encode buffers. It is therefore neither evidence
of a 9.1.0 regression nor proof that all drift came from test code. Cache/memory
service, remaining internal layout/state and invisible physical-host contention
remain unseparated. Guest CPU reservation does not reserve physical cache or
memory bandwidth; the current VM lacks usable CPU/uncore PMU and resctrl data.

The subsequent fixed original-binary diagnostic includes all three anomalies from
complete replay, four fresh workers per case and 144 observations. Full observer
boundary heap checks, original counts/input/context, counter-window correspondence,
byte-identical worker and resource restoration pass. Guest execution-rate signals
correlate with some service changes, but buffered 4 KiB verification retains an
unexplained component. No unique cache, internal-layout or physical-host cause
and no new runtime defect is established by that diagnostic. Its
[scope and result status](https://github.com/rustfs/backlog/issues/2897#issuecomment-6292534181)
never approve a candidate.

**PMU access and fixed frequency are diagnostic options, not additional release
gates.** A usable scaling policy or hardware counter can help distinguish causes;
missing them limits attribution, not functional testing or release in principle.
Release still requires the original comparable-context, complete-catalog,
duration, 5% repeat, 10% distribution, regression and strict evidence/preflight
checks. Unique attribution of every historical failure is not required. A changed
condition must be concrete and recorded before another full campaign; do not
retry unchanged failures until one happens to pass.

## Current local Docker ARM64 status

The [2026-10-20 local ARM64 preflight](docker-arm64-preflight-2026-10-20.md)
completed all 72 existing recovery-path cases and 216 samples with the unchanged
runtime source and complete correctness checks. Raw measurements remain private.
Local OrbStack cannot provide a valid delegated exclusive cpuset partition:
the owned partition is invalid and ancestor writes return EPERM. Probe state was
restored; the formal collector still refuses the environment. The CPU identity
correction removes a separate x86-specific provenance assumption, without
relaxing isolation or any performance gate. No formal baseline/ABBA, layout or
release acceptance follows from this diagnostic.

## Backlog #2897 completion marks

Only condition 4 is checked within its declared normal-format scope. Keep the
[canonical issue](https://github.com/rustfs/backlog/issues/2897) open:

| Condition | State | Concrete remaining acceptance |
| --- | --- | --- |
| 1: Buffer entry/layout/lifetime | Partial | Caller geometry, ordinary-verify scratch and all 15 stream byte-owner cases are covered. The remaining codec-internal decode scratch, non-stream destinations and smoke/hotspot source scope now has a complete two-source native inventory and independent replay. Instrumented request/view evidence does not establish uninstrumented performance geometry or complete paired-layout acceptance. |
| 2: Calibration/measurement lifetime | Partial | All 126 original cases execute both structural calibration and measurement lifetimes in both source copies, with six additional decoder cases: 516 workers, 6,192 observations and 12,384 kernel iterations. Native replay and restored ownership pass; these bounded structural rehearsals do not qualify formal durations or final performance geometry. |
| 3: 1/4 KiB layouts | Partial | The paired supplement is implemented. Obtain real passing A1/B1/B2/A2 results for all 16 source/scratch offset combinations at each size, kept separate from original Vec cases. |
| 4: Compatibility | Complete in declared scope | Retain normal-format byte parity and ordinary/optimized, invalid-input and missing-shard behavior tests on the final candidate; do not expand the legacy compatibility promise. |
| 5: Protocol/source/rejection | Partial | The complete private dynamic inventory binds source/lock/harness/observer/context and independently replays every original row. Rehashed warmup-backend and stale-request-generation counterexamples are refused without changing originals. Final accepted performance capture/replay and its immutable source/evidence bindings remain outstanding. |
| 6: Full performance acceptance | Incomplete | A stable complete baseline, then full final candidate/layout ABBA and strict positive preflight. Latest qualification rejected 8/126 repeats; the subsequent forced diagnostic stopped at A1 distribution, before candidate phases or complete repeat/regression checks. |

## Release sequence and benchmark decision

1. Finish and review concrete functional fixes, run ordinary compatibility/tests,
   generated Node/Chromium WASM integration and package dry-run, then rebase merge
   passing PRs. Freeze the complete final source/lock/harness before final capture.
2. Complete the outstanding workload/layout acceptance above. Before another full
   baseline run, establish a distinguishing observation or a controlled condition
   addressing residual execution service or internal-layout variability. Do not
   add speculative runtime tuning or retry the unchanged failed matrix.
3. Qualify all 126 baseline cases under the existing 5% repeat / 10% central-width
   and duration gates. Then collect complete final baseline/candidate A1/B1/B2/A2
   and the required 16-layout paired supplement with matched compiler, features,
   backend, I/O, input, count, CPU and memory-policy context. Retain all failures;
   no favorable case/count selection, time normalization or threshold relaxation.
4. Verify the accepted immutable evidence data commit. At this audit,
   `RSE_RELEASE_EVIDENCE_REF` is absent. Set its full commit SHA, or supply manual
   `evidence_ref`, only after acceptance. Rejected audit snapshots are not inputs.
5. Run CI manually on final main with `release_preflight=true` and that full
   `evidence_ref`; verify actual success. Ordinary PR/main CI skips these strict
   evidence/preflight jobs, so green ordinary CI is insufficient.
6. Only then create the matching 9.1.0 tag and verify actual registry publication.
   A version in Cargo.toml, a package dry-run or an available secret is not upload.

Benchmark preparation and functional/load smoke tests can proceed now. A formal
candidate comparison cannot yet support release attribution on the unchanged VM:
its baseline self-control fails. A new controlled condition must first pass the
baseline prerequisite. Existing workers and gates can then be used directly;
no additional general-purpose diagnostic framework is needed. A RustFS/S3 stress
comparison is a separate downstream workload, not a substitute for codec ABBA.

Native ARM and downstream/S3 throughput remain independently tracked in
[#2853](https://github.com/rustfs/backlog/issues/2853). Docker ARM and compile/mock
checks cannot certify native throughput. Those P2 tasks can remain separately
tracked only when actual release gates pass and release notes avoid unsupported
speed claims; they do not waive #2897 or the retained performance gate.

## Historical audit records

Earlier rejected controls remain auditable under their frozen identities:
[protocol 10](https://github.com/houseme/rustfs-erasure-codec/tree/75456a77a57953f8b069bab3d2eb788643ca21d1/diagnostics/protocol10-qualification-controls),
[protocol 13](https://github.com/houseme/rustfs-erasure-codec/tree/dc05250/diagnostics/protocol13-baseline-qualification).
Use the [evidence procedure](release-evidence.md) for current commands and exact
contracts. No historical rejection is promoted by documentation or code merges.
