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
