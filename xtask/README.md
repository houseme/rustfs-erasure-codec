# Repository tools and review

`xtask` is an unpublished workspace package. The codec has no dependency on it,
and it is excluded from the crate. Default workspace members are unchanged;
tool tests run explicitly in CI. Rust edition 2024 and the environment's stable
toolchain apply. No encoding kernels, matrices, field representation, format IDs,
or dispatch thresholds change in this cleanup.

```bash
cargo xtask check-release --tag 9.1.0
cargo xtask audit-json
cargo xtask audit-package
cargo xtask audit-workspace
cargo test --locked --package xtask
```

## Executable migration

| Removed entry point | Replacement | Retained contract |
| --- | --- | --- |
| `scripts/check_release_manifest.py` | `cargo xtask check-release` | Package/workspace/lock agreement; exact bare or `v` tag, including prereleases |
| `scripts/extract_galois_backend_criterion.py` | `cargo xtask extract-criterion` | Fresh estimates only; same CSV header and JSONL fields; append behavior |
| `scripts/analyze_galois_backend_abba.py` | `cargo xtask analyze-abba` | Complete unique ABBA cases, exact revision/override provenance, inclusive drift limit and whole-campaign attribution refusal |
| ABBA runner's inline Python metadata/clock | `abba-metadata` and `epoch` | Metadata fields and subsecond freshness cutoff |
| `scripts/summarize_x86_simd_benchmarks.py` | `cargo xtask summarize-simd` | Machine/archived report schema, backend override eligibility, stable ties and manual adoption rules |
| `scripts/perf_stack_audit.py` | `cargo xtask perf-stack-audit` / `perf-record-stats` | Address scrubbing, source anchors, real periods, sample conservation, bounded inventories and no overwrite |
| Collector's inline Python metadata | `machine-metadata` | Same fields; OS description is native `uname` output |
| `scripts/check_benchmark_regression.py` | `cargo xtask check-benchmark` | JSON/CSV normalization, aliases, strict provenance, complete case sets and metric-specific thresholds |
| `scripts/check_reconstruction_hotspot_gate.py` | `cargo xtask check-hotspot` | All baseline scenarios, case/provenance comparability, scenario limits and speedup floors |
| `scripts/stream_allocator_audit.py` | `cargo xtask stream-allocator-audit` | Resumed calls, split stdout, complete sampler journals, mapping splits/replacements and address-free ledgers |
| `scripts/stream_core_source.py`, `scripts/dynamic_leopard_source.py` | Shared `diagnostics::{source,stream,leopard}` modules | Reversible source observations for both published-baseline and current layouts |
| `scripts/check_sampler_telemetry.py` | `cargo xtask check-sampler-telemetry` | Separate allocation counter; owned temporary input; preserve changed/pre-existing files; stop and reap the worker on cancellation |
| `scripts/release_memory_policy.py` | `cargo xtask memory-policy` / native capture bindings | Read-only THP/mTHP policy, full process mode, explicit absent interfaces and strict replay validation |
| Intermediate `scripts/benchmark_contract.py` | Shared native `benchmark_contract` model | Row order, extra fields, normalization, aliases, strict provenance and hotspot dimensions |
| Python CPU identity parser/test module | Native `runtime::cpu_identity` | Named models, reserved ARM CPUs, canonical fields and rejection of incomplete identities |
| `verify-x86_64.sh` inline Python | `cargo xtask summarize-smoke` | One read pass, ordinary/legacy smoke rows, separate A/B/profile inputs and recorded backend metadata |
| Python sampling contracts/validators and their pure test module | Shared `sampling_contract` and `diagnostics::catalog`; `cargo xtask sampling-contract --input FILE` | All 126 cases, API/masks/seeds, RNG witnesses, CPU masks/counters, retained buffers, measured intervals and complete calibration rehearsals |

The obsolete implementations and their Python test entry points are removed.
Rust unit and executable tests cover the retained contracts. The migration was
also exercised against the original 34 Python behavior tests before deletion.
The runner freezes the compiled tool before switching Git revisions so old
baseline revisions do not need the new source tree or dependencies.

Extraction additionally rejects nonpositive or unordered confidence intervals
before appending either artifact. Invalid ABBA inputs still replace stale PASS
reports with failed JSON/Markdown and a nonzero exit. Schema field names remain
compatible; diagnostic text and numeric formatting are not a byte-level format.

## Root dependency policy and shared implementation

Every workspace member inherits dependencies from the root
`[workspace.dependencies]`, including the two renamed codec dependencies in the
compatibility probe. The root lockfile records both versions. The nested probe
lockfile has been removed. `audit-workspace` reads Cargo's member inventory and
rejects local dependency versions/paths/git sources, including target-specific
overrides; CI runs it with the package and JSON audits. Package versions and
binary source paths remain properties of their own package manifests.

The probe remains outside default workspace members and outside the codec's
normal dependency graph. Tools that resolve all graph packages can see two
packages named `rustfs-erasure-codec`; use the root manifest's exact package ID
instead of duplicating the candidate version in scripts:

```bash
cargo clippy -p "$(cargo pkgid --manifest-path Cargo.toml)" --all-features --all-targets -- -D warnings
```

`artifact` provides strict JSON parsing, CSV reading, sorted artifact discovery,
validated Criterion estimates and buffered JSON output. Extraction, summaries,
perf and audits share these implementations. The CLI command catalog also
generates command help, so adding a command does not leave a separate help list
out of date.

Machine summaries keep XOR kernels out of
`mul_slice` rankings and avoid counting their weight twice. Archived raw result
arrays are skipped. These are intentional reporting
corrections. Valid historical machine-report objects retain their schema and
values. Rankings and scores are computed once, and machine reports serialize
borrowed input rows directly instead of allocating a second complete JSON tree.
Perf input is read one line at a time, frames/stacks are interned, and samples
and periods use checked counters. Overflow and corrupt/lost input fail before
creating an output inventory; existing inventory files remain untouched.

Native gates and capture/replay share the same Rust normalization model and
`contracts/benchmark.json` field/alias/threshold definition. The intermediate
Python normalization implementation has been removed. Ten archived JSON
artifacts (672 rows) retain their complete normalized values. Per-field queries
retain independent errors, so a malformed unrelated field does not change the
requested field's result. The library preserves row order and extra fields.
Regression gates run through an explicit, locked, offline tool manifest so
evidence can be replayed from directories with no Cargo aliases or workspace.
The Bash release checks invoke the same native commands and preserve their
preflight/refusal ordering. The syscall ledger likewise has no Python runtime
dependency and retains the original no-overwrite and conservation contracts.

Formal sampler generation and all four shared native
observer templates use the same Rust source-generation subsystem. Exact anchor
counts and reversible edits reject changed layouts before an overlay is written.
All eleven generated sampler files, stream/verify/Leopard source observations,
and observer templates were compared byte-for-byte with the previous generators
for the current and published-baseline sources. A permanent native golden test
also checks every formal sampler file and keeps the allocation counter separate.

The remaining capture controllers call Rust through `native_tools.py`. It builds
the host executable once and reuses an owned JSON-lines worker, serializing calls
from multiple threads. Invalid requests return an error without losing the next
frame. EOF/shutdown reaps the child; a dead worker fails the current call without
automatic replay. Request/response limits are 16/32 MiB. The original one-shot
`native-helper --input FILE` interface remains available. There is no
Rust-to-Python launcher. Rust sources, manifests and the lockfile participate in
implementation digests. Controller
validation and process/evidence ownership remain in Python until those contracts
migrate together. The native telemetry command
runs the real generated Rust tests, supervises its owned child process group on
Unix, and checks the generated input before cleanup.

Only pure data operations use the persistent worker. File adapters use a fresh
child so relative paths follow the caller's current directory. Live kernel-policy
capture also uses a fresh child, inheriting current process flags; an older
worker would retain its earlier PR_GET_THP_DISABLE state. Observation agreement
is checked in batches of at most 64, sending only the contract's provenance/case
fields and placement. Original record references and error positions are retained.

The sampler renderer and offline verifier now share one native API/mask catalog.
Mask values and emitted Rust expressions come from the same model. Service-window
constants are compiled directly from the worker's Rust module, with the original
bounds enforced at compile time. The complete 126-case workload/fixture catalog
retains the original Python model's fixed digest. An additional 5,206 valid and
malformed requests retain the original acceptance decisions and returned values.
The standalone `sampling-contract` command reads a strict JSON request without
Python; the legacy controllers use the same typed requests through the worker.

Measurement and calibration validation each send one complete 12-record worker
batch, replacing repeated per-record IPC. Both reuse the same scheduling,
placement, fixture and timing checks. Calibration retains its full rehearsal,
target flags, worst-probe growth and iteration budgets. Expected workload
contracts are computed once per distinct case in a batch; bounded Python caches
return private copies of pure catalog/contract values. Capture orchestration and
evidence-member/revision checks remain active in their existing controllers.

Dynamic matrix/decode observers and the complete
stream-view, ordinary-verify, stream-core, dynamic-storage, ShardSlot and CPU
placement overlay generators are native. Controllers retain thin bindings. The
generated maps match the original Python bytes for both source revisions; the
CPU allocation-check variant additionally matches on the current source (the
published baseline has no allocation-check template). Native golden tests cover
all seven current overlay variants and all nine dynamic source files.

The final source review also reproduced a newline regression: the original Python
reader accepted CRLF/CR source files, while literal Rust reads rejected the same
layout. All generator file reads now share universal-newline handling. A native
CLI regression compares all eleven generated files for LF, CRLF and CR inputs;
artifact JSON bytes and the existing provenance hashing remain unchanged.

Linux THP/mTHP capture and replay validation also share one Rust module. Capture
only reads sysfs and PR_GET_THP_DISABLE; it never changes policy. Explicit null
interfaces, shmem inheritance and modes 0/1/3 are retained. The old and native
validators agreed on 861 input/base-page combinations. Python keeps a bounded
cache of successful pure validation, keyed by the complete serialized current
input; live captures are never cached. Local fixtures and Linux cross-compilation
verify the implementation here; native Linux sysfs/prctl execution is a CI check.

Successful normalization is cached as immutable serialized values with bounded
entries; callers receive fresh objects. Large batches bypass the cache. The
provenance cache includes every current alias but excludes unrelated timing/case
fields. Cache keys are derived again from current values, so alias mutations
cannot reuse an earlier result. Live system capture is never cached.

The root serde_json dependency enables float_roundtrip. The default parser was
observed to shift the least significant bit of some metric values; all 2,000
tested numeric pairs now survive the bridge unchanged. CPU identity parsing is
also native, preserving canonical ASCII JSON for ARM identities. These changes
affect repository tooling, not codec kernels or encoding formats.

## Complete Python inventory

The initial inventory contained **56 Python files / 13,120 lines**. Twenty-two
original files were removed (twelve implementations/helpers and ten test
modules). One native-tool compatibility binding remains. The intermediate Python
normalization library was also removed after its consumers moved to Rust.
The remaining **35 files / 9,065 lines** consist of 15 production/helper modules
and 20 test/fixture modules: about 31% fewer Python lines overall. The latest
migration removed the pure workload test module after native coverage passed;
its four worker/build/source integration tests remain in the sampler suite.
The unreferenced Python-2-style `sage/galois_ext_test.sage` worksheet was also
removed. Its four fixed tower-field addition/multiplication/division/inverse
vectors now execute against both codec versions in the compatibility probe.
All remaining modules have callers, CI checks, tests or documented evidence
replay uses; none is classified as obsolete merely because it is old.
Counts cover tracked and non-ignored worktree sources; generated target caches
and validation copies are outside this inventory.

| Remaining group | Modules | Rust migration boundary |
| --- | --- | --- |
| Native tool binding | `native_tools` | Own/reap one host worker for source generation, normalization, CPU identity and memory policy; bounded immutable-value caches |
| Campaign and machine qualification | `release_evidence`, `release_sampling`, `analyze_baseline_variability`, `diagnose_adjacent_baseline`, `diagnose_release_vm` | Port as one coordinated state machine with process ownership, platform adapters and immutable evidence contracts |
| Native probes and source overlays | `diagnostic_overlay`, `diagnose_fixture_layout`, `diagnose_verify_layout`, `diagnose_shard_slot`, `diagnose_cpu_placement` | Reuse existing Rust probe programs and templates; retain rollback/cancellation behavior and original-source digests |
| Allocation/lifecycle audits | `dynamic_buffer_audit`, `stream_core_audit`, `stream_view_audit`, `verify_scratch_audit` | Move inventory witnesses, capture/replay and resource-conservation checks together; source transforms and overlay generation are already native |
| Tests and fixture support | `scripts/tests/*.py` | Move each behavior test with its owning subsystem; remove the old test only after exercising its replacement |

This is an incremental migration, not a Python-free repository. The remaining
scripts currently validate release evidence and Linux runtime/resource behavior.
Keeping them executable preserves those checks while the typed Rust boundaries
are migrated. A blanket deletion or a Python launcher hidden behind `xtask`
would not satisfy those contracts. Future migrations belong in these modules of
the same tool rather than additional one-off executable layers.

The inventory also checks embedded Python. `verify-x86_64.sh` now uses one native
report command instead of three JSON-display fragments; CI retains resource-record validation, locked
wasm-bindgen version extraction, runtime/evidence commands, and failure outcome
journals. The journals run with `always()` even when Rust installation or checks
fail, so replacing them with a freshly compiled tool would lose failure evidence.
These active inline fragments are separate from the 35 `.py` files counted above
and remain outside both crates. No Python-2/Sage executable remains.

## JSON inventory and retention

`audit-json` checks tracked and non-ignored untracked JSON files in the current
worktree, including files outside the codec. Deleted files are skipped until staging. It rejects
duplicate object keys, non-finite/out-of-range numbers and trailing input, and
reports exact-byte duplicate groups without modifying files.

The initial audit found **575 files / 2,807,430 bytes**, all valid. These archives
and WASM inputs remain unchanged; one active tool contract brings the current
total to **576 files / 2,809,815 bytes**:

| Category | Files | Policy |
| --- | ---: | --- |
| Raw Criterion evidence | 448 | Preserve phase-local records and manifest paths |
| Versioned evidence | 35 | Preserve schema and digest-bound contents |
| Historical reports/evidence | 89 | Repository-only; do not reinterpret old runs as current acceptance |
| WASM configuration/lockfiles | 3 | Preserve build and dependency reproducibility |
| Shared tool contract | 1 | Field, alias and default-threshold definition; excluded from the codec crate |

The [evidence catalog](../benchmarks/README.md) indexes the archive directories
and their purposes without moving or rewriting digest-bound artifacts.

Eleven exact-byte duplicate groups exist. Identical metadata under separate ABBA
phases is expected, and resource reports can be copied as evidence in later
reviews. Deleting or replacing these with links would break self-contained
campaigns and potentially their digests. Reports are not runtime dependencies.
New generated artifacts belong under ignored `target/` unless deliberately
archived with provenance; use the audit to inspect rather than rewrite history.

## Crate boundary

The old exclusion list already kept scripts, docs, benchmarks, fuzz and Sage
out of crates.io. The explicit inclusion list now admits only `src/**/*.rs`,
`build.rs`, the manifests/lockfile, the license and the English README. Cargo
also generates its normalized manifest and VCS metadata. Integration tests,
release notes, translated documentation and all tooling remain in GitHub.
Source-level unit tests remain alongside their implementation. Both crate audits
also reject non-Rust files within `src/`, preventing future Python/JSON additions
there from silently entering a release.

The initial package list had **97 members**; the allowlist has **67**. CI audits
the actual Cargo package list and rejects unexpected repository files or missing
build inputs. `cargo package --allow-dirty --locked --offline --package
rustfs-erasure-codec` verifies the extracted distributable without publication.
The codec's dependency graph does not include `xtask` or its parsers.

The WASM Rust crate also has an independent whitelist. Its package list shrank
from **17 to 6 members**: Rust binding source, manifest/lockfile, README and Cargo
metadata. npm manifests, TypeScript, build scripts and Playwright tests remain
in the repository and are unaffected for npm builds. `audit-package` verifies
both publishable crates, including rejection of `src/index.ts` from the Rust
binding package.

The WASM package was built from its extracted contents for
`wasm32-unknown-unknown`, using an invocation-local registry patch to the checked
candidate core. The local offline registry index has no 9.1.0 core release, so
an unpatched registry package attempt cannot resolve that dependency. This does
not prove registry publication readiness; no patch or dependency version is
added to a member manifest, and no package is uploaded.

## Current branch review and compatibility

The cleanup starts from `022c0a7` and is grouped into functional commits for
dependency upgrades, crate contents, native tools, controller migration,
compatibility checks and documentation. The existing dependency upgrade is
preserved in its own commit. Codec sources and `build.rs` are unchanged from
that base.
The duplicate median/spread implementation has been
consolidated in `release_sampling.selected_row`: strict mode rejects short
samples and diagnostic mode records them, while both use the same raw-record
validation, warmup exclusion, median and central-spread computation. Diagnostic
schemas remain unable to approve a release. This reduces policy drift without
changing codec hot paths or relaxing acceptance.

The unpublished `scripts/release-compatibility` member pins published 9.0.0
and local 9.1.0. It compares complete parity bytes for 200 configurations and
now feeds each version's parity to the other decoder, including simultaneous
data/parity loss where the parity budget permits. It covers Classic GF8/tower
GF16, Leopard GF8/GF16, matrix options and aligned/partial lengths. Existing
format-ID, golden-vector and codec-contract tests remain in the repository.

Fast-one-parity's historically inconsistent incremental behavior is excluded
from that old/new probe as documented in its README; this cleanup makes no
universal compatibility claim for changing codec options or incompatible
formats. No throughput improvement is claimed from a tooling refactor. Real
performance attribution still requires qualified, isolated ABBA measurements;
diagnostic runs retain their existing inability to authorize release.

The review following functional commits found an unsupported DIT4 strategy
assertion in the x86 verification report. The script inferred `auto`/`direct`
from shard size even though `RSE_DIT4_STRATEGY` can override the choice and the
JSON exporter does not record it. Its filename glob also included A/B arrays
and instrumented profiles. `summarize-smoke` replaces those inline parsers with
one pass using the shared strict JSON reader. It distinguishes measurement
types, preserves legacy ordinary objects and reported values, and states that
actual strategy is unavailable. It neither changes encoder behavior nor mutates
artifacts. Invalid measured inputs fail before any partial report is printed.
Native CLI tests exercise mixed artifacts, explicit strategy overrides, legacy
fields, malformed metrics/JSON and empty input. The x86 wrapper's build/test
steps are unchanged; local ARM64 execution validates the portable report, not
a new x86 hardware benchmark.

## Local validation for this change

Validated on the native macOS ARM64 host with stable Rust 1.99.1 (seventh migration;
the complete published-baseline worker replay below was established in the fourth
migration, whose codec sources remain unchanged):

| Check | Result |
| --- | --- |
| Core `--all-features --lib --tests` | 517 passed; 43 existing benchmark/manual tests ignored |
| Core all-feature doctests | 10 passed |
| Rust tooling unit/executable tests | 82 passed, including generator bytes, LF/CRLF/CR, framing/bounds, exact float bits, CPU identity, policy, batched agreement, sampling contracts, smoke-report scope, cancellation and cleanup |
| Native row normalization | Ten archived JSON artifacts / 672 rows match the original Python results |
| Exact numeric transport | All 2,000 numeric pairs retain their original float values |
| Sampling contract migration | All 126 workload/fixture contracts retain the old model digest; 5,206 acceptance/value comparisons match |
| Controller worker lifecycle | Reuse after validation errors, concurrent calls, crash recovery and immutable caches are exercised by Python tests |
| Real diagnostic compilation/execution | ShardSlot: 2 native checks; CPU placement: 19 native checks passed on the host |
| Linux tooling compilation | All targets checked for x86_64-unknown-linux-gnu, including the native prctl ABI |
| Original/native memory-policy validator comparison | 861 acceptance/refusal decisions match; 11 original Python policy tests passed before removal |
| Native sampler telemetry allocation checks | 28 passed through the Rust command; generated source removed after completion |
| Real generated workers against both source versions | 16 native checks and all 23 smoke/hotspot workers passed per version; all eight hotspot input replay witnesses match |
| Original migrated Python behavior tests against Rust commands | 34 passed before removal |
| Original perf behavior tests exercised through the Rust CLI | 14 passed; 98 CLI comparisons |
| Original benchmark/hotspot gate tests against Rust commands | 39 passed |
| Real archived small-file rows | All 447 cases from six JSON archives produce identical gate reports on self-comparison |
| Original syscall ledger tests | 5 passed; 8 CLI comparisons preserve complete reports |
| Machine report comparison | Full compatible-operation report and seven archived machine summaries match Python; regression tests cover the corrected XOR grouping and raw arrays |
| Remaining Python suite | 274 passed after native sampling/calibration batching and test migration; original 278-test suite also passed before consolidation |
| Published 9.0.0/local 9.1.0 probe with locked dependencies | 200 exact parity comparisons, 200 cross-decoding cases per direction and four retained Sage arithmetic vectors on each version passed |
| Core and tooling Clippy, all targets, `-D warnings` | Passed |
| Core `--no-default-features` check | Passed |
| Formatting, shell syntax and whitespace | Passed |
| Strict JSON audit and package allowlist audit | Passed |
| Offline Cargo package with extracted-source build verification | 67 files; 192.0 KiB compressed; extracted all-feature and no-default-feature checks also pass |
| WASM Rust package | 6 files; extracted sources verified on wasm32 with the local candidate core patch, as described above |

These are local results, not a new CI run, native x86/VSX validation, isolated
performance attribution or approval to publish the crate.

The historical local perf-audit CLI comparison from the second migration used 100,000 input samples, two warmups and
nine retained invocations in each A1/B1/B2/A2 phase. All 44 output documents were
identical between the original Python implementation and the release Rust
executable. Baseline repeat drift was **65.46%**, exceeding the 5% limit; the
campaign therefore withholds performance attribution. It also had no exclusive
CPU partition. No passing subset or repeated campaign is substituted. Raw timing
records and input/source digests are retained locally under ignored
`target/tooling-review-round2/performance.json`; they describe utility parsing
and I/O rather than codec throughput or release acceptance.
