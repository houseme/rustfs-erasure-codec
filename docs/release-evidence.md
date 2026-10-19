# Controlled release evidence

Release 9.1.0 uses published **9.0.0** (`9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6`)
as its baseline. No accepted snapshot has been configured merely by adding this
workflow. Ordinary CI and mocked tooling tests do not establish performance.

## Collect on the controlled Linux x86_64 environment

Use fresh, clean Git checkouts of the baseline and the final candidate; keep the
baseline commit available in the candidate repository. Finish candidate changes
before collecting: evidence binds the **entire candidate tree**, lockfile and
benchmark harness. An unchanged tree after a rebase merge is accepted; code,
configuration or documentation changes produce a different tree and require new
evidence. The recorded measured commit and verified current commit remain separate.

Reserve an exclusive cgroup v2 CPU partition and launch the collector inside it.
Audit ownership before reserving resources, monitor the reservation during capture,
and restore the partition/controller state afterwards. Do not stop existing
workloads. The collector checks its partition and affinity, exclusive CPU set and
absence of other workloads, before/after measurement. VM partition isolation does
not establish physical-host exclusivity. Keep the launcher, telemetry and original
logs in private experiment storage.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
python3 candidate/scripts/release_evidence.py capture \
  --baseline /test/baseline --candidate /test/candidate \
  --output /test/new-campaign --isolation-cgroup /sys/fs/cgroup/reserved-release
```

The collector builds the same external integration test against each unmodified
codec using the original benchmark functions and writers. Generated overlay files
are untracked test inputs, recorded through the renderer and normalized overlay
digests; neither release's production code nor original benchmark files are
edited. The stream adapter corrects one explicit fixture constructor on both sources:
cloning an empty `Vec::with_capacity` discards its reservation. Every parity writer
now receives an independent reservation; these allocations remain inside each
original timed iteration. The original benchmark files stay untouched for source
comparison. Protocol 7 binds this output policy; earlier policies require new
calibration and cannot be mixed with the corrected workload. This fixture fix
is not a production algorithm change or an attributed speedup.

The external adapter excludes one-time fixture preparation from the
small-file and stream timers on both versions, with a checked reversible transform
that preserves all loop bodies and per-iteration allocations. Smoke keeps its
original per-iteration workload; hotspots keep their original inner recovery
intervals. Timing scopes are recorded, so the older cohort is not quantitatively
comparable to these new summaries. The candidate tree binds the committed renderer/templates. Both use
locked release builds, `std simd-accel`, no instrumentation, four Rayon workers,
a four-job policy and empty RUSTFLAGS. The context also binds libc name/version, the OS base page size,
and all inherited `MALLOC_*`, `GLIBC_TUNABLES` and `LD_*` allocator/loader controls.
These must be present and identical in every phase and the final context. An
allocator policy change requires a new complete campaign and calibration.
Builds and workers explicitly use each source checkout's own `target` directory,
including the paired-layout supplement. An inherited shared `CARGO_TARGET_DIR`
cannot mix generated build outputs across sources. The native dual-version check
uses a real isolated baseline checkout so deferred Git provenance also remains valid.

Every case is calibrated on both releases before A1. Calibration targets at least
100 ms wall time and 5 ms timed work, with bounded increasing iterations. For each
proposed count, calibration retains the same three warmups and nine probes used
for measurement. All nine probes must meet both duration targets before the
count is frozen. Any short probe increases the count and restarts the full warmup
schedule. Calibration is bounded to 12 batches / 144 records per case, and every
observation is retained. The Python controller chooses counts; the Rust worker
executes one fixed schedule and receives the same scalar case/count inputs in
both modes. Frozen-table parsing and adaptive selection stay in the controller,
so no previous-count state or measurement-only count map changes the worker heap. This avoids freezing a count from cold probes that
precede the actual measurement warmup schedule. Allocation state may still change
between separate invocations; every formal sample must pass its own interval
check. The larger
iteration count for each case is frozen for **both releases and every phase**.
Each proposed calibration count and each measurement case runs in a fresh test process, in a fixed
lexicographic case order. This prevents another case from seeding its allocator
arenas or worker-pool state. Protocol 7 records `isolated_case=true` on every
observation; missing/false markers are rejected. Every phase performs exactly
three warmup invocations at the frozen count, then collects exactly nine samples.
Basic scheduling snapshots retain numeric counters instead of formatted heap
strings. Extended process observation remains diagnostic-only and may affect
allocation state; release qualification uses basic telemetry.
Protocol 12 retains the protocol-9/10 caller contract: one codec and caller source fixture for all 88 small-file and
15 stream cases for the complete worker; buffered verify also retains caller
scratch. Caller buffers precede codec construction and encoding setup. The shared
inner loops are extracted from the unchanged benchmark sources, preserving timed
per-iteration clones, missing-output allocation and I/O. Verification booleans are
observed. Codec caches now live for the worker; this explicit workload change
requires new calibration and measurements.

Fixed storage records source/scratch lengths, capacities and offsets modulo 64,
4096 and the actual OS base page size, with full pointer identity checked privately
outside the inner timer. Every record also requires the actual call entry, output
policy and missing/required masks. Layouts must stay stable within a worker and
match final calibration and all four phases. API/policy mismatch fails closed.
The controller supplies the page size from sysconf; inconsistent or missing page
metadata is rejected. Base-page offsets do not assert physical/huge-page placement.
Dynamic destination/internal-scratch offsets remain explicitly untracked.
Protocol 12 extends worker-lived codec preparation and mandatory API/output policy,
mask, seed and source-lifetime contracts to all 126 cases. Smoke retains its
per-iteration source generation and timer. Hotspot borrows a worker-seeded SmallRng
for its original per-byte fill loop; inputs are generated/encoded each iteration,
with all original clones, missing outputs and inner recovery timers. Bounded
metadata records generated-byte consumption and two non-consuming RNG preview
words before/after each observation. Verification rejects missing/wrong contracts,
input resets and calibration/four-phase input mismatches. Real native baseline
and candidate checks compare prefix bytes and all 23 changed workers' input traces
using their own lockfiles; these are correctness checks, never performance approval.
This explicit workload change requires a fresh complete campaign. It does not
identify the cause of historical ordinary-encode drift. See the complete [caller fixture
contract](release-caller-fixtures.md) for this scope and its limits.

Hotspot result pattern metadata stays in bounded inline storage until deferred
export, instead of retaining new Strings between observations. Its labels match
the actual original formatter; a separate executable checks zero Rust heap
activity for construction/formatting. Hotspot random preparation changes explicitly in protocol 12; recovery intervals
remain unchanged. Earlier observer protocols cannot approve protocol 12.
Protocol 11 removes the unused diagnostic template from the formal sampler overlay
and its implementation digest; layout tests now use one separate diagnostic fixture.
The full-profile operation loops and limits are unchanged. Fresh source-bound
evidence is required for the changed renderer; earlier captures stay historical.

The structural inventory checks all 103 retained caller cases in A1/B1/B2/A2 order
against both sources before performance capture:

```bash
python3 candidate/scripts/diagnose_fixture_layout.py \
  --tool /test/candidate --baseline /test/baseline --candidate /test/candidate \
  --output /test/new-fixture-inventory \
  --isolation-cgroup /sys/fs/cgroup/reserved-release
```

It compares same-version and cross-version layouts, including capacity and base
page metadata. Its one-iteration workers retain the full warmup/sample schedule
for structural inspection only; they do not meet release-duration qualification.
Every mismatch is retained and the report always has `release_approval=false` and
no candidate performance attribution. Both sources must remain clean and use the
same external harness. A failed or interrupted worker preserves its completed
sample/timing journals and the controller removes only its generated inputs.
The inventory is a separate diagnostic schema and cannot replace calibration or
any of the original full release cases.

Formal capture now orchestrates the layout supplement automatically, after the
entire original profile campaign passes its quality, repeat and regression gates.
The collector itself must be clean and match the frozen candidate tree/lock;
this is checked before builds or calibration, so a different tool checkout cannot
spend a campaign collecting evidence that release verification would reject.
It closes both formal overlays before starting the existing layout collector.
The supplement uses one count frozen from the maximum of the two releases' common
calibrated `verify_with_buffer:4:2:1024` and `verify_with_buffer:4:2:4096` counts.
No layout is selected or silently retried. A failed original campaign never spends
time collecting a supplement that could not approve release.

The existing layout tool also supports an independent **A1/B1/B2/A2** supplement
for all 16 combinations of source/scratch offsets `0/16/32/48`, at 1KiB and 4KiB.
Every phase runs the complete forward and reverse grid with the same backing
buffers per length. The buffers align to `max(actual OS base page size, 4096)`
before applying the selected offset, so both page and cache-line offsets match
across processes. This controls virtual offsets, not physical/huge-page placement.
The original Vec release cases retain their original API and allocation policies.

```bash
# Choose one bounded iteration count before starting; it is identical for every
# layout and phase. There is no retry, adaptive cell selection or pooled summary.
python3 candidate/scripts/diagnose_verify_layout.py \
  --baseline /test/baseline --candidate /test/candidate \
  --iterations 262144 --output /test/new-layout-pair \
  --isolation-cgroup /sys/fs/cgroup/reserved-release

# Recompute every result from the source-bound raw records and numeric journals.
python3 candidate/scripts/diagnose_verify_layout.py \
  --verify /test/new-layout-pair --candidate /test/candidate

# The single-source mode remains available for within-version diagnosis.
python3 candidate/scripts/diagnose_verify_layout.py \
  --repository /test/baseline --output /test/new-layout-diagnostic \
  --isolation-cgroup /sys/fs/cgroup/reserved-release
```

A complete pair contains **3,072 observations** (768 warmups and 2,304 measured
samples), keeping each length, offset pair and pass separate. Each measured loop
must reach 50ms; its wall and timed interval are the same complete verify loop,
so it also meets the 2ms timed-work minimum. The central distribution width must
be at most 10%. Forward/reverse repeats and A1/A2, B1/B2 repeats must stay within
5%; each of the four candidate/baseline comparisons must satisfy the existing
buffered-verify 12% latency regression limit in every cell. Ratios are withheld
when any quality check fails; a single regressing cell fails the supplement.
A failed complete phase retains all rows and stops later phases. No failure is
filtered or silently retried; the predeclared count can be insufficient, in which
case the attempt is retained and rejected.

The controller pins its worker process to one reserved CPU before codec
construction; the worker checks the actual thread affinity before and after the
grid. Raw records bind backend, features, target and actual page offsets. Per-phase
provenance binds source tree/lock, tool/template/renderer digests, compiler, CPU,
allocator settings and exclusive resource context. A manifest hashes each phase's
provenance, samples and raw journal. Replay rejects missing/reordered observations,
wrong source/context/layout, modified members, symlinks and journal disagreement.
The controller reaps its owned worker on handled cancellation before removing
exactly owned overlay inputs; modified inputs are retained for inspection.

`layout_gate_passed=true` approves only this slice supplement. Every report keeps
`release_approval=false`: this schema cannot replace any of the original 126
Vec/stream/hotspot cases or the strict release manifest. Schema 3 requires the
supplement at `layout-pair/manifest.json`, binds its digest, and independently
checks its source identities, complete resource context, frozen count and native
backend/features/target against the corresponding original cases. Only both
validated components can produce a complete release success report or stage
preflight inputs. No real passing paired
capture is claimed merely by adding the runner or testing it. The remaining
[layout acceptance task](https://github.com/rustfs/backlog/issues/2897) still needs
comparable evidence for dynamic paths, real per-layout results, a fresh complete
9.0.0/candidate ABBA and positive strict preflight. Historical failures retain their
original schemas/protocols and remain available for audit.

`--check` runs native guard/page-offset correctness tests. On Linux it also executes
the real pinned worker at one iteration and validates its entire JSON grid. This
is a short correctness self-check, not calibration or performance acceptance.

The original Git queries and JSON/CSV exporters run after the entire fixed kernel,
so their allocations, output-file writes and child processes do not seed the
next observation. A mode-independent `RSE_TIMING` journal preserves completed
timing observations if a process is interrupted; original rows are exported for
all completed observations before a workload panic is rethrown. The controller
writes source/protocol/context provenance before calibration, so an interrupted
attempt retains its source binding even without a final manifest.
All three warmups are retained, and none of the nine samples can be replaced or
silently retried. All pilot,
warmup and sample exports are retained. A sample needs at least 50 ms wall time
and 2 ms timed work; its interval must agree with the original raw metric.
The summary must be the actual median of the nine samples. The width between
the second and eighth sorted samples may not exceed 10% of the median; all nine
samples remain in the evidence, including extremes. This distribution gate is
additional to the unchanged 5% A1/A2 and B1/B2 drift limits and existing regression
thresholds. It does not assume that sequential samples are independent.

| Profile | Cases per phase |
|---|---:|
| Smoke, extended | 15 |
| Small files, fast | 88 |
| Reconstruction hotspots | 8 |
| Streams, fast, auto I/O | 15 |

The main test thread records CPU IDs and scheduling run/wait/switch counters
before and after each invocation. These observations help diagnose migrations
and scheduling delay; they do not establish physical-host exclusivity or observe
every Rayon worker. Fixed 10-second cooling intervals precede each complete
A1/B1/B2/A2 phase.

No case filter is accepted. Both A and B repeat drift must be at most 5% for every
case; all four A/B pairings must also pass the existing per-operation regression
thresholds. Failed runs preserve raw results and a failure report, withhold
candidate attribution, and cannot be used for release. Do not retry selected cases
or raise thresholds to obtain approval. The historical rejected 66-case campaign
remains rejected; this is a separate whole-release cohort. Different dependency
locks are recorded as part of comparing releases, so results cannot attribute a
change to one optimization.

## Publish a data snapshot after acceptance

First run `verify` on the final candidate and inspect the successful report:

```bash
python3 candidate/scripts/release_evidence.py verify \
  --repository candidate --manifest /test/new-campaign/manifest.json \
  --report /test/verified-report.json
```

Create a separate data-only Git branch in the same repository. Its
`release-evidence/` directory must contain the accepted `manifest.json` and the
sixteen summary JSON members plus every declared per-profile sample and
calibration JSON (`A1/`, `B1/`, `B2/`, `A2/`, `calibration-baseline/`,
`calibration-candidate/`). Copy only these declared files, never raw logs, local paths, credentials, host/process inventories or
executable hashes. Commit and push it, then set repository variable
`RSE_RELEASE_EVIDENCE_REF` to its full 40-character commit. A branch name, URL or
runner-local path is not an evidence reference. The source tree is not modified
by this data snapshot.

Checks bind 9.0.0's source, candidate tree, lockfile, unchanged benchmark harness,
compiler, CPU/resource context, backend, features, profile and I/O metadata. Every
file is bounded, digest-checked and required to match the complete profile
catalog. Schema 1 smoke omits its profile label; the manifest settings, complete
15-case catalog and exact exported iteration count bind its extended profile.
Empty, duplicate, missing, extra, malformed, non-finite, self-comparison
or incompatible data fails closed. The snapshot is trusted maintainer-supplied
measurement evidence; hashes detect changes, not fraudulent measurements.

## Preflight before tagging

```bash
gh workflow run ci.yml --ref <candidate-branch> \
  -f release_preflight=true -f evidence_ref=<full-data-commit>
```

The normal CI matrix and strict release checks run. The four quantitative gates
use verified recorded data from the reserved environment; they do not compare
measurements from a different, shared Actions CPU. Correctness tests, golden
vectors, backend consistency, SIMD override checks and the package dry-run still
execute on the candidate. Reports identify the current source, measured source,
data commit, digests, run and outcome; failed inputs and checks also upload reports.

Manual preflight **never publishes**. After review and rebase merge, confirm the
main tree matches the measured tree and the exact main commit passes CI. Only then
create the requested version tag. A tag runs the same strict checks again and
publishes only after their success, using the registry secret. A dry-run, ordinary
CI success or presence of the variable alone is insufficient release acceptance.

Local `VALIDATION_PROFILE=release` also accepts `RSE_RELEASE_EVIDENCE=<manifest>`.
Existing local baseline file inputs remain available for direct measurements;
they do not replace the mandatory immutable snapshot input in release CI.


Formal verification and materialization require manifest schema 3 and the current
sampling protocol (12). All acceptance thresholds remain unchanged; the new envelope makes the already required layout acceptance part of
the same automatic verification path. The snapshot must include the complete
`layout-pair/` directory, its four phases' provenance/samples/journals and its
manifest. Missing, changed, failed or independently collected incompatible
supplements cannot satisfy the contract. Legacy schema 1 mean-only campaigns and
schema 2 envelopes without bound layouts cannot approve release; every historical
raw observation remains under its original source/schema for audit.

## Why the earlier single-batch campaign was rejected

The complete earlier cohort remains rejected: 18/252 repeat checks exceeded 5%,
including 6 baseline and 12 candidate checks, with maximum drift 21.18%. Some
failing small-file batches lasted only 0.14–0.98 ms and stream batches 0.90–3.64 ms.
They had one mean per case and no independent warmup or sample distribution.
However, 260–294 ms batches also drifted; short intervals cannot explain every
failure. Whole-campaign CPU snapshots cannot determine the per-case effects of
migration, scheduler delay or clock frequency. The new protocol addresses the
confirmed measurement gaps and adds diagnostics; its implementation alone is not
accepted performance evidence or permission to publish.


## Private diagnosis on the existing VM

When the same baseline remains variable, collect a separate complete 9.0.0-only
resource diagnosis under the same reserved CPU partition:

```bash
python3 candidate/scripts/diagnose_release_vm.py \
  --baseline /test/fresh-baseline --output /test/new-private-diagnosis \
  --isolation-cgroup /sys/fs/cgroup/reserved-diagnosis
```

The sampler is shared, but diagnostic mode adds observations of every live
process thread and minor/major page faults, with anonymous stable thread slots.
It stores no raw process/thread IDs or start times. Snapshots occur outside the
operation timers and their cost is reported. They span the full invocation,
including input preparation and observation effects, so they cannot be treated
as hardware counters for the inner codec interval. New/departed threads are
reported rather than silently included in cumulative differences.

The complete 126-case catalog is calibrated, warmed and sampled nine times on
published 9.0.0. A new output directory is created with mode 0700; retain these
observations privately. The manifest identifies baseline and diagnostic-tool
sources and uses `baseline-vm-diagnostic-v1`, not a release-evidence schema.
Short diagnostic samples are retained and flagged, not discarded. Formal release
checks still reject insufficient intervals. Successful collection means data is
available, not that performance passed.
The extra process observation is deliberately rejected by the formal release
telemetry validator. It cannot substitute for complete A1/B1/B2/A2 acceptance.

Linux counters are interpreted using the
[/proc stat definitions](https://docs.kernel.org/filesystems/proc.html) and
[schedstat definitions](https://docs.kernel.org/scheduler/sched-stats.html).
Hardware cycles/cache misses and frequency were unavailable in the observed VM;
no particular frequency or physical-host cause is inferred from that absence.

Allocator state is a diagnostic variable: a separate syscall trace can identify
page reclamation (`MADV_DONTNEED`) and repeated minor faults. Preserve the trace
privately and state whether its interval includes fixture/observer work. Tracing
and allocator-policy probes are separate experiments; neither is release
acceptance. A probe that changes both mmap and trim controls identifies the
combined policy effect, not either control in isolation. The
[GNU libc tunables documentation](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html)
describes the dynamic allocation thresholds. Do not remove allocation costs,
relax gates, or selectively rerun cases to hide an unstable baseline.

## Published rejected audit snapshot

The [complete protocol-2 rejected campaign](https://github.com/houseme/rustfs-erasure-codec/tree/f12cbb675e9379d58bfc7217c41f6415778e1c83)
retains every calibration/warmup/sample JSON and file binding without changing
numeric observations. It is audit data, not an accepted baseline. The
[online real-data negative preflight](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39114330164)
rejected unstable distributions on main `8f775a2`, and skipped release-preflight
and publication. The input's complete validator file set was reviewed for
credentials, internal addresses and private paths; ancillary SSH/launcher logs
remain private. Do not configure this rejected commit as accepted evidence.
Protocol 4 requires new complete calibration and capture on both releases;
previous timing distributions cannot establish its acceptance.

A complete protocol-3 baseline diagnostic was also rejected: 101 short samples,
21 unstable distributions across two phases, and 12/126 repeat drifts above 5%.
Its raw traces exposed the missing warmup rehearsal in calibration. Protocol 4
repairs that contract; its implementation and unit tests are not performance
acceptance, and the earlier rejected observations are not discarded.
