# Complete protocol-9 same-version baseline control

Frozen tool 00b6560a18490b12ca211475532e4df6ac9bb157; actual codec is published 9.0.0. Complete
calibration preserves 4812 observations. Completed phases preserve
{"A1": 1512} observations, including 1134 measured
samples. Every warmup and completed numerical journal is retained.

Distribution failures under the unchanged 10% gate:

- A1/small_file/reconstruct_opt:10:4:524288: 18.012053%.
- A1/hotspot/reconstruct_data_missing_data_plus_parity: 11.544911%.
- A1/stream/encode_stream:10:4:1048576:1048576: 33.385285%.
- A1/stream/encode_stream:10:4:16777216:4194304: 36.664491%.
- A1/stream/verify_stream:10:4:1048576:1048576: 10.524643%.
- A1/stream/verify_stream:10:4:16777216:4194304: 22.367302%.

Short-sample groups: 0. Tracked caller layout mismatches:
0. Repeat completed: False.
Baseline prerequisite passed: False.
This is not a candidate comparison or release approval. A first-phase quality
failure stops before A2; no failing subset is removed or threshold weakened.
Allocator policy is explicitly bound in provenance and is never generalized to
other allocator contexts. Caller equality does not establish untracked dynamic
output/internal-scratch layouts. All CPU endpoints and complete failure traces
remain available; equal endpoints do not rule out migration or host effects.

Original numerical JSON/TSV, source/controller bindings and cleanup files are
byte-identical copies. Additional journal JSON extracts every completed numeric
line without changing values; derived-analysis.json replays the actual frozen
validator, recomputes every required case width, repeats and interval/layout gates,
and preserves complete failed-case final-calibration/measurement traces. Build,
launcher and management logs stay private. Any source-bootstrap refusal happened
before reservation/observations and is explicitly retained. This snapshot must
not be treated as an accepted release evidence ref by itself.
