# Complete protocol-10 same-version baseline control

Frozen tool 63b6fa0a3d0b0dc6a7d206c9dbff82f5f729eea8; actual codec is published 9.0.0. Complete
calibration preserves 4980 observations. Completed phases preserve
{"A1": 1512, "A2": 1512} observations, including 2268 measured
samples. Every warmup and completed numerical journal is retained.

Distribution failures under the unchanged 10% gate:

- None.

Short-sample groups: 0. Tracked caller layout mismatches:
0. Repeat completed: True.
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

## Repeat gate failure

All 252 within-phase distributions pass width and every interval/layout check.
Seven of 126 same-version repeat checks still exceed the unchanged 5% gate:

- small_file/encode:10:4:524288: 15.827029%.
- small_file/reconstruct:10:4:16384: 28.181878%.
- small_file/reconstruct_opt:4:2:65536: 18.164593%.
- small_file/reconstruct_some_data_only:10:4:524288: 6.383947%.
- small_file/reconstruct_some_data_only:4:2:65536: 9.745576%.
- stream/encode_stream:10:4:1048576:1048576: 7.677177%.
- stream/encode_stream:4:2:65536:65536: 24.248709%.

These are same-version drifts, not candidate regressions or speedups. Matching
caller storage does not establish all internal or physical memory placement. The
ordinary encode case has all caller byte buffers tracked; untracked recovery
outputs therefore cannot uniquely explain every residual drift. No candidate
comparison starts after this failed prerequisite.
