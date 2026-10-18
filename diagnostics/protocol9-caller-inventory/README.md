# Complete protocol-9 caller-storage inventory

Frozen source 00b6560a18490b12ca211475532e4df6ac9bb157 prepares a worker-lived
codec/source fixture for all 88 small-file and 15 stream cases. The shared overlay
on published 9.0.0 and that candidate uses the same unchanged benchmark kernels
and explicit retained-caller/API/output contracts. All 103 cases complete all four
one-iteration phases: 4,944 original observations and 618 same/cross-version caller
layout comparisons, with zero mismatches. All raw records, timings, code/context
bindings and controller sources are retained; validator-replay.json recomputes the
complete inventory from actual raw observations, rather than synthetic fixtures.

This is structural qualification, not performance ABBA or a speedup. Observations
need not meet release minimum duration. Dynamic destinations/internal scratch are
explicitly untracked; matching caller buffers do not establish those layouts.
Smoke/hotspot contracts and the separate 16-layout 1/4 KiB acceptance remain
outside this inventory. All reports retain release_approval=false and null
candidate attribution. No completed global #2897 condition follows from this alone.

Native Linux checks pass per executable: nine shared-sampler tests, six observer
executable tests (including the zero-Rust-heap regression), and two tracer tests.
Some shared tests appear in multiple executables; these counts are not summed as
unique tests. Qualification replaces its coordinator before inventory execution,
so no extra ancestor remains in the measurement CPU partition.

Copied numerical/source files are byte-identical to capture inputs; the replay
file is derived and named accordingly. Management/build/console logs remain
private. Every owned resource was restored after 99 isolation checks. Subsequent
full 126-case baseline controls are a separate capture and do not inherit a
performance pass from this inventory. No rejected data is an accepted evidence ref.
