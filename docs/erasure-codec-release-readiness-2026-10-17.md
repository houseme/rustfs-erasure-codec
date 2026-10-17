# PR and release readiness — 2026-10-17

A draft PR can be opened for the reviewed codec changes. Creating or merging a
PR does not publish a package; this repository publishes the core crate only
from tag workflows. No version, tag, registry write or GitHub release is created
by this work. The [prior source and evidence report](erasure-codec-recovery-metadata-validation-2026-10-17.md)
remains authoritative for runtime source `0a2d5ee` and its frozen experiments.
This follow-up changes scripts, tests, documentation and CI only.

## Findings and implementation

The [findings were recorded before implementation](https://github.com/rustfs/backlog/issues/2708#issuecomment-6235517034).

- Tag preflight previously downgraded to optional `extended` checks when one
  baseline variable was absent. It now always uses `release`; all four readable,
  nonempty baseline files and complete-case mode are checked before Cargo.
  Required gate switches cannot disable release validation.
- Developer smoke previously forced the quick 4+2 case while later gates required
  10+4/1 MiB. Extended/release now run the focused complete smoke producer with
  release + SIMD flags. Golden tests run separately without `--ignored`; fast
  developer checks retain the quick profile.
- The independent hotspot gate returned zero for both an empty artifact and a
  required scenario with NaN throughput. It now compares all baseline scenarios,
  rejects malformed/dropped/duplicate rows and invalid numbers/limits, and reuses
  the general gate's numeric and available-provenance normalization. Legacy
  metadata absent on both sides is still unavailable evidence, not strict CPU or
  compiler comparability.
- Tags must match the core manifest version exactly, with an optional `v`;
  package, workspace dependency and lockfile versions must match. Cargo remains
  responsible for validating version syntax. Branch/PR CI runs gate regression
  tests and a real locked `cargo publish --dry-run --registry crates-io`.
  Publishing no longer uses `--allow-dirty` or unchecked `--no-verify` packaging.

These changes use one normalization policy and one release script. They do not
add an alternative codec, modify payload bytes, expand SIMD dispatch or change
Rust runtime sources or dependencies.

## Validation

- Python 3.12: 79 gate tests pass, including 7 hotspot tests, 5 manifest/tag tests
  and 6 release-script tests. The shell tests deliberately use command stubs to
  verify early failure, required switches, actual smoke profile selection and
  failure propagation; they do not claim successful performance acceptance.
- Real `cargo publish --dry-run --locked --registry crates-io --package
  rustfs-erasure-codec` packages 93 files and successfully builds the unpacked
  distributable, then explicitly aborts upload because of dry-run. The local
  registry mirror required an explicit registry; the offline attempt failed
  before packaging and is not counted as successful validation.
- Actual golden-vector and selftest commands execute 7 and 2 passing tests.
- Actual focused release smoke test passes and produces 15 unique rows, including all five cases required by release comparison. This is a content/profile-generation check, without baseline comparison or timing attribution. A real strict invocation without baseline variables exits 1 before Cargo.
- `actionlint`, shell syntax, Rust formatting, typos and diff checks pass.

Final branch/PR Actions results are recorded in the linked PR/backlog after
actual push. Earlier green SHA `0b4d075` is not substituted for the new head.

## Release decision and remaining work

The live official registry reports [core version 9.0.0](https://crates.io/crates/rustfs-erasure-codec)
already published and not yanked. The current manifest and existing tag are also
9.0.0. A new release therefore needs an explicit new version, synchronized
manifests/lock/docs/changelog, merged source and tag validation; no version bump
was inferred from a readiness question. Public npm lookup returned 404 for the
WASM package, and the current workflow does not publish npm.

The repository currently has zero Actions variables: none of the four release
baseline paths is configured. Real comparable baseline artifacts must also be
made available on the release runner; variable names alone do not provide files.
The strict release preflight has **not** passed and must not be represented as
passed based on unit tests, dry-run packaging or ordinary branch CI.

The complete 66-case campaign still has seven drift failures and all comparison
ratios remain null. Dedicated ARM throughput remains unverified; Docker validates
ARM64 Linux functionality. [#2853](https://github.com/rustfs/backlog/issues/2853)
and [#2708](https://github.com/rustfs/backlog/issues/2708) remain open. No same-condition
selected-case retry was run for this follow-up. These performance follow-ups do
not need to be closed merely to open a PR. A correctness/API release may keep
performance follow-ups open if its actual release checks pass and its release
notes make no unsupported throughput claim. This candidate is not ready for an
immediate formal publish: the new version and real strict release evidence are
still missing.
