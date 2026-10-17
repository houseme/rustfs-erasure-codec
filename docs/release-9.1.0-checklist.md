# 9.1.0 release preparation

Status: **prepared candidate, not tagged or uploaded**. Main review base is
`500444a` after the PR #8 rebase merge (initial review base `26c1051`);
[review and validation](erasure-codec-main-review-2026-10-18.md) records
the justified recovery and follow-up stream refactors and their limits. CHANGELOG's 2026-10-18 date is the prepared
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

Workspace 500 / no_std297 tests, focused 43 release regressions, all-feature Clippy,
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

The four baseline variables RSE_SMOKE_BASELINE, RSE_SMALL_FILE_BASELINE,
RSE_RECONSTRUCTION_HOTSPOT_BASELINE and RSE_STREAM_PATH_BASELINE must name real,
readable comparable files on the release runner. Missing inputs must fail; no
extended-profile downgrade or partial-case release override is allowed. Ordinary
branch/PR CI skips tag release-preflight/publish and is not evidence they passed.

The historical full 66-case campaign has seven drift failures, with null candidate
ratios. It has not been rerun under the same conditions. Dedicated ARM throughput
and S3 throughput are not established. A correctness/API release may retain
[#2853](https://github.com/rustfs/backlog/issues/2853), provided actual release
checks pass and notes make no unsupported speed claim.

Review/merge the release PR and verify the resulting main SHA first. Do not push
a tag or publish based only on this checklist or a dry-run. A final tag must match
all manifests/lock and the validated source. Actual strict release-preflight and
registry authorization remain separate from preparing this candidate.
