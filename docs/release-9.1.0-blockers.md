# 9.1.0 publication blockers and completion criteria

Audit date: 2026-10-20 (Asia/Shanghai). Reviewed main:
`475be6cdd55cdb4f2d6dcf4457a6e2a943374757`, after PRs #40–#43.
The manifests already declare 9.1.0. No accepted final timing campaign, positive
strict release preflight, tag or publication is recorded. This assessment keeps
historical measurements bound to their original sources.

## Production progress and actual version differences

Recent production fixes include excess-shard error classification before stream
I/O, Leopard-compatible payload splitting, validation before progressive XOR
merges, and bounded GF8/GF16 exponentiation. The accompanying join fix permits a
small requested prefix when repeated borrowed shards exceed the platform's
aggregate `usize` range. These are functional changes, not just benchmark changes.
Normal field definitions, matrices and compatible encoded bytes are preserved.
Generic reconstruction still keeps decode-before-publication where required,
late failures and selective-output behavior.

The [version delta](9.0.0-to-9.1.0-codec-delta.md) records a source-bound normal-
format 200-case byte/roundtrip comparison and 15 allocation scenarios. It excludes
historically inconsistent fast/custom/mixed-entry codewords. Recorded allocation
requests per API invocation include:

| Scenario | Published 9.0.0 | Measured 9.1.0 source |
| --- | ---: | ---: |
| Classic warmed encode | 0 | 0 |
| Classic cached single-output recovery | 1 | 1 |
| Classic cold recovery | 6 | 2 |
| Leopard GF8 / GF16 warmed encode | 7 / 19 | 0 / 0 |
| Leopard GF8 / GF16 warmed recovery | 27 / 40 | 2 / 2 |
| Multiblock stream encode / verify / recovery | 41 / 40 / 52 | 8 / 10 / 10 |

These count allocation plus reallocation requests, not throughput, latency, RSS
or peak memory. The report names its measured source; later production fixes
must not be relabeled as that measurement. No accepted percentage throughput
change between published 9.0.0 and the final 9.1.0 source is currently available.

## What the remaining drift actually establishes

The [complete service review](baseline-service-review-2026-10-20.md) retains the
sources, full catalogs and diagnostic scope. Both controls compare **9.0.0 with
itself**, never a candidate with its baseline:

| Complete control | Repeat failures above 5% | Failed signed changes |
| --- | ---: | --- |
| Protocol-15 phase-major A1/A2, all 126 cases | 3 / 126 | -6.5942%, +6.7256%, +5.1983% |
| Adjacent diagnostic A1/A2, all 126 cases | 2 / 126 | Encode 10+4/512 KiB: -13.1933%; recovery 4+2/64 KiB: +8.0321% |

Neither control has short samples or central-width failures. The adjacent
schedule is diagnostic only; it cannot replace formal qualification. Passing the
previous three failures there does not repair either complete control.

In the failed cases, formal observations have zero process minor/major faults
and at most about 0.0147% calling-thread runnable wait relative to wall time.
A fixed eight-worker diagnostic further bounds work outside original wall
windows below 0.311%; the four encode medians still span 15.344%, versus 2.167%
for whole-worker APERF/MPERF. Those virtualized, non-atomic guest counters cannot
serve as exclusive API cycles or normalize timings. The residual lies inside
original operation windows; measured wrapper cost, faults, wait and observed
execution-rate changes do not individually explain its magnitude.

Some earlier test fixtures and metadata contracts were defective and have been
repaired. Residual variability also occurs in unchanged 9.0.0 with unchanged
worker binaries and retained encode buffers. It is therefore neither evidence
of a 9.1.0 regression nor proof that all drift came from test code. Cache/memory
service, remaining internal layout/state and invisible physical-host contention
remain unseparated. Guest CPU reservation does not reserve physical cache or
memory bandwidth; the current VM lacks usable CPU/uncore PMU and resctrl data.

## Current local Docker ARM64 status

The [2026-10-20 local ARM64 preflight](docker-arm64-preflight-2026-10-20.md)
completed all 72 existing recovery-path cases and 216 samples with the unchanged
runtime source and complete correctness checks. Raw measurements remain private.
Local OrbStack cannot provide a valid delegated exclusive cpuset partition:
the owned partition is invalid and ancestor writes return EPERM. Probe state was
restored; the formal collector still refuses the environment. The CPU identity
correction removes a separate x86-specific provenance assumption, without
relaxing isolation or any performance gate. No formal baseline/ABBA, layout or
release acceptance follows from this diagnostic.

## Backlog #2897 completion marks

Only condition 4 is checked within its declared normal-format scope. Keep the
[canonical issue](https://github.com/rustfs/backlog/issues/2897) open:

| Condition | State | Concrete remaining acceptance |
| --- | --- | --- |
| 1: Buffer entry/layout/lifetime | Partial | Retained geometry covers 103 caller cases; selected ordinary-verify and stream internals have native diagnostics. Complete the agreed dynamic-output/internal-scratch and remaining source-position scope with matching evidence. |
| 2: Calibration/measurement lifetime | Partial | All 126 cases have explicit preparation/input contracts, but caller matching and one-iteration inventories do not establish every dynamic allocation, clone, clear and release interaction. |
| 3: 1/4 KiB layouts | Partial | The paired supplement is implemented. Obtain real passing A1/B1/B2/A2 results for all 16 source/scratch offset combinations at each size, kept separate from original Vec cases. |
| 4: Compatibility | Complete in declared scope | Retain normal-format byte parity and ordinary/optimized, invalid-input and missing-shard behavior tests on the final candidate; do not expand the legacy compatibility promise. |
| 5: Protocol/source/rejection | Partial | Shared source/raw validators and refusal tests exist. Final complete accepted capture/replay must bind the final source, lockfile, renderer, context and every original record. |
| 6: Full performance acceptance | Incomplete | A stable complete baseline, then full final candidate/layout ABBA and strict positive preflight. Both existing complete controls remain rejected. |

## Release sequence and benchmark decision

1. Finish and review concrete functional fixes, run ordinary compatibility/tests,
   generated Node/Chromium WASM integration and package dry-run, then rebase merge
   passing PRs. Freeze the complete final source/lock/harness before final capture.
2. Complete the outstanding workload/layout acceptance above. Before another full
   baseline run, establish a distinguishing observation or a controlled condition
   addressing residual execution service or internal-layout variability. Do not
   add speculative runtime tuning or retry the unchanged failed matrix.
3. Qualify all 126 baseline cases under the existing 5% repeat / 10% central-width
   and duration gates. Then collect complete final baseline/candidate A1/B1/B2/A2
   and the required 16-layout paired supplement with matched compiler, features,
   backend, I/O, input, count, CPU and memory-policy context. Retain all failures;
   no favorable case/count selection, time normalization or threshold relaxation.
4. Verify the accepted immutable evidence data commit. At this audit,
   `RSE_RELEASE_EVIDENCE_REF` is absent. Set its full commit SHA, or supply manual
   `evidence_ref`, only after acceptance. Rejected audit snapshots are not inputs.
5. Run CI manually on final main with `release_preflight=true` and that full
   `evidence_ref`; verify actual success. Ordinary PR/main CI skips these strict
   evidence/preflight jobs, so green ordinary CI is insufficient.
6. Only then create the matching 9.1.0 tag and verify actual registry publication.
   A version in Cargo.toml, a package dry-run or an available secret is not upload.

Benchmark preparation and functional/load smoke tests can proceed now. A formal
candidate comparison cannot yet support release attribution on the unchanged VM:
its baseline self-control fails. A new controlled condition must first pass the
baseline prerequisite. Existing workers and gates can then be used directly;
no additional general-purpose diagnostic framework is needed. A RustFS/S3 stress
comparison is a separate downstream workload, not a substitute for codec ABBA.

Native ARM and downstream/S3 throughput remain independently tracked in
[#2853](https://github.com/rustfs/backlog/issues/2853). Docker ARM and compile/mock
checks cannot certify native throughput. Those P2 tasks can remain separately
tracked only when actual release gates pass and release notes avoid unsupported
speed claims; they do not waive #2897 or the retained performance gate.

## Historical audit records

Earlier rejected controls remain auditable under their frozen identities:
[protocol 10](https://github.com/houseme/rustfs-erasure-codec/tree/75456a77a57953f8b069bab3d2eb788643ca21d1/diagnostics/protocol10-qualification-controls),
[protocol 13](https://github.com/houseme/rustfs-erasure-codec/tree/dc05250/diagnostics/protocol13-baseline-qualification).
Use the [evidence procedure](release-evidence.md) for current commands and exact
contracts. No historical rejection is promoted by documentation or code merges.
