# Rejected 9.1.0 controlled campaign — audit data only

This data-only branch is not a release or an accepted performance baseline. Do not merge it into main or configure it as `RSE_RELEASE_EVIDENCE_REF`.

The complete published-9.0.0/candidate A1/B1/B2/A2 campaign contains 126 cases per phase, 504 distributions and 4,536 formal samples. All calibration probes, warmups, raw samples, normalized summaries, frozen counts and source/runtime metadata are retained without numeric redaction. Every copied evidence member is byte-for-byte identical to the original capture; SHA256SUMS lists the published files.

The current validator rejects 10/504 sample distributions. Independent same-version diagnostic replay also found 11/252 repeat drifts above 5%. No baseline/candidate performance attribution or release approval follows from this snapshot. The distribution rejection happens before candidate regression comparisons.

The measured candidate commit is recorded in the manifest. Main 8f775a2dff51740e766ea51eaa628cc78d795d08 has the identical complete source tree. Sampling protocol and implementation hashes remain bound by the manifest. Both versions use the same explicit combined mmap/trim policy; results do not establish default-allocator or downstream throughput.

The snapshot intentionally excludes ancillary build/launcher logs, SSH/controller scripts and unrelated process/host telemetry containing internal paths or identities. Those private diagnostic files do not participate in release-evidence validation; all validator inputs and raw performance observations are present. The small isolation summary contains restoration outcomes only. No user objects, customer data or credentials are included.

Use main's `scripts/release_evidence.py verify --manifest <checkout>/release-evidence/manifest.json` to reproduce the rejection. Manual workflow dispatch with `release_preflight=true` and this immutable data commit tests online rejection only; it never publishes. A fresh accepted complete campaign and positive strict preflight are still required before tagging 9.1.0.
