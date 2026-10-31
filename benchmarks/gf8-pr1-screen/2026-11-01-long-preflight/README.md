# Longer default-caller baseline preflight

This attempt prepared two same-source copies of commit
`d584af6a139296b02f59e5b67a48fde8effb1481` with the temporary retained-row
bridge from [`../2026-10-31-freeze-7587132/`](../2026-10-31-freeze-7587132/).
A temporary package-version change allowed the two copies to link into one
process; the remaining files were identical. The separately built candidate
also applied the [byte-balanced scheduler patch](../2026-11-01-byte-balanced/).
Neither the bridge nor the candidate changes product code.

`probe.rs` uses the ordinary external caller with four Rayon workers, retains
and verifies output, and performs the 4+2 control first. Compared with the
prior 300 ms samples and one-second warmup, each phase was configured for a
two-second warmup and three 500 ms samples. The strict streaming runner from
commit `70898c1` was configured to require two complete same-source baseline
rounds before launching the candidate, with all existing within-round,
cross-round, same-source and resource gates unchanged. A successful isolated
row comparison would still require a separate end-to-end acceptance run.

The monitor rejected the initial state because its named-process check was
active. Only one monitor record and a failure summary were produced; no
baseline sample or candidate process was started. The monitor did not record
the competing process identity. A later independent observation saw active
baseline/candidate processes and an effective CPU set reduced to 0–7, but
cannot establish which process triggered the initial rejection. No process was
stopped. `results/` preserves the original refusal; it contains no codec timing
and supports no candidate performance conclusion.

The source revision has since advanced. Any later preflight must use freshly
rebuilt, provenance-matched copies of the then-current source and wait until
the competing tasks have ended and the reserved CPU set is restored.

## Reserved-window retained-row comparison

After the earlier competing workload ended, a separate attempt refreshed the
snapshot to commit `6855f44f40a90d20ca3227c58d97388e36b4f42d` and rebuilt both
probe binaries. The two baseline codec trees differed only in their temporary
package version; the candidate tree also applied the byte-balanced patch.
`results-6855/` contains two same-source baseline rounds followed by two
candidate rounds, with all output bytes checked after each phase.

All specified within-round, same-source, cross-round and resource gates passed.
Across the four rounds, the A-level range was under 5% for every case. The
monitor recorded 804 observations, effective CPUs 0–15 throughout, no named
competitor, and a maximum off-target busy streak of one observation. These
observations do not exclude shorter interference or unobserved host effects.

Candidate B versus its paired A had these mean-of-phase-median latency changes:

| Retained recovery row | Round 1 | Round 2 |
| --- | ---: | ---: |
| 4+2, 1 MiB, one missing data | -29.75% | -29.63% |
| 10+4, 1 MiB, one missing data | -9.89% | -10.41% |
| 10+4, 1 MiB + 1 byte, one missing data | -10.01% | -9.93% |

These are isolated retained-output row measurements on one virtual machine,
not end-to-end recovery throughput, tail latency, or a general production
claim. They do not cover GF16, other job counts, output counts, small sizes, or
other architectures. The candidate remains disabled in production.

`end_to_end_probe.rs` prepares an independent Option-based
`reconstruct_data_opt` experiment: each iteration marks one data shard missing,
reconstructs it, and checks recovered bytes after each phase. Its two-second
warmup and three 500 ms samples match the row experiment; the runner's expected
case identifiers are changed by `end-to-end-runner-cases.patch` without changing
its gates. The probe compiled locally; a separate scalar smoke run used 5 ms
phases and verified recovered bytes for all three cases. That smoke run used
different CPU availability and was not a performance measurement. A later
competing workload reduced the CPU set to 0–7, so the remote build was stopped
before it began and **no native end-to-end timing has run**. A new quiet
preflight is still required before running the candidate.
