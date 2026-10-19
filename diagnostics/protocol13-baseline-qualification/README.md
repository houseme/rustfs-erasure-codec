# Complete protocol-13 baseline qualification — rejected

Measured tool: `121b39d09774a51c79a21f51f5570ef4ef2a782c`.
Measured library: published 9.0.0 `9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`.
Descriptive analyzer: `9a353a3cff525237f7b2c7a31eca5b1a3276b2c5`.

One predeclared default-allocator qualification followed the protocol-13 placement
correction. All 126 cases calibrated: 4,800 observations. Complete A1 contains
1,512 observations (378 warmups and 1,134 measured samples). No short samples;
all 103 recorded caller layouts match calibration. Nine distributions exceed the
unchanged 10% limit. A2 and candidate capture were not run. This is not A1/A2 drift,
a 9.1.0 regression, accepted evidence or release approval.

| Profile | Case | Central distribution width |
| --- | --- | --- |
| smoke | `update:4:2:65536` | 15.146718% |
| small_file | `encode:10:4:65536` | 11.701369% |
| small_file | `reconstruct_opt:10:4:524288` | 23.304436% |
| small_file | `reconstruct_shard_slot:10:4:524288` | 17.193914% |
| small_file | `reconstruct_some_data_only:10:4:16384` | 10.152551% |
| small_file | `verify_with_buffer:4:2:524288` | 11.949453% |
| hotspot | `reconstruct_some_required_1_of_2_missing_data` | 12.137249% |
| stream | `encode_stream:10:4:1048576:1048576` | 25.228623% |
| stream | `encode_stream:10:4:16777216:4194304` | 59.103368% |

Every one of the 6,312 sample records was compared against its completed timing
journal, including all numerical fields carried by the journal. JSON and frozen
count files were copied without numeric redaction. Timing logs retain only the
original RSE_TIMING lines; ancillary build/process output is private. Cleanup
restored the partition, controller and parent CPU ranges after 636 isolation
checks. The failure is expected quality rejection, not a library failure.

## Service-time assessment

The unchanged 10+4/64KiB ordinary encode caller has central ranked sample wall time
ratio 1.12042 and measuring-thread run ratio 1.11470; its maximum guest runnable
wait/wall is 0.1697%. Buffered verify 4+2/512KiB has wall ratio 1.12182, run ratio
1.12187 and maximum guest runnable wait 0.0218%. Matching retained caller offsets
and low guest runnable wait do not prove matching physical pages, memory service,
frequency or host scheduling. Pool-thread time is absent from formal protocol 13.
These numbers cannot establish a unique cause or justify a runtime-code change.

## One descriptive software profile of the largest failing case

`service-profile/` contains one baseline `encode_stream:10:4:16777216:4194304`
worker, with the already frozen count six and all 3 warmups + 9 samples. The opt-in
allocating process/thread observer is explicitly diagnostic. It is not substituted
for formal telemetry or used for a throughput comparison.

The first two measured observations have 184,141 / 92,138 process minor faults;
subsequent observations have 24,582 or 24,583, with zero major faults. Wall time is
331.99 / 200.48 ms, then approximately 142 ms. All six anonymous thread slots are
stable; endpoint migrations are zero. This exposes a page-fault/service-time state
transition in this diagnostic, but does not prove the same transition explains the
oscillating formal A1 trace or all nine failed cases. Extra observer allocations
can change allocator state; simply extending warmup or discarding early samples
is not an established fix.

The cpu-clock:user profile has 2,042 leaf samples; all symbol counts are retained,
including 45.40% unresolved leaves. Lost and throttle event counts are zero.
Capture finished once. The private export initially applied the formal-only
telemetry validator to diagnostic process fields; its raw data and failure remain
preserved. Reprocessing used the existing diagnostic validator and analyzer,
without another measurement. Raw perf addresses and unrelated host paths remain
private; anonymous thread counters, every numerical observation, full symbol
summary and cleanup outcomes are public.

Replay the baseline analysis with the analyzer source above and its matching
protocol/renderer:

```sh
python3 scripts/analyze_baseline_variability.py \
  --capture <audit>/diagnostics/protocol13-baseline-qualification/campaign \
  --output <new-analysis.json>
```

All original gates remain. Later protocols must collect their own evidence; these
failures are never migrated into a new protocol as passing results. Do not merge
this audit branch into main or use it as RSE_RELEASE_EVIDENCE_REF.
