# Writer page-lifecycle point diagnosis — not release evidence

Frozen tool `334fe2b027185e203189cb2dd67360395aa5c097`, published 9.0.0 source
`9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`, original
`encode_stream:10:4:16777216:4194304`, six iterations per observation,
three warmups and nine samples in one worker. No candidate performance comparison,
new full calibration, repeat qualification, physical-host exclusivity or release
approval. All original reservations, copies, read/encode work and scope-end frees
remain. No extra warmups, prefaulting, allocator overrides or sample selection.
The transparent adapter and bounded counter probes still perturb service/layout.

All 12 observations, 72 API calls, raw views/windows and numeric journals are
retained unchanged. `derived.json` partitions every call and retains all warmups.
Window process faults include pool threads and exclude child processes; scheduler
run/wait counters cover the calling thread only. Procfs counter intervals bracket
operations differently from their timestamps and include observer effects.
`outside_writer_windows` includes reads, codec work, other allocation/free and
probes/metadata; it is not pure codec CPU. Physical residency and unique fault
origins are not measured. Formal protocol 14/renderer and all gates remain intact.

## Observed results

Times below are totals for six API calls, in milliseconds. The outer wall also
contains observation-level instrumentation outside the individual call windows.

| Index/kind | Outer wall | Reserve | Append | Drop | Outside writer windows | Append / outside minor faults |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 warmup | 265.283 | 0.152 | 78.813 | 14.281 | 171.271 | 49144 / 86069 |
| 1 warmup | 146.519 | 0.028 | 23.845 | 0.003 | 121.947 | 4095 / 29702 |
| 2 warmup | 207.458 | 0.125 | 37.865 | 8.963 | 159.766 | 16380 / 63494 |
| 3 sample | 177.119 | 0.093 | 32.801 | 3.808 | 139.699 | 12285 / 52230 |
| 4 sample | 173.259 | 0.072 | 27.859 | 5.414 | 139.236 | 8190 / 40966 |
| 5 sample | 209.883 | 0.163 | 41.961 | 7.259 | 159.813 | 20475 / 74759 |
| 6 sample | 143.875 | 0.008 | 18.676 | 1.775 | 122.727 | 0 / 18438 |
| 7 sample | 230.296 | 0.205 | 46.573 | 10.686 | 172.143 | 24570 / 86022 |
| 8 sample | 144.474 | 0.036 | 23.223 | 0.004 | 120.528 | 4095 / 29702 |
| 9 sample | 203.730 | 0.132 | 37.008 | 8.839 | 157.068 | 16380 / 63494 |
| 10 sample | 176.570 | 0.095 | 32.512 | 3.483 | 139.780 | 12285 / 52230 |
| 11 sample | 178.447 | 0.067 | 27.876 | 5.214 | 144.528 | 8190 / 40966 |

All 54 measured calls fall into three counter tuples `(reserve, append, outside)`:
28 calls `(0, 0, 3073)`, 25 calls `(2, 4095, 14337)`, one call `(2, 4095, 14338)`.
Across the 216 measured writer lifetimes, 190 have zero append faults and 26 have
4,095. This page-sized step is consistent with 16 MiB destinations on 4 KiB pages,
but does not prove allocation identity, physical residency or a unique allocator
mechanism. All measured major-fault deltas are zero. Drops still consume time.

The slowest/fastest observed samples (7/6) differ by 86.421046 ms in call windows:
reserve 0.196901 ms, append 27.896594 ms, drop 8.911422 ms and outside 49.416129 ms
(57.18% outside). This is an arithmetic partition of observed intervals, not a
counterfactual speedup or a matched causal experiment. Writer-only changes cannot
be justified as a complete solution from this evidence.

Source inspection of published 9.0.0 stream encoding shows 10 read buffers plus
4 parity buffers, each reserved at 4 MiB per API call (56 MiB). The outside-window
14,337-fault state is close to that footprint in pages. That makes scratch first
touch/free and its interaction with caller writer allocation the next focused
hypothesis; it is not proof that every outside fault belongs to scratch. No
VerifyWorkspace, Leopard owned-output or generic ReconstructShard behavior changed.

## Checks, provenance and replay

Native Linux heap executable: 19 passing tests, including zero callback
alloc/realloc/dealloc; both actual codec sources/lockfiles execute all 15 stream
case contracts; generated baseline sampler Clippy passes. 75 isolation checks,
owned partition removed, controller and original CPU ranges restored. This only
establishes VM partition ownership, not physical-host isolation.

`prior-attempt.json` preserves the premeasurement generated-item-order Clippy
failure and restored resources; no point observations existed in that attempt.
Preparation-only launch failures are also recorded. Compiler/launcher logs and
private process identity/path material stay private. All captured point numerical
observations and their public-safe source/runtime bindings are present.

Replay with a checkout containing PR #29's strict offline validator:

```sh
python3 replay.py --tool /path/to/codec-checkout
```

The final validator additionally rejects missing Linux window counters. Its Rust
observer/native template and generated overlay were checked byte-for-byte equal
to frozen `334fe2b`; completed observations were replayed without recapture.
This directory is audit data only: never merge into main or configure it as
`RSE_RELEASE_EVIDENCE_REF`. Backlog #2897 conditions 1/2/3/5 remain partial, condition
4 stays within its declared normal-format scope, condition 6 remains unaccepted.
