# 9.1.0 publication blockers and completion criteria

Borrowed parity rows (2026-10-25): runtime `31ccf7e` on main `e758f11` replaces
per-entry parity-row reference tables with a static borrowed matrix range. The
[complete source-bound review](borrowed-parity-rows-review-2026-10-25.md) retains
all 270 controls: 162 wide cases decrease from 1 / 4 / 10 / 33 allocation requests
to zero, and 108 small controls remain zero. Fresh 573 parent / 268 no_std / 317 tool
tests, published-9.0.0 and WASM compatibility pass; package inventories remain 71/6.
Separate recovery PR #106 is not included. Allocation evidence is not end-to-end
throughput or release approval; final combined source, accepted full performance
evidence and strict positive preflight remain outstanding.

Distribution compatibility (2026-10-25): merged main `4353935` has the same
complete Git tree as executed `bfac8fc` / runtime `4aa4ee0`. The
[artifact-level record](inverse-work-review-2026-10-25.md#distribution-and-consumer-compatibility)
verifies the 71-member core crate, extracted 395 library / 266 no_std tests,
real WASM generation/typecheck, 14 Node tests in both built and extracted NPM
packages, offline consumer exports/recovery/types and the successful Node/Chromium
CI job. Sixteen original inputs are independently replayed unchanged without
codec calls. No registry upload or performance approval is produced; the separate
26-command native continuation and final release gates remain outstanding.

Direct inverse work (2026-10-25): source `4aa4ee0` removes a temporary identity
matrix while preserving Gaussian arithmetic and row widths. The
[source-bound review](inverse-work-review-2026-10-25.md) records six cold-recovery
allocation controls (10 to 9 requests), 569 local codec parent tests / 266 no_std,
317 tool tests, published-version compatibility and 585 native parent tests.
The native pipeline has eight completed commands, then a preserved strict
ownership refusal caused by a foreign cargo task; its restored lease is closed.
The remaining 26-command continuation is prepared with zero launches while
resources are busy. No complete native, performance or release acceptance is
claimed for this new source.

Combined native integration (2026-10-24): source `8ec2dde` merges main
`76897f6` and passes all 31 Linux functional commands: 582 parent tests,
six isolated child executions, 265 no_std, six-backend published-version recovery,
978 cache-layout checks and 18 warmed zero-allocation cases. The
[current native record](recovery-cache-selected-survivors-2026-10-24.md#combined-native-validation-2026-10-24)
includes the complete independent replay of 74 unchanged inputs and verified
resource/lease closure. Current local Rust tooling passes 312 tests. There are
zero performance observations; accepted complete ABBA/layout evidence and actual
strict positive preflight remain outstanding.

Prior recovery-cache candidate (2026-10-24): runtime `b0bd342`, integrated
source `d138103`, keys inverse reuse by the selected-survivor erasure prefix.
The [cache review](recovery-cache-selected-survivors-2026-10-24.md) records real
before/after allocation regressions, 978 independent layout checks, 565 local
parent codec tests / 264 no_std, published-format compatibility and 310 native
Rust tool tests. Its separate 31-command Linux plan was superseded before any
launch; the combined-source result above retains its own binding. No new latency or release acceptance is claimed; earlier proofs
and refusals below keep their own source bindings.

Current native proof (2026-10-24): main `544bb06`, including `727690f` GFNI
batching, now passes its own 29 Linux commands (575 parent codec tests / 261
no_std), both native row widths and six-backend published-format/selective
compatibility. The [batched-native review](gfni-batched-native-review-2026-10-24.md)
records 82-input exact replay, shared-resource controls and the initial ACK
publication failure plus explicit atomic-ACK recovery without native retries.
Zero performance observations were collected. Final accepted complete evidence
and strict positive release preflight remain outstanding; earlier source-bound
refusals and snapshots below remain unchanged.

Latest synchronized main (2026-10-24): `727690f` adds four-vector GFNI matrix
reuse after the `3a62879` native snapshot. The [subsequent integration review](selective-parity-planning-2026-10-24.md#subsequent-main-synchronization-four-vector-matrix-reuse)
records fresh local dispatch/recovery/identity/buffer, no_std and cross-target
compile checks on merge `941489c`. It does not transfer the earlier native
execution or rejected timing results to the batching revision. Final native
source binding, accepted complete performance evidence and strict positive
release preflight remain outstanding.

Reviewed native snapshot (2026-10-24): `3a62879` includes the independently added
GFNI row-fusion runtime `c558ef8`, so it differs from frozen timing candidate
`b1a10e4`. The [later-main review](selective-parity-planning-2026-10-24.md#later-main-independent-gfni-row-fusion)
records 29 fresh native checks (575 parent codec cases / 261 no_std), positive
execution of both row-kernel widths, six-backend published compatibility and
43-input independent replay. Package lists are now 71/6. These are functional
results, not this later runtime's throughput acceptance. The completed old72
API comparison still has eleven repeat refusals and cannot be relabeled.
Final accepted complete evidence and strict positive release preflight remain
missing; older inventories below retain their source bindings.

Historical API diagnostic result (2026-10-24): the complete 72-case A1/B1/B2/A2
comparison of frozen `e3a1c1c` / `b1a10e4` has finished. All 288 rows / 864 samples
and zero execution errors independently replay exactly, with 46 inputs unchanged.
Eleven of 144 repeat checks fail (eight baseline, three candidate); all allocation
bookends repeat, but the whole catalogue remains unqualified for latency and
allocation attribution. The [complete selective recovery review](selective-parity-planning-2026-10-24.md#complete-72-case-api-result-repeatability-refusal)
retains every case, every ratio and the passing-target counterexample. No prior
measurement is relabeled and no unchanged retry is used. Final accepted full
published-baseline evidence and strict positive release preflight remain missing.

Historical selective-planning candidate (2026-10-24): `b1a10e4` is based on
main `e3a1c1c`. The [selective parity review](selective-parity-planning-2026-10-24.md)
records the systematic-row substitution, `k*d` rather than `k²` planning products
per requested parity row, independent dense oracles and 28 positive native Linux
commands. Published-format/selective publication checks pass across six backend
settings. Actual package lists are now 69/6, retaining the same include rule.
These are functional/work-count results; final accepted full ABBA/layout and
strict positive release preflight remain missing. Every older measurement and
validation inventory below keeps its original source and counting convention.

Historical integrated native review (2026-10-24): PR #84 has been externally
merged. Executed source `43d678b` integrates main `0b10e7a`; reviewed main
`801f499` has identical codec/test/build/WASM/workflow files and newer tool files.
The [integrated native proof](gfni-tower-fusion-review-2026-10-24.md#integrated-main-native-compatibility-and-static-attribution-review)
records all 21 Linux checks (562 parent codec cases / 252 no_std), six-backend published-format
compatibility and the exact inverse/quotient comparison. All six earlier tower
cohorts remain unaccepted. Static selected masks/tables match; caller geometry
and execution-cost attribution remain unqualified. No final accepted full
ABBA/layout evidence or strict positive preflight is recorded. Older source
inventories and validation counts below retain their original bindings.

Historical rebase review (2026-10-24): PR #84 now integrates main `0b10e7a`,
including the GF16 norm, direct GFNI and portable accumulation/layout changes.
The [rebase validation record](gfni-tower-fusion-review-2026-10-24.md#rebase-integration-2026-10-24)
binds fresh ARM64 correctness/compatibility and x86 compile-only checks to
source `f3b7145`. Earlier source identities and rejected measurements below
remain historical; functional integration grants no performance or publication
approval. New-head Linux CI remains a separate result.

Historical source review (2026-10-24): runtime `e38a8ad`, final validated test source
`7fa4fdd`, in draft PR #84 stacked on #81. These candidates are not merged into
reviewed main `afb9a52`. The [complete tower review](gfni-tower-fusion-review-2026-10-24.md)
retains all six rejected component cohorts; the final same-42-case comparison
has zero width/short failures but twelve repeat and ten regression refusals.
Compatibility and 21-command native validation pass; accepted final full-release
ABBA/layout evidence and strict positive preflight remain missing. Historical
source inventories below retain their original scope.

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

## GF16 tower fusion and cached execution

The [tower review](gfni-tower-fusion-review-2026-10-24.md) retains the complete
first source's twenty-case comparison: no width/short failures, ten repeat
refusals and one scalar-control regression refusal. It is not accepted.
Cached execution source `678ddd6` also completes and independently replays:
no width/short failures, nine repeat and two regression-case refusals, including
unchanged GF8 control drift. It remains unaccepted. Follow-through `2f16f8b`
caches only optional native functions and retains static portable calls. Its
complete independent replay now has zero width/regression/short refusals, but
five repeats still fail, so the complete component comparison remains rejected.
Both scalar controls pass their repeat/regression limits in this cohort. Local
correctness, wire compatibility and ordinary CI do not qualify throughput or
strict release. All three cohorts remain source-bound. Combined runtime `7914ac4` explicitly
integrates PR #83 portable accumulation/AVX2 layout with the optional native
cache, with twenty positive Linux functional commands and exact published-9.0.0
compatibility across six backend settings. It has no timing observations of its
own; older refusals are not relabeled. Final accepted source-bound performance
and strict positive preflight remain outstanding.
The [new integrated-source plan](../benchmarks/release-9.1.0/2026-10-24/gf16-integrated-component-plan.json)
retains all controls and adds portable-backend codec workflows: 42 cases / 168
ABBA workers, with all original limits preserved. It does not replace complete
published-version release geometry or any earlier refusal.
That comparison now completes and independently reproduces every record: one
width, eleven repeats and five regression cases refuse the original limits,
with zero short/execution failures. It remains unaccepted. The SSSE3 1,025-element
overwrite case is slower in all four pairs despite passing its own width/repeat
checks; unchanged GF8/native controls also drift, so no universal cause is claimed.
Width-coordination runtime `0a35491` (final test source `986c1fe`) now caches
layout eligibility with the selected arithmetic backend. All twenty native
functional commands, explicit width markers and published compatibility pass
in a new capture; the original split-marker capture remains failed and retained.
Its complete 42-case comparison now independently reproduces 1 width / 17 repeat /
6 regression refusals, with zero short/execution failures. The former SSSE3
case is lower in all pairs, but all eight scalar cases are stably slower. Both
the benefit and cost remain recorded; the entire cohort is not accepted.
No earlier refusal or release approval is reassigned to this new source.
Runtime `e38a8ad` preserves scalar wide permutation after all eight stable scalar
costs; final test source `7fa4fdd` isolates backend environments in fresh children.
The original concurrent metadata failure remains retained. All 21 native checks
and twelve fixed parallel stress rounds pass; its own same-42-case comparison
now completes and independently replays all 168 workers / 2,016 observations.
Zero width/short failures do not qualify it: twelve repeats and ten regression
cases refuse the limits. All eight scalar cases are lower, but all eight SSSE3
cases are flagged despite unchanged logical selection/kernel source. Retained
code sizes/opcode sequences match while code/caller geometry remains unqualified;
no unique cause or release approval is inferred.

## GFNI component follow-through and cancellation fix

PR #82's identified-member exit confirmation passed 31 PR checks, with three
intentional release steps skipped, and was rebase-merged as `afb9a52`. Its full
tree equals the tested source. Encoding bytes and recovery publication are
unchanged; those skips are not strict positive release preflight.

The separate [GFNI component review](gfni-direct-affine-review-2026-10-24.md)
retains all 292 workers / 3,504 observations and the original final source-clean
failure caused by its own generated executables. Read-only replay exactly
recovers the original statistics without running new samples: one distribution,
11 repeats and one unqualified regression observation fail, with no short
samples. All 73 cases and four comparisons remain recorded. Post-run source and
resource audits do not rewrite the original failure or approve throughput.

## Current merged implementation

The [merged-candidate review](release-9.1.0-readiness-2026-10-23.md) now covers
reviewed main `0a85348`, including contiguous matrix rows, shared ordinary
Classic verification, empty aligned storage, concurrent cache metrics and the
newer bound-original/source-plan/process-owner checks. Its exact new functional
record passes 211 native, 527 all-feature codec (43 existing ignored), 239 no_std,
246 Python and the 200-case published-9.0.0 parity/cross-decoding comparison.
Strict codec/tool Clippy, Linux tool Clippy and preparation audits also pass.
These checks are not throughput measurements or strict positive preflight. It differs
from both the completed frozen `9f935ee` tree and earlier runtime `449920e`. The earlier `14cf56e` and
`09597e4` file-tree equality remains a historical fact; their ordinary CI and
package dry-run do not provide later-source performance acceptance. Strict
evidence validation, release preflight and registry publication were skipped.

New checks on `62632e5` pass: 357 codec library tests (five existing ignored),
237 codec-only no_std tests, 199 native tests and all 248 Python controller tests,
strict codec/tool Clippy and Linux tool metadata compilation. Version, workspace,
JSON and actual package allowlist preparation checks pass; these are not strict
release preflight. One subsequent `744f7e9` macOS cancellation job failed exit
confirmation; the old job is retained. Tool follow-through `5882adb` passes 205
native and 248 Python tests plus strict macOS/Linux tool Clippy and all 96 bounded
post-fix cancellation trials. Integrated `aa89274` preserves main's newer repair
and adds the shared proof/refusal tests; 205 native, 359 codec (five ignored),
237 no_std, all 248 Python tests and strict codec/macOS/Linux tool Clippy pass.
PR #67 subsequently completed both workflow sets with 62 SUCCESS / 6 SKIPPED;
the original failure remains retained. Those skips are not accepted evidence or
strict positive preflight. The separate full
Linux campaign on frozen `9f935ee` has completed; see the new full review below. The earlier 64-worker encode/stream plan has completed and
independently replays exactly. All
64 distributions and eight regression programs pass, but three of 32 repeats fail
(maximum 8.3833%); no sample is short. The [complete GFNI review](gfni-priority-review-2026-10-23.md)
retains all sixteen cases and failures. This unaccepted result cannot
be relabeled as a measurement of the later combined main. Final-source freeze,
complete accepted evidence and strict positive preflight remain required.

The subsequently merged [matrix-row change](matrix-row-review-2026-10-23.md)
retains both rejected local cohorts. A fresh independent replay of all 2,208
observations reproduces every report field and refusal, without codec execution.
The later cohort still has five width, fourteen repeat and three regression-case
refusals. Its integration does not approve those comparisons. The current
functional record and original matrix-source binding are explicit in the
[merged-candidate review](release-9.1.0-readiness-2026-10-23.md).

## Completed combined-source diagnostic and GFNI candidate

The [complete combined-source review](complete-linux-diagnostic-2026-10-24.md)
retains all 20,724 observations for frozen `9f935ee` versus published 9.0.0 and
an exactly matching independent frozen replay. Sources, four cleanup items and
two observer services are restored. Two profile distributions, ten repeats,
all sixteen regression programs and 26 cross-phase layout repeats still refuse
the original limits; all 504+256 comparisons remain unqualified. These results
do not bind later main or the new [direct GFNI affine candidate](gfni-direct-affine-review-2026-10-24.md).
That candidate has independent byte/compatibility tests and a completed component
comparison, whose refusals and original controller error remain preserved. Final
release acceptance remains outstanding. The [owner/residency follow-through](recovery-ownership-review-2026-10-24.md)
localizes selected historical fault increments before decoder entry and verifies
a zero-allocation warmed caller-reuse contract without changing the production
allocator or formal workload.

## Scalar unit-pivot follow-through

The [scalar unit-pivot candidate](scalar-unit-pivot-review-2026-10-23.md) is bound
to executed source `9e5ee94` after integration of main `8d59e77`. It omits remaining
known scalar pivot products while retaining field addition, GF8 normalization and
full byte-batch widths. A fixed fixture reduces scalar products from 10 to 7;
byte-path work is intentionally unchanged. All 530 codec / 239 no_std / 221 native /
247 Python tests and 200 normal-format published-version comparisons pass.
These are functional/work-count results, not accepted throughput evidence.
The existing complete diagnostic refusals remain unchanged; native Linux CI and
final-source performance/release acceptance still need their own actual results.

## GF16 norm inversion candidate

The [GF16 norm review](gf16-norm-review-2026-10-24.md) binds candidate `14b1b98`
to reviewed main `a773e74`. All 65,536 inverses and 327,680 quotients match both
the retained Euclidean algorithm and the actual published 9.0.0 crate; the
200-case byte/cross-decoding probe, 538 all-feature codec tests, 246 no_std tests
and strict codec Clippy pass. The complete local 29-case / 1,392-observation
comparison is rejected: 30 width, 25 repeat and five regression-case refusals,
with zero short samples. All cases and failures are retained. This arithmetic
refactor is a review candidate, not accepted codec throughput or Linux release
evidence; frozen `9f935ee` remains a separate source.

## 2026-10-23 adaptive verification candidate

The [complete affected-catalog review](verify-dispatch-review-2026-10-23.md) of
main `db69018` versus runtime `17432e5` retains all 28 workers / 336 observations
and an exact independent replay. Distribution, duration and repeat checks pass,
but one of eight regression programs rejects 4+2/64 KiB stream verification:
A1/B1 latency increases 15.0854% against the original 15% limit. All four
pairings are slower. This remains an unaccepted result for the original runtime.
The later shared GFNI-priority fix is merged, but its new complete affected-catalog
comparison is complete, with three repeat failures retained as described above.
Passing correctness, ordinary CI or merging PR #57 does
not establish a performance gain. Neither subset is the final 9.0.0 comparison
or release acceptance, and neither replaces historical complete-catalog failures.

## Latest complete diagnostic follow-through

The [complete Linux review](complete-linux-diagnostic-2026-10-22.md) supersedes
pending-collection status for frozen `022c0a7`: all 20,712 observations were
collected and the independent frozen replay exactly matches. The retained result
has one profile distribution failure, nine profile repeat failures, sixteen
program-level threshold refusals and sixteen cross-phase layout repeat failures.
It supplies complete diagnostic data, not accepted release evidence. All original
failures below keep their own dates, sources and scope. The separate fixed-count
mechanism controls have now completed all 16 workers / 192 observations with exact independent replay and no spread, duration or
repeat failures. Result retention does not remove the reconstruction contrast;
these targeted controls do not supersede the failed complete campaign. The
existing source-bound zero-codec capability probe also replays exactly. See the
completed-control and counter-ownership sections of that review.

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
| 6: Full performance acceptance | Incomplete | A stable complete baseline, then full final candidate/layout ABBA and strict positive preflight. Historical qualification rejected 8/126 repeats. The later complete diagnostic collected all phases/layouts and retained its distribution, repeat and regression failures; merged runtime changes and the prepared affected catalog do not supply final acceptance. |

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
