# Complete protocol-10 same-version baseline control

Frozen tool 63b6fa0a3d0b0dc6a7d206c9dbff82f5f729eea8; actual codec is published 9.0.0. Complete
calibration preserves 4800 observations. Completed phases preserve
{"A1": 1512} observations, including 1134 measured
samples. Every warmup and completed numerical journal is retained.

Distribution failures under the unchanged 10% gate:

- A1/small_file/reconstruct:10:4:524288: 19.285169%.
- A1/small_file/reconstruct_opt:10:4:524288: 19.802595%.
- A1/small_file/reconstruct_opt:4:2:65536: 27.103972%.
- A1/stream/encode_stream:10:4:1048576:1048576: 35.054125%.
- A1/stream/encode_stream:10:4:16777216:4194304: 35.445439%.
- A1/stream/verify_stream:10:4:1048576:1048576: 10.922217%.
- A1/stream/verify_stream:10:4:16777216:4194304: 22.574548%.

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
