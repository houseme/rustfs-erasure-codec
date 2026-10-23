# Completed combined-source Linux diagnostic

Frozen tool and candidate: `9f935eef9c1af721165b245709897f7c4b9acc37`.
Baseline: published 9.0.0, `9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`.

**Collection and independent replay are complete; performance/release approval is withheld.**

All 1,471 calibration/profile workers exit successfully: 483 baseline and 484
candidate calibration workers retain 11,604 observations; the 504 profile workers
retain 6,048 observations. Paired layouts add 3,072, for **20,724 total**. The
adaptive calibration algorithm determines batch counts; the earlier campaign's
966/1,470 counts are not imposed on this new source.

The frozen tool independently recomputes the complete report as an exactly equal
JSON value. All 1,499 original inputs remain unchanged. This replay runs no codec
workload. Sources are restored, all four cleanup checks pass, both original observer
services are active, and owned partitions are removed. These facts do not turn a
diagnostic-only manifest into accepted release evidence.

## Complete retained refusals

| Check | Count | Failures |
| --- | ---: | ---: |
| Profile distributions | 504 | 2 |
| Profile repeats | 252 | 10 |
| Regression programs | 16 | 16 |
| Unqualified cross-version metric comparisons | 504 | See every comparison |
| Layout distributions | 256 | 0 |
| Layout within-phase repeats | 128 | 0 |
| Layout cross-phase repeats | 128 | 26 |
| Unqualified layout comparisons | 256 | 0 regression-threshold refusals |
| Short profile/layout samples | All | 0 |

All sixteen program exits are verified threshold refusals. Hotspot checkers emit
their four original failures per pairing as human-readable stderr; empty stdout
is not an execution failure. Structured reports and human thresholds are checked
against the complete numerical comparisons, without changing limits or inputs.
No earlier failed observation is replaced and no unchanged campaign is rerun.

The two width failures are candidate B1 `reconstruct_opt:4:2:65536` (15.7976%)
and B2 hotspot `reconstruct_data_missing_2_data` (13.4103%). The complete
[derived artifact](../benchmarks/release-9.1.0/2026-10-24/combined-complete-diagnostic-review.json)
contains every distribution, repeat, original program comparison, case and layout.

## Parent windows, service and fault counterexamples

All 504 profile parent intervals join to 47,891 host snapshots. The final
post-capture entry has no host payload and is an end marker. Host snapshots do
not provide absolute inner-API timestamps. Calling-thread service brackets wall
windows; hotspot windows include both the baseline and candidate operations,
while their primary metric covers only the candidate operation.

Of 126 cases, 65 have zero measured faults throughout; 85 retain exactly equal
nine-sample minor/major-per-iteration sequences across all four phases. All 126
recorded workload witnesses match. Caller fixtures are actually recorded for
103 cases and match there; the other 23 are untracked, not proven equal layouts.
Dynamic destinations and physical/cache residency remain outside that witness.

Seven small-file repeat failures have matching recorded caller fixtures and zero
measured faults. Their signed latency changes closely track calling-thread run
changes. The remaining three hotspot repeat failures also retain their larger
process windows and the two-operation timing limitation. These are counterexamples
to treating measured page faults as the universal source of repeat drift.

The persistent reconstruction/stream investigation priority also remains:

- `reconstruct_data:10:4:65536`: candidate B1/B2 has 192 minor faults per iteration;
  baseline A1/A2 has zero.
- `reconstruct_data:4:2:65536`: candidate 64, baseline zero.
- `verify_stream:10:4:65536:65536`: candidate 256, baseline zero.

The artifact retains normalized ranges for every case, not only these three.
Process windows contain clone/recovery/drop and all process threads; they do not
locate a fault at a particular allocation or prove an allocator-only cause.
Low guest runnable wait, pressure or steal does not establish physical-host
isolation, fixed frequency, cache/memory service or virtual-counter fidelity.

## Every profile/API group, without qualification

Ranges below cover all four source pairings and every case in each group.
They express candidate/baseline latency (inverse throughput where appropriate).
Passing a local case does not override a failed complete campaign.

| Profile / operation | Comparisons | Unqualified latency ratio range |
| --- | ---: | ---: |
| `hotspot/reconstruct_data_32x16_missing_2_data` | 4 | 1.49613–1.57992 |
| `hotspot/reconstruct_data_missing_1_data` | 4 | 0.92657–0.98140 |
| `hotspot/reconstruct_data_missing_2_data` | 4 | 1.20513–1.35843 |
| `hotspot/reconstruct_data_missing_data_plus_parity` | 4 | 0.99320–1.02352 |
| `hotspot/reconstruct_some_32x16_required_2_of_4_missing_data` | 4 | 1.48370–1.53051 |
| `hotspot/reconstruct_some_required_1_of_2_missing_data` | 4 | 0.97097–1.01004 |
| `hotspot/reconstruct_some_required_2_of_3_missing_data` | 4 | 1.17645–1.25967 |
| `hotspot/reconstruct_some_required_data_and_skip_parity` | 4 | 0.95321–1.01775 |
| `small_file/encode` | 44 | 0.71652–1.07068 |
| `small_file/reconstruct` | 44 | 0.67897–2.99508 |
| `small_file/reconstruct_data` | 44 | 0.88607–12.00609 |
| `small_file/reconstruct_opt` | 44 | 0.75280–2.95029 |
| `small_file/reconstruct_shard_slot` | 44 | 0.61522–1.02740 |
| `small_file/reconstruct_some_data_only` | 44 | 0.71722–1.01374 |
| `small_file/verify` | 44 | 0.92866–1.07960 |
| `small_file/verify_with_buffer` | 44 | 0.97272–1.16489 |
| `smoke/encode` | 12 | 0.96989–1.00194 |
| `smoke/reconstruct` | 12 | 0.70002–1.05281 |
| `smoke/reconstruct_data` | 12 | 0.70512–1.06013 |
| `smoke/update` | 12 | 0.92106–1.34251 |
| `smoke/verify` | 12 | 0.97277–1.01145 |
| `stream/encode_stream` | 20 | 0.90508–2.22483 |
| `stream/reconstruct_stream` | 20 | 0.81686–1.09791 |
| `stream/verify_stream` | 20 | 0.61098–7.70235 |

## Release boundary

These observations remain bound to frozen `9f935ee`. Later matrix, GF16 and
tool changes are not silently substituted, even where individual source files
match. The new [GFNI direct-map candidate](gfni-direct-affine-review-2026-10-24.md)
has separate correctness and component evidence. Its source arithmetic reduction
does not explain or erase the allocator/residency and no-fault residuals above.
Final reviewed source/lock/harness binding, complete accepted evidence and an
actual strict positive release preflight remain required. PMU/fixed-frequency
proof or unique explanation of every historical failure is not a new gate. No
tag, registry upload or automatic PR merge is performed.
