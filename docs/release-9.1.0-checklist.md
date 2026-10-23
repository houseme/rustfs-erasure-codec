# 9.1.0 release preparation

Status at the 2026-10-23 review: **implementation merged, performance acceptance
incomplete, not tagged or uploaded**. The
[current merged-candidate review](release-9.1.0-readiness-2026-10-23.md) records
combined main `9f935ee`, its new-source validation and its differences from frozen
candidate `449920e`. The earlier `14cf56e` file-tree equality is preserved as a
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
