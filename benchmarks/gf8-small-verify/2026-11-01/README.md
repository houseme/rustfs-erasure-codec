# Small-shard Classic verification investigation

Sizes below are **per-shard elements**, not logical object bytes. The focused
release test in `tests/parallel_verify_workspace.rs` observes allocations on
the calling thread after warmup, checks valid parity and corruption in both
parity rows at the first and last element, and covers Vandermonde, Cauchy,
Jerasure-like and custom matrices. It covers GF8, GF16 and a custom three-byte
field. A retained caller buffer is checked separately; no timing is inferred
from allocator request counts.

On the local Apple Silicon build, ordinary 4+2 GF8 verification made no heap
request at 2,047 or 2,048 elements per shard, then made one 4,098-byte request
at 2,049 and one 8,192-byte request at 4,096. GF16 showed the same element
boundary, with requests of 8,196 and 16,384 bytes respectively. The warmed
caller-buffer path made no calling-thread requests in the observed cases.
These observations do not by themselves establish that allocation dominates
latency or that the same request sizes apply to all builds or allocators.

`candidate.patch` is an **unapplied experiment** that checks parity rows one at
a time using one inline scratch row for lengths within the inline limit. In a
local disposable source copy at `5f93c16`, the focused GF8/GF16 allocation and
corruption checks removed the measured request. It can change generated-kernel
selection, repeat input work and short-circuit after a mismatching row; resource
savings therefore do not prove a throughput gain or full compatibility.
`candidate-single-scratch.patch` is a later local variant that keeps one scratch
object for both branches to avoid introducing a second inline scratch object.
Its local release integration suite passed all eight tests, including the
expanded GF8/GF16/custom-field and matrix checks, and no-default-features build
passed. Neither patch is active in production or accepted for performance.

An additional disposable build applied `candidate-single-scratch.patch` and
`candidate-interop-counter.patch`, together with the expanded release
compatibility probe. On Apple Silicon, 32 GF8/GF16 small verification cases
matched published 9.0.0 parity and rejected corruption in both directions.
The counter observed 208 candidate-path entries during the new version's
initial small-case pass; it is test-only and makes no performance claim. The
counter patch assumes the expanded compatibility probe is already present.

For native x86, `paired_probe.rs` uses the automatic SIMD backend, one visible
CPU and one Rayon thread. It shares valid and corrupted input buffers, checks
both codec copies, and tests 4+2 and 10+4 at 1,024, 2,048, 2,049, 4,096 and
4,097 elements. `runner-small-cases.patch` applies after the previously
validated cross-arm gate to the archived streaming runner; it changes cases and
single-CPU policy, and monitors fourteen off-target CPUs at the same half-core
aggregate threshold. The candidate tested by this setup would have been the
first patch, not the later single-scratch variant.

The `results/` preflight used committed baseline
`dbb6e670ab9604d4011bd18b6732ed1356cd8d7f`; the two copies differed only by a
temporary package version. It stopped at 4+2 / 2,049 elements because the
same-source A/B level bias was -5.80%, outside the 5% limit. The active backend
was GFNI AVX-512; 112 monitor observations retained CPU set 0–15, no named
competitor and no off-target busy streak. No candidate ran. Layout, timing and
other sources of this bias were not distinguished; do not derive a candidate
performance conclusion.

`single_copy_probe.rs` is a materially different baseline-only diagnostic: A
and B call the same non-inlined function and codec instance. The native binary
contained one matching function symbol. `single-results/` stopped on a
resource-monitor rejection after one completed 4+2 / 1,024-element case;
65 monitor observations recorded an active named competitor in one sample.
The earlier 2,049-element bias case was not reached, and no candidate ran.
This does not show whether the bias is resolved. Before considering adoption,
use a stable, provenance-matched comparison and validate both ordinary and
parallel verification, custom fields, stack usage, feature combinations and
small-size regressions. The original same-source gate must not be weakened.

## Refreshed same-function control

A later baseline-only attempt rebuilt both copies from committed source
`a76dd2ef66bd2392847c8ff0fc449f7df50e2ae7` and retained the same non-inlined
common function. Its native binary again contained one matching symbol. The
first resource-monitor observation reported CPUs 0–15 but an active named
competitor, so the runner refused to start. `continuation-a76/results/`
contains the one observation and failure summary; no baseline or candidate
case was recorded. The 2,049-element bias therefore remains unexplained.
