# Synthetic cross-arm gate validation

This is a control-flow test of
[`../2026-11-01-e2e-current/cross-arm-gate.patch`](../2026-11-01-e2e-current/README.md),
not a codec benchmark. `synthetic_probe.rs` emits constant synthetic samples:
the same-source preflight reports 100 for every phase, while the simulated
candidate reports 106 for every phase. Thus all within-round and same-source
checks pass, but the candidate A level differs by 6% from both preflight A
levels. The executable waits for a `continue` acknowledgement after each case.

On native x86 Linux, the patched historical streaming runner completed both
three-case synthetic preflight rounds, then rejected the first simulated
candidate case for 6.00% cross-arm drift. `results/candidate-1.jsonl` contains
exactly that one recorded but unacknowledged case; no second candidate round was
created. The expected `failed` summary is success for this control-flow test,
not a failed codec benchmark. No latency or candidate performance conclusion
can be drawn from these invented values.

The runner monitored four resource intervals: effective CPUs were 0–15, no
named competitor was observed and the off-target busy streak remained zero.
This short synthetic test does not establish a stable window for a real
benchmark. The earlier end-to-end candidate comparison remains rejected.
