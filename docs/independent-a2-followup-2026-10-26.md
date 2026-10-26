# Interrupted full campaign and independent A2 follow-up

## Outcome and source identity

The performance and 9.1.0 publication gates remain open. This review is bound to
main `07f02df5f78ada84588a6f4fc737b7fb484d33d1`, frozen tool/candidate
`4bd12089c9a1b9aa1f136de6473e3821f9d9b9d8`, and published-9.0.0 baseline
`9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`.
The [complete derived record](../benchmarks/release-9.1.0/2026-10-26/independent-a2-followup.json)
retains all 126 case keys, every available phase median/width/short observation,
and every baseline/candidate repeat diagnostic. No favorable subset is selected.

The original controlled campaign completes A1, B1 and B2, then stops after
85 successful A2 workers. It remains a failed attempt. A separately authorized
A2 rerun completes all 126 cases with the original frozen iteration tables,
three warmups and nine measurements per case: 378 warmups and 1,134 formal
observations, with zero execution errors. The new reservation is a separate
resource episode; it does not replace the interrupted original A2 or complete
the original ABBA sequence. The paired-layout supplement was never reached.

## Original failure and resource closure

The original continuous watcher latches a process-visibility refusal after
a process identity read reports `FileNotFoundError`. That refusal lists no
foreign heavy process; the lease journal has no deadline-reached event. The
owned sampler is canceled during `small_file / verify:4:2:1024`. The available
record does not establish why identity visibility failed or justify changing
the guard's fail-closed policy.

All 85 completed original A2 workers are retained. The interrupted worker
also retains six timing records, including three measurement intervals, but
exports no complete sample worker. These partial records are not assigned a
nine-sample median.

Both runs restore services, CPU partitions, controller state and source
cleanliness, then independently verify their archives before acknowledgement
and lease closure. Read-back confirms the canonical holder is closed and no
release partition remains. All 157 original-attempt members and 203 independent
A2 members retain their exact archived bytes. Original failures and raw records
remain preserved.

## Complete distribution and repeat checks

The frozen protocol retains the 10% central-width, 5% repeat-drift and original
duration rules. Width uses the second through eighth sorted measurements,
divided by the median of all nine. Each median keeps its actual original row.
Short measurements remain failures rather than being removed.

| Record | Complete workers | Width refusals | Short measured samples |
| --- | ---: | ---: | ---: |
| Original A1 | 126 | 0 | 0 |
| Original B1 | 126 | 0 | 0 |
| Original B2 | 126 | 1 | 0 |
| Original A2 before interruption | 85 | 1 | 0 |
| Independent A2 | 126 | 1 | 0 |

The independent A2 refusal is `small_file / verify:4:2:262144`, with central
width 13.7754%. The original B2 refusal is
`reconstruct_shard_slot:4:2:65536` at 19.1537%; the completed original A2
refusal is `reconstruct_some_data_only:4:2:65536` at 36.4584%.
All other cases and their complete values remain in the derived record.

| Profile | Original B1/B2 repeat refusals | Original A1 vs independent A2, descriptive only | Available original A1/A2 repeats | Original A1/A2 refusals |
| --- | ---: | ---: | ---: | ---: |
| Smoke | 2/15 | 0/15 | 15 | 0 |
| Small file | 7/88 | 21/88 | 70 | 9 |
| Recovery hotspot | 3/8 | 2/8 | 0 | 0 |
| Stream | 0/15 | 0/15 | 0 | 0 |
| Total | 12/126 | 23/126 | 85 | 9 |

The 23 cross-episode differences describe reproducibility; they do not repair
the original baseline gate. Nine of the 85 baseline pairs available within the
original episode already exceed the unchanged limit. Twelve candidate repeat
failures independently remain. No candidate throughput benefit, regression
cause, RSS result or release acceptance is attributed from this record.

## Independent replay and current-main boundary

A separate arithmetic reader recomputes all 589 complete workers across the
original attempt and independent rerun. All 393 available original exported
median rows agree exactly. All 126 independent A2 selected indices and quality
records agree with the frozen native validator and its independent replay.
Five reader control tests cover real original rows, missing/relabelled samples,
retention of short/wide failures and invalid JSON. Raw input bytes are checked
before and after replay; no codec work is run for this review.

Reviewed main has changed `Cargo.toml` and `Cargo.lock`, including smallvec
1.16.3 to 1.16.4. The checked Rust codec/tool sources, sampling templates, build
configuration, WASM, benchmarks and top-level tests otherwise match the frozen
source. Whole-tree and lock binding still differ. Captured source labels stay
unchanged and cannot qualify this newer main.

## Remaining execution order

1. Establish a reproducible complete published-9.0.0 baseline on the controlled
   Linux environment. Preserve every repeat/width refusal before investigating
   fault, scheduler, frequency or caller-layout hypotheses.
2. Freeze the actual final candidate, tool, manifest and lock inputs. Dependency
   or source changes need their own validation and complete source binding.
3. Obtain a fresh accepted complete ABBA and paired-layout campaign on that
   frozen source. A separate endpoint rerun cannot substitute for it.
4. Independently verify every raw member and run the actual strict positive
   release preflight before tagging or publication.

This follow-up records completed diagnostics and their remaining failures.
No additional performance campaign, tag or registry publication is launched.
