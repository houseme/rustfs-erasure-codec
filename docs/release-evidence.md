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

This invokes real Cargo benchmarks, with locked release builds, `std simd-accel`,
no instrumentation, four Rayon workers, a four-job policy and empty RUSTFLAGS. It
clears inherited benchmark/policy tuning, builds both revisions first and runs all
four profiles in **A1/B1/B2/A2** order with fixed 10-second cooling intervals:

| Profile | Exported cases | Iterations |
|---|---:|---:|
| Smoke, extended | 15 | 200 |
| Small files, fast | 88 | 1000 |
| Reconstruction hotspots | 8 | 100 |
| Streams, fast, auto I/O | 15 | 30 |

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
sixteen declared JSON members (`A1/`, `B1/`, `B2/`, `A2/`). Copy only these declared
files, never raw logs, local paths, credentials, host/process inventories or
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
