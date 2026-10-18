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
edited. The external adapter excludes one-time fixture preparation from the
small-file and stream timers on both versions, with a checked reversible transform
that preserves all loop bodies and per-iteration allocations. Smoke keeps its
original per-iteration workload; hotspots keep their original inner recovery
intervals. Timing scopes are recorded, so the older cohort is not quantitatively
comparable to these new summaries. The candidate tree binds the committed renderer/templates. Both use
locked release builds, `std simd-accel`, no instrumentation, four Rayon workers,
a four-job policy and empty RUSTFLAGS.

Every case is calibrated on both releases before A1. Calibration targets at least
100 ms wall time and 5 ms timed work, with bounded increasing iterations. The larger
iteration count for each case is frozen for **both releases and every phase**.
Every phase then warms each case and collects exactly nine samples. All pilot,
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
Successful collection means data is available, not that performance passed.
The extra process observation is deliberately rejected by the formal release
telemetry validator. It cannot substitute for complete A1/B1/B2/A2 acceptance.

Linux counters are interpreted using the
[/proc stat definitions](https://docs.kernel.org/filesystems/proc.html) and
[schedstat definitions](https://docs.kernel.org/scheduler/sched-stats.html).
Hardware cycles/cache misses and frequency were unavailable in the observed VM;
no particular frequency or physical-host cause is inferred from that absence.
