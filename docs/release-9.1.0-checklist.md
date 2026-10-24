# 9.1.0 release preparation

Windows parity-row validation (2026-10-25): test/workflow fix `ad00e79` keeps
runtime `31ccf7e` unchanged. All three Windows toolchains now pass the complete std
suite and explicit 270-case debug/release contracts. The [Windows context review](borrowed-parity-rows-review-2026-10-25.md#windows-debug-allocation-context)
retains the three original failures, exact policy controls, absolute-zero release
results, complete case records and 12-input independent replay. The corrected gate
still rejects 162 old wide allocations in each local parent profile. This is
functional/resource validation; combined-source and performance/preflight gates
remain outstanding.

Borrowed parity rows (2026-10-25): runtime `31ccf7e` on main `e758f11` replaces
per-entry parity-row reference tables with a static borrowed matrix range. The
[complete source-bound review](borrowed-parity-rows-review-2026-10-25.md) retains
all 270 controls: 162 wide cases decrease from 1 / 4 / 10 / 33 allocation requests
to zero, and 108 small controls remain zero. Fresh 573 parent / 268 no_std / 317 tool
tests, published-9.0.0 and WASM compatibility pass; package inventories remain 71/6.
Separate recovery PR #106 is not included. Allocation evidence is not end-to-end
throughput or release approval; final combined source, accepted full performance
evidence and strict positive preflight remain outstanding.

Distribution compatibility (2026-10-25): merged main `4353935` has the same
complete Git tree as executed `bfac8fc` / runtime `4aa4ee0`. The
[artifact-level record](inverse-work-review-2026-10-25.md#distribution-and-consumer-compatibility)
verifies the 71-member core crate, extracted 395 library / 266 no_std tests,
real WASM generation/typecheck, 14 Node tests in both built and extracted NPM
packages, offline consumer exports/recovery/types and the successful Node/Chromium
CI job. Sixteen original inputs are independently replayed unchanged without
codec calls. No registry upload or performance approval is produced; the separate
26-command native continuation and final release gates remain outstanding.

Direct inverse work (2026-10-25): source `4aa4ee0` removes a temporary identity
matrix while preserving Gaussian arithmetic and row widths. The
[source-bound review](inverse-work-review-2026-10-25.md) records six cold-recovery
allocation controls (10 to 9 requests), 569 local codec parent tests / 266 no_std,
317 tool tests, published-version compatibility and 585 native parent tests.
The native pipeline has eight completed commands, then a preserved strict
ownership refusal caused by a foreign cargo task; its restored lease is closed.
The remaining 26-command continuation is prepared with zero launches while
resources are busy. No complete native, performance or release acceptance is
claimed for this new source.

Combined native integration (2026-10-24): source `8ec2dde` merges main
`76897f6` and passes all 31 Linux functional commands: 582 parent tests,
six isolated child executions, 265 no_std, six-backend published-version recovery,
978 cache-layout checks and 18 warmed zero-allocation cases. The
[current native record](recovery-cache-selected-survivors-2026-10-24.md#combined-native-validation-2026-10-24)
includes the complete independent replay of 74 unchanged inputs and verified
resource/lease closure. Current local Rust tooling passes 312 tests. There are
zero performance observations; accepted complete ABBA/layout evidence and actual
strict positive preflight remain outstanding.

Prior recovery-cache candidate (2026-10-24): runtime `b0bd342`, integrated
source `d138103`, keys inverse reuse by the selected-survivor erasure prefix.
The [cache review](recovery-cache-selected-survivors-2026-10-24.md) records real
before/after allocation regressions, 978 independent layout checks, 565 local
parent codec tests / 264 no_std, published-format compatibility and 310 native
Rust tool tests. Its separate 31-command Linux plan was superseded before any
launch; the combined-source result above retains its own binding. No new latency or release acceptance is claimed; earlier proofs
and refusals below keep their own source bindings.

Current native proof (2026-10-24): main `544bb06`, including `727690f` GFNI
batching, now passes its own 29 Linux commands (575 parent codec tests / 261
no_std), both native row widths and six-backend published-format/selective
compatibility. The [batched-native review](gfni-batched-native-review-2026-10-24.md)
records 82-input exact replay, shared-resource controls and the initial ACK
publication failure plus explicit atomic-ACK recovery without native retries.
Zero performance observations were collected. Final accepted complete evidence
and strict positive release preflight remain outstanding; earlier source-bound
refusals and snapshots below remain unchanged.

Latest synchronized main (2026-10-24): `727690f` adds four-vector GFNI matrix
reuse after the `3a62879` native snapshot. The [subsequent integration review](selective-parity-planning-2026-10-24.md#subsequent-main-synchronization-four-vector-matrix-reuse)
records fresh local dispatch/recovery/identity/buffer, no_std and cross-target
compile checks on merge `941489c`. It does not transfer the earlier native
execution or rejected timing results to the batching revision. Final native
source binding, accepted complete performance evidence and strict positive
release preflight remain outstanding.

Reviewed native snapshot (2026-10-24): `3a62879` includes the independently added
GFNI row-fusion runtime `c558ef8`, so it differs from frozen timing candidate
`b1a10e4`. The [later-main review](selective-parity-planning-2026-10-24.md#later-main-independent-gfni-row-fusion)
records 29 fresh native checks (575 parent codec cases / 261 no_std), positive
execution of both row-kernel widths, six-backend published compatibility and
43-input independent replay. Package lists are now 71/6. These are functional
results, not this later runtime's throughput acceptance. The completed old72
API comparison still has eleven repeat refusals and cannot be relabeled.
Final accepted complete evidence and strict positive release preflight remain
missing; older inventories below retain their source bindings.

Historical API diagnostic result (2026-10-24): the complete 72-case A1/B1/B2/A2
comparison of frozen `e3a1c1c` / `b1a10e4` has finished. All 288 rows / 864 samples
and zero execution errors independently replay exactly, with 46 inputs unchanged.
Eleven of 144 repeat checks fail (eight baseline, three candidate); all allocation
bookends repeat, but the whole catalogue remains unqualified for latency and
allocation attribution. The [complete selective recovery review](selective-parity-planning-2026-10-24.md#complete-72-case-api-result-repeatability-refusal)
retains every case, every ratio and the passing-target counterexample. No prior
measurement is relabeled and no unchanged retry is used. Final accepted full
published-baseline evidence and strict positive release preflight remain missing.

Historical selective-planning candidate (2026-10-24): `b1a10e4` is based on
main `e3a1c1c`. The [selective parity review](selective-parity-planning-2026-10-24.md)
records the systematic-row substitution, `k*d` rather than `k²` planning products
per requested parity row, independent dense oracles and 28 positive native Linux
commands. Published-format/selective publication checks pass across six backend
settings. Actual package lists are now 69/6, retaining the same include rule.
These are functional/work-count results; final accepted full ABBA/layout and
strict positive release preflight remain missing. Every older measurement and
validation inventory below keeps its original source and counting convention.

Historical integrated native review (2026-10-24): PR #84 has been externally
merged. Executed source `43d678b` integrates main `0b10e7a`; reviewed main
`801f499` has identical codec/test/build/WASM/workflow files and newer tool files.
The [integrated native proof](gfni-tower-fusion-review-2026-10-24.md#integrated-main-native-compatibility-and-static-attribution-review)
records all 21 Linux checks (562 parent codec cases / 252 no_std), six-backend published-format
compatibility and the exact inverse/quotient comparison. All six earlier tower
cohorts remain unaccepted. Static selected masks/tables match; caller geometry
and execution-cost attribution remain unqualified. No final accepted full
ABBA/layout evidence or strict positive preflight is recorded. Older source
inventories and validation counts below retain their original bindings.

Historical source review (2026-10-24): runtime `e38a8ad`, final validated test source
`7fa4fdd`, in draft PR #84 stacked on #81. These candidates are not merged into
reviewed main `afb9a52`. The [complete tower review](gfni-tower-fusion-review-2026-10-24.md)
retains all six rejected component cohorts; the final same-42-case comparison
has zero width/short failures but twelve repeat and ten regression refusals.
Compatibility and 21-command native validation pass; accepted final full-release
ABBA/layout evidence and strict positive preflight remain missing. Historical
source inventories below retain their original scope.

Status at the 2026-10-23 review: **implementation merged, performance acceptance
incomplete, not tagged or uploaded**. The
[current merged-candidate review](release-9.1.0-readiness-2026-10-23.md) records
reviewed main `0a85348`, its matrix/compatibility and bound-original replay
checks, and its differences from the completed frozen source `9f935ee` and earlier `449920e`. The earlier `14cf56e` file-tree equality is preserved as a
historical source relation; the completed affected-catalog comparison keeps its
original source labels and does not qualify the later combined implementation.
Its exact replay retains three failed repeats despite passing scoped regression
programs; see the [complete GFNI review](gfni-priority-review-2026-10-23.md). The
[earlier global review](global-review-9.1.0-2026-10-23.md) retains its own sources,
process-ownership fixes and complete Linux diagnostic history.
The [current readiness assessment](release-9.1.0-blockers.md) is the canonical
remaining-work list. It supersedes historical checklist snapshots, without
changing their source bindings or promoting rejected measurements.

The subsequently requested [complete diagnostic mode](release-evidence.md#complete-diagnostic-collection)
collects the full A1/B1/B2/A2 profiles and paired layouts before evaluating all
statistical failures. A distribution or repeat failure no longer truncates that
diagnostic campaign. Its distinct manifest/report remains separate from the
formal publication checks below, with all original observations and failures kept.

The [completed frozen Linux diagnostic](complete-linux-diagnostic-2026-10-22.md)
now retains all calibration, profile and layout observations and independently
replays exactly. Its failed distributions/repeats/regression checks remain
unaccepted. Its source identity and diagnostic-only manifest do not satisfy the
final-candidate publication boxes below.

The [later complete combined-source diagnostic](complete-linux-diagnostic-2026-10-24.md)
also finishes all 20,724 observations and an exact frozen replay. Its full retained
refusals still withhold approval. The [new GFNI shared-kernel candidate](gfni-direct-affine-review-2026-10-24.md)
requires its own complete component result and final-source evidence; the boxes
below are not checked by old-source diagnostics or ordinary CI.

## Compatibility and functional review

- Review the [incremental-update implementation and drift validation](drift-analysis-9.1.0-2026-10-22.md).
  Empty updates keep full error validation and avoid scratch; changed updates
  reuse the existing serial chunk policy and read matrix coefficients directly.
  The existing ordinary-CI report covers 45 resource scenarios: the original 15
  plus 30 update cases, including the 33-parity metadata-allocation boundary.
  Identical chunks now skip parity arithmetic; reserve/extend computes scratch
  directly without zero fill. Keep the sparse and dense Criterion cases and the
  owned/in-place caller lifetimes separate. Constructor-count checks and local
  service observations do not constitute formal timing acceptance.
- Retain the [Linux native follow-through](drift-analysis-9.1.0-2026-10-22.md#linux-原生功能与有界服务验证):
  eight incremental regressions, 200 normal-format compatibility cases and 45
  stable resource scenarios pass on merged main. The four-worker baseline-only
  diagnostic is a single-case window; its refused counter-alignment checks and
  unclosed kernel-marker checks cannot qualify the full baseline or final ABBA.
- Preserve normal encoded bytes and the existing field/matrix definitions. The
  [version delta](9.0.0-to-9.1.0-codec-delta.md) records the original 200-case
  comparison and source-bound allocation results. Refresh final-source checks;
  allocation reductions do not establish throughput improvements.
- Retain the declared fast/custom/mixed-entry compatibility boundary. Fixing old
  inconsistent single-parity behavior cannot promise recovery of codewords that
  were already inconsistent; rebuild parity from healthy data where necessary.
- Review actual production fixes, including stream count errors, Leopard split
  lengths, progressive merge validation, exponentiation and bounded-prefix join.
  Keep generic reconstruction publication, late-failure and selective-output
  semantics. Merge passing reviewed PRs by rebase before freezing the candidate.
- The workspace declares no MSRV. Edition 2024 and dependencies still impose
  compiler requirements; absence of a rust-version declaration is not evidence
  of compatibility with arbitrary older compilers.

Run relevant focused regressions, the workspace and no-default-features suites,
all-feature Clippy, formatting and manifest checks. Ordinary CI additionally
checks generated WASM Node/Chromium integration and actual package dry-run.
Record exact tested source and outcomes; historical test totals are not a test
result for new main. At reviewed runtime source `4f2799e`, the
[actual main CI](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39394001627)
passed real generated Node/Chromium WASM, 200 byte/roundtrip cases, 15 resource
scenarios, ordinary verify scratch, all stream byte-owner checks and package
dry-run. Its downloaded numeric reports were independently replayed/compared.
These are completed functional/structural checks, not strict release preflight.

```sh
RUSTUP_TOOLCHAIN=stable cargo test --locked --workspace
RUSTUP_TOOLCHAIN=stable cargo test --locked --package rustfs-erasure-codec --no-default-features --lib
RUSTUP_TOOLCHAIN=stable cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=stable cargo fmt --all --check
cargo xtask check-release --tag 9.1.0
```

## Remaining performance and publication gates

- [ ] Complete #2897 conditions 1/2/3/5 within their agreed dynamic-buffer,
      lifetime, paired-layout and final source/raw-evidence scope. Stream-layer
      byte owners and the remaining codec-internal decode scratch, non-stream
      outputs and smoke/hotspot sources are implemented and natively observed.
      The complete 516-worker two-source structural inventory independently
      replays; uninstrumented performance geometry and final acceptance remain.
      Condition 4 stays checked only within the declared normal-format
      compatibility scope.
- [ ] Establish a controlled condition or distinguishing observation for residual
      execution-service variability before a new full baseline campaign. The
      historical qualification failed 8/126 repeats despite passing all 252
      distributions; an earlier forced diagnostic stopped at its A1 distribution.
      The later complete diagnostic collected every planned phase and layout,
      retaining one profile distribution failure, nine profile repeat failures,
      sixteen regression-program refusals and sixteen cross-phase layout repeat
      failures. The new affected-catalog plan tests a changed candidate and does
      not replace those failures or qualify the complete baseline. Fixed
      frequency, PMU and unique historical-cause attribution are not additional
      release conditions.
- [ ] Freeze final source/lock/harness and qualify the complete baseline, retaining
      5% same-version drift, 10% central spread and uniform protocol-15 duration
      gates (2 s wall / 100 ms measured calibration targets; 1 s / 40 ms sample
      floors). Do not select favorable cases/counts or mix incompatible contexts.
- [ ] Collect and verify complete final A1/B1/B2/A2 for all 126 original cases,
      plus all 16 source/scratch offsets at both 1 KiB and 4 KiB in the paired
      supplement. Preserve every calibration, warmup, sample and failed record.
- [ ] Verify an accepted immutable evidence commit and configure its full SHA via
      `RSE_RELEASE_EVIDENCE_REF` or manual `evidence_ref`. The variable was absent
      at this audit; rejected snapshots cannot serve as accepted inputs.
- [ ] Run manual CI on final main with `release_preflight=true` and the accepted
      evidence SHA. Verify actual strict positive preflight success. Ordinary
      branch/PR CI and historical negative runs do not satisfy this step.
- [ ] Create the matching 9.1.0 tag only after all preceding gates pass, then verify
      actual registry publication. CHANGELOG's preparation date is not a release
      timestamp; a package dry-run is not an upload.

Use the [evidence procedure](release-evidence.md) and
[service-cost analysis](baseline-service-review-2026-10-20.md) for exact commands,
source identities and known limits. Native ARM and downstream/S3 performance
remain under [#2853](https://github.com/rustfs/backlog/issues/2853); Docker/compile
checks cannot replace them, and they do not waive the performance gates above.
