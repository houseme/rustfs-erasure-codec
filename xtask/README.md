# Native repository tools

All repository controllers, validators and tests are Rust. No interpreter or
Python binding is needed. `xtask` is unpublished and is outside the core and
WASM crate allowlists. All five workspace members inherit dependencies from the
root Cargo.toml; the codec does not depend on the tooling package.

```bash
cargo xtask --help
cargo test --locked --package xtask
cargo xtask audit-no-python
cargo xtask audit-json
cargo xtask audit-package
cargo xtask audit-workspace
```

## Capture and replay commands

| Command | Purpose |
| --- | --- |
| `release-sampling --check --check-workers` | Compile the shared overlay and check all 23 smoke/hotspot workers at one iteration |
| `release-evidence capture` | Complete controlled baseline/candidate ABBA and paired layout collection |
| `release-evidence qualify-baseline` | Same-version full-catalog baseline control; no release approval |
| `release-evidence diagnose-complete` | Collect the entire predeclared campaign despite statistical refusals |
| `release-evidence verify` | Bind every original member, source/compiler/context, quality gate and paired supplement |
| `release-evidence verify-diagnostic` | Recompute the complete diagnostic without release approval |
| `release-evidence materialize` | Verify first, then stage the accepted quantitative inputs for strict preflight |
| `diagnose-fixture-layout` | Retained Vec or stream-view inventory across all four phases |
| `verify-scratch-audit --check` | Ordinary verify scratch in both reversible source copies |
| `stream-core-audit --check` | Complete stream ownership/view lifecycles in both source copies |
| `dynamic-buffer-audit --check` | All 516 original dynamic workers, including decoder supplements |
| `diagnose-verify-layout` | Complete paired slice layouts or one-source structural checks |
| `diagnose-cpu-placement` | Post-construction thread placement with complete READY/END epochs |
| `diagnose-shard-slot` | Fixed-count original Slot clone/layout observations |
| `diagnose-adjacent-baseline` | Complete adjacent same-source capture or replay of all 126 cases |
| `analyze-baseline-variability` | Original baseline or source-bound adjacent service analysis |
| `diagnose-release-vm` | Whole-worker VM diagnosis, zero-codec native probes or read-only MSR cohorts |

Each command's `--help` lists its options. Use the [release evidence guide](../docs/release-evidence.md)
and [caller inventory guide](../docs/release-caller-fixtures.md) for complete
reserved-CPU setup and invocation examples. Capture commands require new output
paths and committed source. Checks that deliberately instrument a private source
copy remain diagnostic-only.

## Shared implementation

`control` composes the existing typed models directly. Its filesystem/child
owners are shared by every collector; domain loops retain their explicit phase,
cohort and failure rules. Calibration, selected medians, preparation/RNG,
source overlays, row aliases, runtime provenance, layouts and statistics each
have one Rust model. Current controllers make no helper RPC calls and no longer
launch sixteen Cargo regression subprocesses during complete verification.
The native regression CLI and controller use the same report functions.

Original files are bounded to 32 MiB and hashes bind the exact bytes decoded.
Legacy duplicate-key, integer-negative-zero, newline and numeric-context rules
remain in the Rust evidence reader. Complete replay checks all calibration and
sample members, every summary, the worker journal and the paired supplement.
Missing/broken supplements are refused before source observation or regression
work. A copied summary cannot replace original observations.

The frontend captures live Git/compiler observations separately from pure
replay. It checks them again before publishing a successful result. Same-tree
rebases retain their supported tree/lock binding. Diagnostic-only and formal
schemas remain distinct. A failed invocation overwrites its requested validation
report with failure, including rejection of stale executable code.

## Resource ownership

Every worker has an owned process group anchored by its unreaped leader. On
cancellation or failure, it is stopped and reaped before generated files are
restored. An unconfirmed cleanup preserves the private inputs and logs.
Generated overlays refuse existing inputs, including dangling links. Cleanup
validates the whole file set before changing any member. External modifications
or extra files preserve the entire transaction. Patched source restores its exact
original bytes, including CRLF. Both normal and error paths use these owners.

CPU diagnostics validate original construction budgets, ownership, observation
counts, cached policy and real affinity at every handshake. After `G`, the
measuring thread changes only its own affinity with PID zero; the parent never
pins a caller-supplied TID. `E` follows a complete validated epoch. Partial numeric
observations remain available on cancellation. Formal sampler timing and kernels
retain their separate original implementation.

## Executable and frozen source identity

The build script and runtime share one source/asset stamp. It covers native Rust
sources, Cargo inputs, contracts, sampler assets and diagnostic templates. A
controller refuses execution after those inputs change; rebuild with Cargo.
Source symlinks are refused. Documentation changes do not alter this stamp.
`RSE_NATIVE_TOOL_ROOT` explicitly selects a relocated matching source checkout;
its complete native stamp must still match the executable.

Frozen analysis accepts an explicit trusted source checkout, verifies recorded
Git/controller bytes, and builds its Rust validators offline in a private clone
of that immutable commit. Bounded persistent Rust frames retain validation-error,
cancellation and full-u64 behavior. The current Rust coordinator records its own
identity as well as the executed frozen model source. Historical controller
files are read only as recorded identity inputs; they are never executed.
All original capture/prior members come from one bounded snapshot and are checked
again after replay. Missing historical kernel policy remains unknown.

## CI and release boundary

CI uses Rust for runtime provenance, lock-version extraction, resource-record
checks and status JSON. `write-ci-status.sh` invokes the Rust writer. If the native
tool cannot be installed or built, the shell emits only a fixed failure object
and fails; it cannot retain a stale passing journal or interpolate untrusted JSON.
Strict release preflight calls `release-evidence materialize` and retains all
existing functional and quantitative gates.

`audit-no-python` rejects interpreter files and CI/Shell dependencies. Repository
JSON retains evidence paths and hashes; duplicate phase-local files are not
removed. The root and WASM package allowlists exclude all repository tools,
controllers, external tests and archives. Actual package reconstruction is part
of validation.

## Validation scope

The retired controller suite passed all 252 behavior tests against the Rust models
before removal. Native tests reuse complete 9,072-observation campaign fixtures
and paired original layout files. They cover approval separation, hashes, source
changes, numeric contexts, schema refusal, complete diagnostic failures, process
reaping, modified/symlinked files, MSR order/short reads, frozen frame recovery,
stale-code refusal, release shell policy and failure journals.

The current source-bound results and test totals belong in the
[global review](../docs/global-review-9.1.0-2026-10-23.md#complete-native-controllers-and-python-retirement).
Older dated reviews retain their original identities and measurements. This
migration provides no codec throughput or release-performance approval. The
published-9.0.0/candidate-9.1.0 compatibility probe covers 200 non-fast exact-parity
and bidirectional cross-decoding configurations, plus historical field vectors.
The older inconsistent fast-one-parity mode remains outside that probe.
