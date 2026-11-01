# Current-source end-to-end recovery comparison

This attempt used committed source
`bd71debd7180673e6751bec064cdb079219e6f94`, including the newer GF16 runtime
changes. Two baseline copies differed only by the temporary package version;
the candidate copy also applied the experimental byte-balanced scheduler
patch. The temporary benchmark bridge and the Option-based recovery probe are
documented in
[`../2026-11-01-long-preflight/README.md`](../2026-11-01-long-preflight/README.md).
The probe uses the default external caller, five available CPUs, four Rayon
workers and four jobs. It checks recovered bytes after every phase.

Two same-source baseline rounds passed the existing resource, within-round,
same-source and cross-round gates before candidate measurement began. In the
second candidate round, the 10+4 / 1 MiB + 1 / missing-data-3 case had +5.74%
A-level cross-round drift. The existing 5% gate rejected the complete
comparison and did not acknowledge the failing case. The raw recorded case is
retained for audit; do not use any candidate phase to claim a gain or
regression. This attempt does not rehabilitate earlier failed comparisons.

`results/` retains both baseline rounds, both candidate rounds and the original
monitor/summary. The monitor recorded 804 observations with CPUs 0–15, no named
competitor and a maximum off-target busy streak of two observations, below the
three-observation stop threshold. This cannot rule out shorter interference,
cache state, allocator behavior or unobservable host effects. The reason for
the measured drift is unknown; no production scheduler change is enabled.

Further acceptance requires a materially improved stable measurement setup,
then fresh exact-source baseline and candidate gates, end-to-end coverage for
GF8 and GF16, sizes and job counts, and functional compatibility validation.

## Cross-arm baseline guard

The historical streaming gate compared repeated baseline rounds with each
other and repeated candidate rounds with each other, but did not automatically
compare the candidate round's A level against both preflight A levels. A stable
shift between those groups could therefore escape that particular check.
`cross-arm-gate.patch` adds a fail-closed comparison against both validated
preflight A levels before acknowledging each candidate case. A missing,
malformed or more-than-5% shifted reference rejects the run. This is an
additional gate; the original phase, same-source, cross-round and resource
checks remain in place. It does not change the outcome of this already-rejected
experiment.

The patch applies without fuzz to the archived
`2026-11-01-affinity-diagnostic/run_streaming_latest.py` from commit `70898c1`;
the existing end-to-end case-ID patch also applies afterward. The companion
`cross-arm-tests.patch` applies to the archived
`2026-10-31-kernel-isolation/test_streaming_gate.py` from the same commit.
With the existing recorded fixtures, 22 local tests passed, including missing
or invalid references, a candidate level that differs from only one preflight,
and rejection before the case is acknowledged. This guard has not been used to
run a new candidate comparison.
