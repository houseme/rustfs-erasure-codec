# Fixed-worker baseline diagnostic

This baseline-only diagnostic continues the previously failed preflight in
[`../2026-10-31-freeze-7587132/README.md`](../2026-10-31-freeze-7587132/README.md).
The source snapshot is commit `7587132` plus the worktree patch saved there.
No candidate performance was measured in this diagnostic.

`pinned_paired_probe.rs` links two same-source diagnostic-bridge copies. It
constructs both codecs while five guest vCPUs are visible, so the cached
parallelism remains five and the explicit codec job limit remains four. It
then creates a dedicated four-thread Rayon pool; its start handlers use
`taskset` outside timed work to pin workers to vCPUs 8-11. A pool-wide check
asserts each worker's affinity, and the external caller is pinned to vCPU 12.
The measured operation runs inside the Rayon pool and reuses an output buffer.
This differs from the ordinary external-caller execution context and is not
production performance acceptance.

`affinity-aa-results/` contains two same-source baseline rounds. All three
cases passed the existing 5% within-round and cross-round gates; the largest
absolute cross-round A/B level change was 1.16%. The monitor observed cpuset
0-15, no named competing benchmark and a maximum off-target busy streak of one
sample, below its three-sample stop rule. The test verifies full output after
every phase. It supports the fixed-worker setup as a diagnostic direction, but
cannot establish how much of earlier drift came from worker migration,
scheduling, binary layout, cache state or other unobserved factors.

A subsequent fresh `affinity-comparison/` attempt was configured to run two
new baselines before any candidate. All three cases in its first baseline
round passed. Before the next baseline case completed, the guest effective
cpuset changed from 0-15 to 0-7; the resource monitor terminated the probe.
No candidate case started. The original summary reported the secondary
`missing case` symptom; `affinity-comparison-posthoc-review.json` records the
resource cause and supersedes that reason. The current `run_streaming.py`
prioritizes the monitor error at EOF, with a regression test. The old runner
and original results are retained unchanged.

Pinning and a dedicated pool may change scheduling, cache traffic and calling
semantics compared with RustFS. Even if a future fixed-worker kernel comparison
passes, a separate default-caller, end-to-end A/B/B/A acceptance remains
necessary. This evidence does not rehabilitate any previously rejected run and
does not enable the 256-byte candidate.

## Fresh preflight and resource interruption

After another sustained idle observation, `affinity-comparison-v2/` ran a
fresh strict preflight. Round 1 passed all three cases. Round 2 stopped at the
4+2 control because its A-level cross-round drift was +8.79%; no candidate file
was created. Its monitor observed cpuset 0-15, no named competitor and a
maximum off-target-load streak of zero. Fixed worker affinity therefore did
not control this cross-round variation; the rejected run cannot support a
performance conclusion.

A subsequent baseline-only diagnostic attempted to disable ASLR only for the
probe process. It did not start: the initial monitor saw sustained off-target
load and stopped (`affinity-no-aslr-aa-results/`). A `rustc` process was
visible immediately afterward, but the monitor did not capture process identity
for the sampled load, so causation is not asserted. The original summary's
initial-state message is retained along with its detailed monitor error; the
current runner now prioritizes the resource-monitor cause. No address-layout
or candidate performance conclusion follows.

## Fixed-worker, per-process no-ASLR diagnostic

A later baseline-only attempt ran `setarch x86_64 -R` on the fixed-worker probe
process, using the updated strict runner. Both rounds in
`affinity-no-aslr-aa-v2/` passed the numeric gates and the resource monitor
saw cpuset 0-15, no named competitor and no off-target-load streak. However,
the 4+2 control had -4.85% within-round A drift in round 1 and +4.98% B-level
cross-round drift in round 2, both close to the 5% limit. The run used a
non-default address-randomization setting and the dedicated Rayon-worker
calling context; it does not establish stability in the default production
configuration. No candidate was run, and no earlier failed preflight is made
valid. `run_streaming_latest.py` is the exact runner version used for this
record; the launch wrapper only executed `setarch x86_64 -R` on the probe.
