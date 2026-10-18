# 9.1.0 release preparation

Status: **prepared candidate, not tagged or uploaded**. Reference main before
this candidate is `bc3a3dc` after PR #16, with successful ordinary CI. The
[published raw protocol-2 audit snapshot](https://github.com/houseme/rustfs-erasure-codec/tree/f12cbb675e9379d58bfc7217c41f6415778e1c83)
remains rejected: 10/504 distributions are unstable; diagnostic replay found
11/252 repeat drifts above 5%. Its online negative preflight rejected the real
data and skipped release-preflight/publication. These checks do not establish
positive strict release acceptance. CHANGELOG's 2026-10-18 date is a prepared
release date, not evidence of registry publication.

## Version and API boundary

- Core package/workspace dependency/lock and both current installation READMEs
  specify 9.1.0. The WASM npm/Rust package keeps its separate 0.2.5 version.
- New encoding-format identity and external reader/writer recovery APIs are
  additive; effective default parity and field algorithms are unchanged.
- Error variants and WASM error codes remain stable. InvalidCustomMatrix's text
  now includes workspace incompatibility; applications should match the enum/code.
- Corrected fast-one-parity, custom-field and mixed-entry behavior must not be
  described as a guarantee to recover previously inconsistent codewords. Validate
  existing data against its actual algorithm/matrix; rebuild parity from healthy
  data if legacy mixed operations produced inconsistent parity.
- The workspace declares no MSRV. Edition 2024 and dependencies still impose
  compiler requirements; older compiler support is not inferred from this release.

## Executed candidate checks

The post-merge main [architecture review](erasure-codec-main-architecture-review-2026-10-18.md)
also validates direct matrix initialization on base `03e8049`.

Workspace 502 / no_std297 tests, focused 45 release regressions, all-feature Clippy,
wasm32 and 79 Python gate tests pass locally. The independent published-version
probe passes all 200 non-fast byte/roundtrip cases. Original-main release tests
reproduce the allocation defect; candidate budgets pass. Formatting, actionlint
and diff checks are required on the final commit. Final head/PR CI, generated
Node/Chromium tests and actual package dry-run are recorded in the release PR.

```bash
python3 scripts/check_release_manifest.py --tag 9.1.0
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
cargo run --locked --manifest-path scripts/release-compatibility/Cargo.toml --target-dir target
cargo publish --dry-run --locked --registry crates-io --package rustfs-erasure-codec
```

## Remaining publish gates

Collect a new controlled whole-release A1/B1/B2/A2 campaign against published
9.0.0, following [the evidence procedure](release-evidence.md). All four complete
profiles, source/context/digest checks, both-side drift and existing regression
thresholds must pass. Protocol 6 also requires retained buffered-verification Vec
layouts, lengths, capacities and OS base page metadata to match the final
calibration and every phase; this scoped guard does
not complete [#2897](https://github.com/rustfs/backlog/issues/2897) or establish
layout comparability for the other dynamic operations. The full counterbalanced
slice diagnostic remains separate from original release cases. Only an accepted data snapshot may be configured as the
immutable `RSE_RELEASE_EVIDENCE_REF`. Rejected snapshots may be published for
audit and explicit negative checks, without promoting them to accepted inputs.
Protocol 6 isolates every case in a fresh process and retains three fixed warmups
plus all nine samples. Allocate caller buffers before codec initialization; run
`diagnose_fixture_layout.py` to inspect all 11 known Vec cases across both sources
before full performance capture. The one-iteration inventory is structural only,
has no performance approval and does not complete other dynamic-path coverage. Calibration rehearses that identical schedule before
freezing a count; collect a new complete campaign for this protocol. Missing inputs must fail; no extended-profile
downgrade or partial-case release override is allowed. Run the manual strict
preflight before tagging; ordinary branch/PR CI is not evidence it passed.

The historical full 66-case campaign has seven drift failures, with null candidate
ratios. It has not been rerun under the same conditions. Dedicated ARM throughput
and S3 throughput are not established. A correctness/API release may retain
[#2853](https://github.com/rustfs/backlog/issues/2853), provided actual release
checks pass and notes make no unsupported speed claim.

Review/merge the release PR and verify the resulting main SHA first. Do not push
a tag or publish based only on this checklist or a dry-run. A final tag must match
all manifests/lock and the validated source. Actual strict release-preflight and
registry authorization remain separate from preparing this candidate.
