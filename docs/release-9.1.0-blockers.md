# 9.1.0 publication blockers and completion criteria

Audit date: 2026-10-19 (Asia/Shanghai). Reference main:
`2fb7f38926d7c95395707cc060785a9f0e23f8a6`, after PR #17.
This is a release-readiness analysis, not release approval.

## What currently prevents publication

| Layer | Verified state | Required next result |
| --- | --- | --- |
| Functional compatibility | The normal-format 200-case published-9.0.0 parity/roundtrip probe and core correctness tests pass; this work changes no production algorithm. Historical inconsistent fast/custom/mixed-entry codewords retain the documented migration boundary. | Preserve the tests and state the compatibility boundary in release notes. |
| Known Vec geometry | Protocol 6 records base-page provenance and all 66 structural comparisons across the eleven known buffered-verification cases match. The workers use one iteration. | Qualify real sample durations, distributions and both-version repeat drift; structural matches are insufficient. |
| Other workload contracts | Dynamic clone/output/recovery paths and complete per-layout coverage remain partial in backlog #2897. Stream output reservation also needed a fixture correction: cloning an empty reserved Vec loses the reservation. | Complete the relevant allocation/lifetime evidence and use one explicit shared workload on both versions. |
| Quantitative evidence | No accepted complete campaign for the final source tree and current sampler has been established. Historical failures are retained; they cannot qualify changed code or protocols. | A fresh complete 9.0.0/candidate A1/B1/B2/A2 with all 126 original cases, both calibrations and raw observations; all existing gates must pass. |
| Workflow input | Repository variable `RSE_RELEASE_EVIDENCE_REF` was absent when checked. Audit snapshots and structural inventories have not been promoted to accepted inputs. | Publish accepted evidence at `release-evidence/manifest.json` on a data commit, then configure its full immutable SHA or supply `evidence_ref` during manual dispatch. |
| Strict preflight | Ordinary CI runs do not execute the tag/manual-only evidence and release-preflight jobs. The earlier real-data negative check proved rejection, not positive acceptance. | Dispatch with `release_preflight=true` against the final main and accepted evidence; verify actual success before tagging. |
| Upload configuration | The repository contains a secret named `CRATES_IO_TOKEN`; its contents were not read. Version/packaging checks are already part of CI. | After all gates pass, tag the validated source and verify the actual publish job. A secret name or dry-run alone is not an upload. |

The immediate automated stop is explicit in `.github/workflows/ci.yml`:
`release-inputs` requires a full immutable evidence SHA, validates its complete
source-bound campaign, and is a dependency of `release-preflight`. `publish`
depends on `release-preflight` and runs only for a tag. Missing or rejected
evidence stops this chain even when ordinary CI is green. GitHub does not inspect
issue checkboxes; those represent the additional agreed acceptance scope.

No new blocking algorithm defect is established by the current layout diagnosis.
The confirmed historical causes include incomplete calibration rehearsal,
measurement-only state/export effects, changing Vec fixtures, and codec setup
allocations affecting cross-version buffer placement. The latter issues have
been addressed for the eleven known Vec cases. Remaining timing variability
cannot all be attributed to alignment: a same-buffer layout diagnostic retained
an unstable distribution. Scalar/register CPU controls do not establish the
stability of allocation, memory or downstream paths. New complete measurements
are needed before assigning remaining causes or asserting a speedup.

## What is still required

1. Finish the remaining #2897 workload contracts and per-layout coverage. Keep
   the original Vec cases; a slice diagnostic cannot silently replace them.
   The shared release stream fixture independently reserves each writer while
   retaining its original per-iteration allocation cost and timing scope.
2. Run complete baseline controls with the declared protocol and allocator policy.
   Stop attribution when duration, distribution, layout or repeat controls fail.
   A new main/protocol-6 full baseline control was started for this audit and is
   recorded separately from the subsequent stream-fixture correction.
3. Freeze the final candidate tree, lockfile and shared harness, then collect the
   complete paired campaign. Keep the 5% repeat-drift limit, 10% distribution-width
   limit, minimum intervals and existing per-operation regression thresholds.
   Do not drop failed cases or compare different workload policies.
4. Preserve and publish every raw observation and provenance member. Verify the
   immutable accepted snapshot locally and in the manual strict preflight.
   Code or documentation changes after capture require new evidence; an unchanged
   complete tree after rebase merging is accepted under the recorded policy.
5. Confirm the validated final main, tag/version/lock consistency and successful
   strict preflight; only then create the release tag and verify upload.

Native ARM performance and downstream/S3 throughput remain independently tracked
in #2853. The existing release checklist allows a correctness/API release to keep
that P2 task open if the actual release gates pass and the notes make no unsupported
throughput claims. An open P2 task is not itself an automated tag gate. The full
performance evidence and strict preflight requirements above still apply under
the current repository policy; this work does not weaken them.

## Issue status accounting

The top-level conditions in #2897 remain: condition 4 checked within its declared
normal-format compatibility scope; conditions 1/2/3/5 partially complete and
unchecked; condition 6 incomplete. PR #17 is merged, so references to it as pending
must be removed. Additional fixture fixes and observations update the evidence
under each condition, rather than automatically completing the whole condition.
