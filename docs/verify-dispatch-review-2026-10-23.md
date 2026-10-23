# Adaptive verification: complete affected-catalog review

Review date: 2026-10-23. This source-bound diagnostic compares main `db690184811584d96c8c3509f37a802156c5b61f`
with runtime change `17432e50f1fd2a790034e3de213ebf58c7d4635c` in
[PR #57](https://github.com/houseme/rustfs-erasure-codec/pull/57).
The complete result rejects performance acceptance: one original regression
threshold is exceeded. Keep the PR in draft while resolving that result.
No percentage speedup or 9.1.0 release approval is established.

The runtime removes verification's outer policy branch and delegates to the
existing verification/encoding dispatch. This eliminates a second policy decision
on the parallel path and exposes the checked generated encoding path to serial
adaptive verification. Correctness tests establish equivalent verification and
parity bytes; they do not establish that the new execution path is faster.

## Complete collection and independent replay

The frozen protocol-15 sampler is `022c0a7476e3d91bee33f86bf4ade55d27122949`.
Both measured sources have the same lockfile. Subsequent commits `10b2f65` and
`1cbdd8e` change tests only: production source, manifests, lockfile, build script
and Cargo configuration remain byte-identical to the measured runtime commit.
They must not replace its identity in the measurements.

All five original stream-verification cases and two unchanged encode controls
ran in phase-major A1/B1/B2/A2 order. All 28 fresh workers completed, retaining
336 observations: 84 warmups and 252 measurements. Each worker kept three
warmups, nine samples and its exact original common iteration count. No timing
loop, count, workload, threshold or observer changed during collection.

An independent replay read every original log and JSON record, validated the
full schedule, counts, workload witnesses and placement, reconstructed all 28
summaries and median rows, and reran all eight original regression programs.
All complete outputs match exactly. All 56 parent start/end events are present;
all original input files remained unchanged. Both source overlays and the frozen
tool were restored, all four resource-cleanup checks passed, and the original
observer services are active. Replay executed no codec workload.

All 28 distributions pass the original 10% central-width bound (maximum
3.7546%); all 14 same-version repeat checks pass the original 5% bound (maximum
4.4528%); no measurement is short. Seven regression programs pass; one returns
a genuine threshold failure. There are no worker or regression-execution errors.

## Every case and comparison

Medians below use nanoseconds per iteration for encode and per block for stream
verification. The ratio range retains all four candidate/baseline pairings
A1/B1, A1/B2, A2/B1 and A2/B2; smaller is faster.

| Case | Iterations | A1 | B1 | B2 | A2 | Four-pair latency ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Encode 10+4, 64 KiB | 73,728 | 35,869.35 | 35,825.71 | 36,331.38 | 35,882.57 | 0.998415–1.012881 |
| Encode 4+2, 64 KiB | 524,288 | 6,737.10 | 6,983.68 | 6,853.16 | 6,790.03 | 1.009297–1.036600 |
| Verify 10+4, 1 MiB / 1 MiB block | 1,792 | 4,429,876.29 | 4,417,520.46 | 4,487,316.22 | 4,418,940.78 | 0.997211–1.015473 |
| Verify 10+4, 16 MiB / 4 MiB block | 128 | 7,664,824.88 | 7,632,864.73 | 7,643,783.60 | 7,628,360.34 | 0.995830–1.002022 |
| Verify 10+4, 64 KiB / 64 KiB block | 45,056 | 444,651.80 | 446,365.78 | 437,409.98 | 450,065.59 | 0.971881–1.003855 |
| Verify 4+2, 1 MiB / 256 KiB block | 3,712 | 172,183.07 | 172,299.01 | 170,948.90 | 171,669.29 | 0.992832–1.003668 |
| Verify 4+2, 64 KiB / 64 KiB block | 176,128 | 13,858.13 | 15,948.69 | 15,590.50 | 14,475.20 | 1.077049–1.150854 |

The last case is slower in every pairing: +15.0854%, +12.5007%, +10.1794%
and +7.7049%. A1/B1 exceeds the unchanged 15% stream threshold. A passing
A2 pairing cannot replace this failure. The encode limit remains 12%.
The [derived JSON](../benchmarks/release-9.1.0/2026-10-23/verify-dispatch-affected-catalog.json)
retains every phase aggregate, all 14 repeats, all 28 comparisons and all eight
checker outcomes, including the failure.

## Subsequent fixed backend control

A separate predeclared control kept the same sampler, source pair and original
counts for 4+2/64 KiB verification and the unchanged 4+2/64 KiB encode control.
It compared automatic backend selection with the existing explicit
`rust-gfni-avx512` override. The override uses the same recorded ordinary GFNI
backend and disables generated encoding through the existing source contract.
Both conditions ran complete A1/B1/B2/A2 schedules: 16 workers, 192 observations,
48 warmups, 144 measurements, eight repeats and sixteen checker programs.

Independent replay exactly matches all original log/JSON pairs, 16 summaries,
32 parent events and all sixteen program results. Source and resource restoration
and the original observer services pass. All distributions, durations, repeats
and checker programs pass; maximum central width is 3.1087% and maximum repeat
drift is 2.5490%. This does not replace the previous seven-case threshold failure.

| Condition / case | A1 | B1 | B2 | A2 | Four-pair latency ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| Auto / encode | 6,730.17 | 6,848.51 | 6,964.76 | 6,901.72 | 0.992290–1.034856 |
| Auto / stream verify | 14,068.50 | 15,847.69 | 15,485.64 | 14,132.95 | 1.095712–1.126466 |
| Explicit GFNI / encode | 5,881.51 | 5,814.74 | 5,840.74 | 5,882.52 | 0.988478–0.993068 |
| Explicit GFNI / stream verify | 14,026.56 | 13,906.53 | 14,258.29 | 14,244.50 | 0.976274–1.016521 |

All measured process fault counts are zero. Workload witnesses match across both
conditions. Recorded caller fixtures match within each condition, but stream
fixtures differ between the automatic and explicit conditions. The control
supports investigating generated-kernel selection; this layout difference and
untracked internal/physical state prevent a unique ISA-only causal claim.
The [complete backend-control JSON](../benchmarks/release-9.1.0/2026-10-23/verify-dispatch-backend-control.json)
retains all four cohorts, every phase and comparison, and this counterexample.

The next candidate should preserve automatic GFNI backend priority at the shared
encoding eligibility boundary, then validate all affected encoding and verification
cases under automatic selection. Do not repair only the failing shape or introduce
a size threshold based on this control. The previous source pair and failed
report remain immutable; the new source requires its own correctness and timing
validation.

## Mechanism and limits

All seven cases have matching recorded caller fixtures and workload witnesses
across phases. The failing 4+2/64 KiB verification case and both encode controls
have zero measured process minor/major faults. The other stream cases retain
matching measured fault sequences across versions, including 256 minor faults
per iteration for 10+4/64 KiB. This result does not support a new page-fault
increase as the explanation of the 4+2 regression. Process counters still bracket
whole operation windows; equal counts do not establish identical internal
owners, physical residency or cache state.

Source review identifies a concrete dispatch hypothesis to distinguish next.
The recorded ordinary backend is `rust-gfni-avx512` in both versions. On x86,
`SelectedEncodeKernel::select` selects the separate generated AVX2 implementation;
that implementation uses nibble-table shuffles. Thus equal exported backend names
do not prove equal kernel execution. Delegating serial verification to encoding
can replace generic field arithmetic with this generated path. The capture does
not profile individual kernel costs, so this source-derived hypothesis is not a
proven unique cause. The subsequent same-backend control above disables generated encoding through
the existing override without changing the sampler or counts.
Do not add a shape-specific threshold, allocator override or temporary heap anchor
merely to make the retained failure pass.

This is a complete affected-catalog diagnostic against main, not a complete
published-9.0.0 comparison. Historical full-catalog failures remain retained.
The final candidate still requires a source/lock freeze, accepted complete ABBA
and paired-layout evidence, and an actual positive strict release preflight.
No tag or registry publication was performed.
