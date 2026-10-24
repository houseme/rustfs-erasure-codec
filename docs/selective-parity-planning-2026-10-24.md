# Selective parity recovery: compose only unknown data rows

Reviewed base: `e3a1c1cffbbe3bcfbeecddce1de24e2b42888d1c`.
Executed candidate: `b1a10e4e6a164cb5f8fdc212a455f7130e2e5e39`.
The [complete aggregate result](../benchmarks/release-9.1.0/2026-10-24/selective-parity-planning-review.json)
records all 28 native checks, raw-replay outcomes and asserted work shapes. Historical captures keep their
original sources; this change does not reinterpret their statistical refusals.

## Execution path and algebra

`reconstruct_some` and adaptive/workspace callers share
`fill_classic_reconstruct_plan`. When parity is requested while data remains
missing, the planner composes each encoding row with the selected survivor
inverse. It must do this even when the missing data is unrequested, so recovery
can write exactly the selected outputs without publishing intermediate data.

Let `k` be the data count, `d` the missing-data count, `P` a requested parity row,
`D` the survivor inverse and `S[c]` the original shard index at input column `c`.
The old dense composition calculates `sum_i P[i] * D[i,c]` for every column,
using `k²` field products per parity row. Every present systematic data shard is
selected before parity, because survivor indices are sorted and there are at
most `k` data shards. Its inverse row is the unit row at its selected column.
The same coefficient is therefore:

```
(S[c] < k ? P[S[c]] : 0) + sum_(i in missing data) P[i] * D[i,c]
```

Only the unknown-data rows need multiplication: `k*d` products per requested
parity row. This applies to every existing Classic systematic constructor,
including custom matrices; it is not a shape/size/backend threshold. All missing
data contributes to the equation, independently of the requested-output mask.

The inverse builder/cache, numerical kernels, public API, validation and output
executors are unchanged. Planning still finishes before output initialization.
Singular survivor matrices keep the same error; a no-output request keeps its
existing validated no-op. The change adds no allocation or unsafe code. Ordinary
encoding, ordinary full/data recovery and previously prepared reusable plans do
not execute this composition branch and receive no new speedup claim.

## Deterministic work and compatibility checks

A counted, algebraically valid wrapper field isolates composition after warming
the existing inverse-cache entry. The original production implementation first
fails the new work bound with 64 actual products versus eight expected. The new
implementation passes all declared shapes; independent dense-inverse tests cover
all small survivor masks in both fields/three matrix modes, shifted large losses,
custom singular/zero-coefficient rows and wide general tower coefficients.

| Data / missing | Previous dense formula | New asserted products per parity row |
| --- | ---: | ---: |
| 8 / 1 | 64 | 8 |
| 8 / 2 | 64 | 16 |
| 8 / 4 | 64 | 32 |
| 8 / 8 | 64 | 64 |
| 32 / 1 | 1,024 | 32 |

The all-data-missing counterexample retains the same arithmetic work. These are
field-operation counts, not cycle counts or latency/throughput ratios.

The new published-9.0.0 test checks 270 exact-codeword configurations and 2,640
bidirectional full cross recoveries after selective repair per backend setting.
It spans GF8/GF16, all three matrix modes, 1/4/10/32 data shapes, a 2+255 tower
shape, ten vector/tail lengths, several loss counts and parity-only/mixed output
requests. Every unrequested missing data slot must remain absent. Published
full recovery reads candidate-repaired parity; current full recovery reads
published bytes. Existing generic late-failure, selective publication, identity,
stream/workspace and allocation regressions remain part of full validation.

## Completed source-bound validation

All 28 Linux commands pass on the frozen candidate, including five planner
oracles, both GFNI widths' execution markers, the AVX2 layout oracle, twelve
fixed backend-environment stress rounds and published-9.0.0 compatibility.
Native parent totals are 402 library / 160 integration / ten documentation
(572 passed, 43 existing ignored) and 261 no_std. Five isolated filtered child
test executions are counted separately. All six backend settings additionally
pass the new selective fixtures: 1,620 exact-codeword configurations and 15,840
cross recoveries in total. Existing 200 normal, custom 162 configurations /
972 recoveries and 72 wide configurations also pass.

Independent replay executes no codec, checks all 42 original/control inputs
unchanged and reproduces every command/summary/marker. Source copies remain
clean, all four owned cleanup conditions pass and both original observers are
active after completion. There are zero performance observations in this run.

ARM64 passes 389 library / 162 integration / ten doc parent tests (561 passed,
43 existing ignored), 261 no_std, all 257 native tool tests and all 252 Python
tests. Six isolated child executions are separate. Strict codec, compatibility
and tool Clippy, x86 all-target compile-only Clippy, fmt/typos/diff pass. Commit
source hashes match the exact worktree bytes used by these local checks.

Actual core/WASM package lists are 69/6. Main's earlier decode-matrix test module
and this candidate's planner test module explain the two additional source
members versus the earlier 67-file inventory; the manifest include rule is
unchanged. No package is uploaded.

## Acceptance boundary

This is a reduction in reconstruction-planning arithmetic. It is not a measured
codec throughput result or a unique explanation of earlier SSSE3/control drift.
No old source, workload, sample, count, threshold or failed comparison is changed;
no unchanged-source performance rerun is used. Final source/tool/lock binding,
accepted complete published-baseline ABBA/paired-layout evidence and actual
strict positive release preflight remain required. No tag, registry upload or
automatic PR merge is performed; automation remains paused.


## Main integration and complete API diagnostic

Integration `5787367` merges reviewed main `9c426f7` into the open candidate PR.
Codec/test/benchmark/build/lock/WASM and compatibility sources remain byte-equal
to executed `b1a10e4`; newer native paired-campaign tooling has a separate full
validation: 260 native tests (231 unit / 29 CLI), all 252 Python tests, strict
tool Clippy, fmt/typos/diff, with its inputs unchanged. This source relation does
not relabel the exact-source functional or performance records.

The [fixed diagnostic plan](../benchmarks/release-9.1.0/2026-10-24/selective-recovery-paths-abba-plan.json)
compares frozen baseline `e3a1c1c` and runtime `b1a10e4` using their byte-identical,
unmodified `recovery_paths` program. All 72 cases run in each A1/B1/B2/A2 phase:
288 rows / 864 samples, with every original case retained. The affected Classic
GF8 selective-parity case is accompanied by all 71 encoding/recovery/verification/
constructor controls. Leopard GF16 controls are not Classic GF16 selective-parity
time measurements.

This separate program preserves its original twenty warmup calls, one all-thread
allocation-count invocation, and three independently time-targeted 1,000 ms
samples in eight-operation batches. It is not the formal three-warmup/nine-sample
collector; its adaptive counts are not shared between sources. No formal sampler,
case, threshold or timing loop is edited. The existing 5% bookends and allocation
bookend repeatability apply to the entire catalogue; all raw medians and four-way
comparisons are retained, and any global refusal withholds all qualified ratios.
No selected-case or unchanged-source timing retry is planned.

Before launch, source/program path lengths and argv0 are equalized. The stock
program does not parse argv, but reads Git metadata before constructing fixtures.
Its two short source strings had different lengths, so benchmark children report
full forty-character revisions through a process-local Git option; repository
and system Git configuration remain untouched. This removes a concrete metadata
length difference without claiming it explains old drift or observes dynamic/
physical fixture addresses. Constructors retain the original eight visible guest
CPUs and four Rayon workers; no new main-thread pin hook changes their budget.
Guest reservation does not prove physical host isolation or fixed frequency.

At the launch checkpoint, the plan had launched once after strict resource admission. No result is assigned
before all four full-program workers, embedded content oracles, raw input replay
and cleanup have been checked. Statistical refusals never truncate later phases.
Execution errors retain partial outputs and allow later planned phases to run;
they are not renamed as statistical refusals. Results were pending at that checkpoint; the completed review below supersedes
that status. The earlier 28-command correctness proof and every historical failure
retain their original source/scope.


## Complete 72-case API result: repeatability refusal

The [complete derived result](../benchmarks/release-9.1.0/2026-10-24/selective-recovery-paths-abba-review.json)
retains every case and all four cross-source comparisons: four full-program
workers, 288 rows and 864 samples, zero execution errors. Every embedded content/
publication oracle completes; source/build/lock/context checks and all four owned
cleanup conditions pass, with both original observers active after completion.
Independent replay checks all 46 inputs unchanged, eight parent events and every
raw field/report outcome, reproducing the full report exactly without running a
codec. The launch declaration remains immutable at its original checkpoint.

The **entire comparison is not qualified**. Of 144 five-percent bookend checks,
eleven fail: eight baseline A1/A2 and three candidate B1/B2, affecting ten cases.
Maximum baseline/candidate drift is 16.8307707% / 12.4699008%. All 144 allocation
bookends repeat, but the original global rule still withholds both latency and
allocation attribution. Every qualified ratio remains null. Failed controls are:

| Case | Baseline repeat | Candidate repeat |
| --- | ---: | ---: |
| `verify.caller_buffer/10x4/32768/missing0+0/block0` | 5.7797719% | 2.9297165% |
| `classic.reconstruct_parity.cached/32x16/4096/missing0+1/block0` | 16.8307707% | 2.6504547% |
| `classic.reconstruct_parity.uncached/32x16/4096/missing0+1/block0` | 15.2472944% | 0.9183338% |
| `classic.reconstruct_data_slots/10x4/32768/missing1+0/block0` | 4.6673210% | 5.1279458% |
| `classic.reconstruct_data_opt/10x4/32768/missing1+0/block0` | 6.1278877% | 3.1438333% |
| `encode.serial/10x4/1024/missing0+0/block0` | 6.8350480% | 0.3080019% |
| `encode.serial/12x4/1024/missing0+0/block0` | 8.5001029% | 1.6801894% |
| `encode.serial/8x4/1024/missing0+0/block0` | 7.5424696% | 3.6969078% |
| `encode.serial/8x4/32768/missing0+0/block0` | 3.7103827% | 5.8349571% |
| `encode.serial/4x2/1024/missing0+0/block0` | 5.6912144% | 12.4699008% |

The targeted `classic.reconstruct_some_parity/32x16/65536/missing1+1/block0`
case has phase medians 25,937.2953 / 25,649.4999 / 25,531.1724 / 26,563.8740 ns.
Its own baseline/candidate repeats pass at 2.4157444% / 0.4613249%, with zero
allocation calls/requested bytes in all four resource observations. Its four
unqualified ratios are 0.9611238–0.9889042. These observations are kept beside
all unfavorable controls; they do not qualify an API improvement or convert the
asserted planning-product reduction into a proportional throughput gain.

All 1,235 retained host snapshots and the final post-capture marker remain
intact. There are 214 snapshots fully inside each parent phase, no recorded
outside workload, zero reserved-guest steal in those snapshot intervals, CPU
some-avg10 pressure at most 0.45 and memory pressure zero. I/O pressure varies
between phases. These are coarse whole-phase guest observations. The benchmark
has no absolute case/API timestamps, so no individual host reading is assigned
to one case; guest observations do not prove physical host isolation, fixed
frequency or a unique explanation of the refusals. Equalized launch metadata
and paths were insufficient for this complete comparison to pass; this neither
attributes old failures to those differences nor proves them irrelevant.

No source, sample, count, protocol, threshold or old refusal was rewritten, and
no selected-case/unchanged-source retry follows. The operation-count and
compatibility proof remains valid on its own source/scope. Final source/tool/
lock freezing, accepted complete published-9.0.0 ABBA/paired-layout evidence and
actual strict positive release preflight remain outstanding. This unpublished
main-versus-candidate diagnostic supplies none of that missing approval.


## Later main: independent GFNI row fusion

Main `3a62879` also includes runtime `c558ef8`, introduced independently while
the frozen API comparison was running. Versus `b1a10e4`, it changes the core
serial coding entry, adds the GFNI row kernel/test modules, registers that
module and updates backend wording/native-preparation tests. Benchmark source,
manifest/lock/build configuration and WASM remain unchanged, but the runtime
is different. Neither the old performance source labels nor its failed result
is reassigned to this later main.

Source review follows checked coefficient/data/output views into the selected
kernel, complete dimension validation, register accumulation and scalar tails.
The current Result errors occur before the first output write; the arithmetic
callback returns no later Result that could trigger a post-write algorithm
fallback. The selected immutable GF8 backend controls ISA choice. Other
backends retain fallback. Full Field byte-view opt-in is checked, not inferred
from element size. The full-vector loop stores once per coding row/vector;
that source-level observation does not count DRAM traffic or establish latency.
No reportable byte/bounds/output-validation defect was confirmed in this review.

The [new exact-source native proof](../benchmarks/release-9.1.0/2026-10-24/current-gfni-rows-functional-review.json)
binds actual source `3a62879`: all 29 Linux checks pass. Dedicated GFNI row tests
execute both AVX2 and AVX-512 paths, compare scalar coefficients/groups/offsets/
tails and reject malformed views before writes. Native parent totals are 405
library / 160 integration / ten doc (575 passed, 43 existing ignored), plus five
separate child executions and 261 no_std. All six backend settings retain the
200 normal, custom 162 configurations / 972 recoveries, 72 wide, and selective
1,620 configurations / 15,840 cross-recovery checks. Independent replay checks
43 inputs unchanged and all raw summaries/markers exactly, with zero codec
calls. Source/owned cleanup and the two original observers restore.

Fresh local checks on documentation source `66601ef` have byte-identical codec,
tests, build, tools and workflows to that main: 561 ARM64 parent codec tests,
261 no_std, all 260 native tool tests and all 252 Python tests pass. Strict
codec/tool and x86 compile-only all-target Clippy, fmt/typos/diff pass. Actual
core/WASM package lists are now 71/6 because of the two added source modules;
older 69/6 and 67/6 inventories retain their original source scope.

This new native proof has zero performance observations. The entire old72 API
refusal and the new runtime's own missing performance acceptance both remain
visible. A later intended candidate/tool/lock freeze still needs its own
accepted complete published-version data and actual strict positive preflight;
no performance result is inferred from an external PR merge or ordinary CI.


## Subsequent main synchronization: four-vector matrix reuse

After the native snapshot above completed, main `727690f` incorporated the
separate GFNI batching revision. Merge `941489c` brings it into this results
branch without changing the frozen diagnostic or reassigning source labels.
Each matrix broadcast serves four adjacent vectors; selection retains the
preceding route below four vector widths. Review follows the four disjoint
output ranges, complete row accumulation, unaligned access bounds, remaining
full vectors and scalar tails. The checked chunk and Field conversion contracts
remain intact; no additional reportable correctness defect was confirmed.

Fresh local checks on this exact merge pass 35 parent tests across dispatch,
recovery-plan, identity and owned-buffer suites, plus two separate filtered
child executions, 261 no_std tests, strict codec Clippy, x86 all-target/all-feature
compile-only Clippy and formatting. Production/test/build/tool/workflow files
match that main; validation inputs are unchanged. The earlier 575-case native
proof belongs to `3a62879`, not this later batching implementation. No new native
ISA execution or performance acceptance is claimed for the batching revision.
Its own source-bound native checks and final accepted full release evidence
remain separate outstanding work.
