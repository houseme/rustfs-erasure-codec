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
| Python stream-view ledger validators and pure tests | `diagnostics::views`; `cargo xtask validate-stream-views --input FILE` | V3 I/O, EOF/cursor transitions, caller backing, windows, native counters, virtual-page snapshots and explicit unknowns |
| Python verify-scratch/stream-core ledger models and eight pure tests | `diagnostics::{ledger,verify_scratch,stream_core}`; `cargo xtask validate-diagnostic-ledger --input FILE` | V1 schemas/summaries, every observation, nested legacy verify calls, transferred/replaced owners and explicit native coverage |
| Python dynamic allocation models and six pure tests | `diagnostics::dynamic_ledger`; `cargo xtask validate-diagnostic-ledger --input FILE` | Live generations, original payload stages, old/current decoder owners, typed lane/byte extents, supplement headers and diagnostic-only workload markers |
| CPU placement, Slot histogram and verify-layout calculations and 17 pure tests | `diagnostics::layout`; `cargo xtask analyze-layout --input FILE` | Complete CPU plans, bounded histograms, forward/reverse cells, paired phase gates, exact integer medians/ratios and original report schemas |
| Profile grouping/selection, v15 protocol and baseline statistics | `sampling_contract::{batch,protocol}`, `statistics`, `diagnostics::baseline`; `cargo xtask analyze-baseline-data --input FILE` | Whole-profile validation, stable original observation indices, service ranks, complete repeat cohorts, adjacent schedules and conservative wall-window bounds |
| VM process/thread analysis, synchronous counters and frequency deltas | `diagnostics::vm::{process,clock}`; `cargo xtask analyze-vm-data --input FILE` | Independent validation copies, shared/retired thread sets, per-thread reset checks, explicit unavailable/idle readings, ordered MSRs, enclosing windows and unchanged diagnostic schemas |
| Retained-layout comparisons and stream-view inventory signatures | `diagnostics::layout::inventory` and the shared typed `diagnostics::views` ledger; `cargo xtask analyze-layout --input FILE` | All four phases, six comparison pairs, 103 retained cases, 15 stream cases, exact difference fields, lifecycle ordering and explicit absence of performance attribution |
| Complete calibration profiles and campaign comparisons/classification | `sampling_contract::calibration_profile`, `diagnostics::baseline::campaign`; existing `sampling-contract` and `analyze-baseline-data` commands | Every raw calibration row, complete bounded rehearsals, frozen counts, optional CPU policy, original case order, finite ratios, complete diagnostic failures and existing report fields |
| Baseline directory loading, preparation comparison, runtime validation and complete report assembly | `diagnostics::baseline::replay`, shared `artifact::snapshot`, `runtime`; `cargo xtask analyze-baseline-capture --capture DIR --tool FROZEN_SOURCE` | Legacy basic/v1/v2 qualification schemas, original encoded-byte hashes, optional historical policies, complete cases/traces, bounded original members and descriptive-only reports |
| Python fixture/workload/RNG projections | Shared `sampling_contract::preparation` in existing calibration, selection and diagnostic-worker frames | Last observed per-case calibration batches, exact seeds/placement, all trace positions and unchanged persisted evidence |
| Python original-worker and frequency-result calculations | `sampling_contract::replay`, shared `batch::select` and `diagnostics::vm::frequency`; existing sampling/VM commands | Original journal agreement, frozen preparation/provenance, exact interval types, counter enclosure and unchanged complete reports |
| Python campaign worker-identity/event validation | Shared preparation worker starts and `sampling_contract::journal`; existing sampling command | Every rehearsal, full phase/profile order, original JSONL numeric rules, monotonic timestamps and exact invocation counts |
| Python dynamic campaign plan/envelope and fixed capture settings | `diagnostics::dynamic_ledger::campaign` and `sampling_contract::protocol`; existing native ledger/catalog frames | Frozen 516-step order, complete runtime/source provenance, every execution boundary, unchanged environment values and diagnostic-only markers |
| Python original diagnostic journal comparison and repeated pair-contract requests | Shared `sampling_contract::output`, `artifact::{snapshot,python_json}` and existing diagnostic workers | Bound original bytes, complete prefix counts, actual raw sample/view types, legacy numeric equality, paired provenance and unchanged summaries |
| Python paired-layout directory, context and journal validation | Shared `diagnostics::layout::replay`, snapshot reader and paired grid model; existing `analyze-layout` command | Every ABBA member/hash, actual source tree/lock, strict runtime/CPU context, original JSON numeric rules, complete raw rows and no release approval |
| Python native/sampler/view source byte assembly and generated-overlay hashing | Shared `fingerprint` model and typed files/digest/plan render outputs | Original byte ordering, caller-file binding, fresh source reads, root normalization, identical generated files and unchanged digest fields |
| Python complete sampling-directory coordination and member adapters | Shared `sampling_contract::campaign`, calibration, batch, preparation and journal models; existing `sampling-contract` command | All 24 original calibration/phase members, optional worker journal, declared hashes, exact selected medians, conditional traces, controlled provenance and existing refusal details |
| Python original summary normalization, path helper and statistical transport | Same campaign snapshot and shared borrowed `baseline::analyze_campaign` model | All 16 summaries, first-seen case order, original hashes, fixed medians, distribution-refusal precedence and unchanged complete report fields |
| Python original harness byte assembly and block hashing | Shared `fingerprint::harness` and existing source reader/block selector | Five raw file streams, eleven first-selected declarations, original separators, universal unit-source newlines and fresh source reads |

The obsolete implementations and their Python test entry points are removed.
Rust unit and executable tests cover the retained contracts. The migration was
also exercised against the original 34 Python behavior tests before deletion.
The runner freezes the compiled tool before switching Git revisions so old
baseline revisions do not need the new source tree or dependencies.

Extraction additionally rejects nonpositive or unordered confidence intervals
before appending either artifact. Invalid ABBA inputs still replace stale PASS
reports with failed JSON/Markdown and a nonzero exit. Schema field names remain
compatible; diagnostic text and numeric formatting are not a byte-level format.

## Bound original-file reads

Release and diagnostic replay share one bounded original-byte reader. It checks
regular members, reads at most 32 MiB plus one byte, enforces the limit on that
actual read, and validates a declared SHA before decoding those same bytes.
The Python boundary preserves the previous default text encoding, universal
newlines and JSON values. Root report hashes come from their parsed snapshots;
JSON and numeric journals no longer use a later read to establish provenance.
Profile normalization reuses the parsed bound rows instead of reopening them.

Both regression gates accept optional `--baseline-sha256` and
`--current-sha256` declarations for JSON inputs. With these flags the existing
Rust snapshot reader binds the exact decoded bytes and applies its 32 MiB limit.
Ordinary unbound JSON/CSV commands retain their behavior. The release controller
passes its declared hashes into every gate, and paired replay rechecks the bound
supplement after profile checks. Diagnostic mode retains complete statistical
refusals (exit 1); invalid-input or execution errors remain fatal.

Six per-file source-planning operations and their Python wrappers are retired:
`matrix-source`, `decode-source`, `stream-source`, `verify-source`,
`exclude-setup` and `reserve-outputs`. Production generation uses complete Rust
plans. Seven pure Python source checks and the source assertions within two
binding tests move into four native tests with the same assertions. Python keeps
the active sampler/observer bindings, resource owners and failure restoration.

## Shared preparation model

Calibration profiles and selected profiles now return fixture layouts, workload
catalogs and input traces from their existing validation frame. Diagnostic workers
return their unchanged summary and the same preparation model after normalizing
observer rows. Directory replay borrows that model directly. Three Python
projectors are removed; no standalone preparation request is added.
Two borrowed passes replace the per-case whole-record scans. Null trace positions,
last observed calibration batches and exact seeds remain covered by native tests;
a frozen digest covers all 126 workload catalogs without duplicate JSON fixtures.
The private bridge responses change together with their source-bound consumers;
persisted report schemas and the encoding/decoding contract remain unchanged.

## Original-worker replay and frequency reports

Adjacent and frequency workers now share one borrowed Rust replay model. It
parses sample/timing journals once, checks their original values, reuses frozen
preparation/provenance and the same median selector as complete profiles.
Frequency replay combines that model with existing counter and service summaries
in one frame per worker. An eight-worker old/native comparison preserves every
report value while reducing requests from 32 to eight and request bytes from
810,888 to 675,681. All 126 adjacent workloads also agree exactly.

The file controller binds frequency plan/journal hashes to the bounded bytes
used for parsing. Integer/float intervals, interrupted final fragments, duplicate
journal keys and original rejection rules retain their behavior. Original file
and live MSR ownership remain in Python. These private tooling changes preserve
persisted schemas and do not alter codec formats or provide throughput evidence.

## Campaign journals from validated profiles

Calibration and selection responses include compact worker starts collected from
already validated observations. The full diagnostic reuses them in one native
journal request instead of reopening every sample member to infer identities.
The shared journal model streams events in the original calibration/ABBA order;
JSONL behavior, numeric identity equality, integer times and exit codes remain
covered. The check completes before campaign regression programs.
A complete old/native report comparison retains 756 workers and all 9,072
observations while reducing sample content reads from 48 to 24 and bytes from
45,162,120 to 22,581,060. Original artifacts and published formats are unchanged.

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
the optimized host executable with `--release --locked --offline` once and reuses
an owned JSON-lines worker, serializing calls
from multiple threads. Invalid requests return an error without losing the next
frame. EOF/shutdown reaps the child; a dead worker fails the current call without
automatic replay. Requests allow a 32 MiB artifact plus 64 KiB of envelope
space; responses are limited to 32 MiB. Compact JSON avoids redundant whitespace.
The original one-shot
`native-helper --input FILE` interface remains available. There is no
Rust-to-Python launcher. Rust sources, manifests and the lockfile participate in
implementation digests. Controller
validation and process/evidence ownership remain in Python until those contracts
migrate together. The native telemetry command
runs the real generated Rust tests, supervises its owned child process group on
Unix, and checks the generated input before cleanup.

The controller's repeated data-validation path uses the optimized tool. The
interactive `cargo xtask` alias keeps Cargo's ordinary developer profile. A
[retained local synthetic comparison](../benchmarks/tooling/2026-10-23/native-helper-profiles.json)
binds both profiles to identical source and replies; it measures helper service,
not codec performance or release acceptance.

Observer source plans now generate each complete verify, stream-core or dynamic
patch map and its original/diagnostic byte bindings in one Rust operation. Replay
can request bindings alone; the dynamic source-copy controller can request files
alone. The Python controllers share one installation/restoration owner, retaining
cancellation, existing-input refusal and preservation of unexpected edits. A
primed-tool comparison reduces the three installations from 4/6/2 native calls
to one each, with identical generated bytes and bindings for both codec sources.
These request counts do not establish codec throughput.

Host helper builds explicitly use their own checkout's target directory, matching
the sampler source-copy isolation policy even when the caller shares
`CARGO_TARGET_DIR`. A real older-checkout rebuild exposed replacement of the
memoized helper with an incompatible protocol; the source-local build fixes it.
A dedicated cross-check rebuilds that older tool under the shared caller cache
and confirms the new source plan still matches its original bytes and bindings.

Only pure data operations use the persistent worker. File adapters use a fresh
child so relative paths follow the caller's current directory. Live kernel-policy
capture also uses a fresh child, inheriting current process flags; an older
worker would retain its earlier PR_GET_THP_DISABLE state. Observation agreement
is checked in batches of at most 64, sending only the contract's provenance/case
fields and placement. Original record references and error positions are retained.

Stream-view events, lifecycle windows and residency snapshots now share one typed
Rust model. Each ledger is partitioned by call once; later checks borrow those
slices and group reader/writer events by shard. Nullable artifact fields remain
required, unavailable counters stay null, and former-range snapshots never imply
physical ownership. Per-call state is bounded before allocation. Geometry uses
checked or wider integer arithmetic, preserving the original values without
guessing absolute addresses or CPU attribution.

The stream controllers submit at most four bounded native observations per
frame. Inventory validation can return window summaries from the same parse;
the four-phase report reuses them instead of issuing a second request for every
observation. Other callers omit summary construction. Adapter normalization
keeps original inputs intact and uses the shared native fixture contracts.
The stream implementation digest now includes the Rust validator sources.
The old/native validators and complete summaries agree on 1,305 inputs; a
permanent digest covers all 15 shapes, two iteration counts and two page sizes.
This improves the tooling's traversal and IPC structure, not codec kernels or
performance acceptance rules.

Structural inventory comparisons also use native models. Complete retained
inventories validate all 4,944 observations through the existing profile/worker
and provenance checks in one frame. The stream controller submits one preparation
frame per phase, retains batches of at most four original view ledgers, and sends
only compact validated shapes to the final comparison. The controller reuses
validated lifecycle summaries locally, avoiding another JSON roundtrip of those
unchanged values.
Each bounded batch unions shapes per case while retaining all original
observation indices. Private wire tuples avoid repeated field names; the original
V3 ledger schema stays intact. Mixed-case batches preserve distinct shapes and
lifecycle order, and the final comparison still requires all twelve observations
for every case in every phase.
It no longer normalizes complete stream rows back into Python or traverses raw
events to construct another signature model. Sorted case/pair/difference order,
original inputs, report schemas and explicit diagnostic coverage remain intact.
Both inventory modes share the existing workload catalog and complete worker
validation; no new codec implementation or dependency is introduced.

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

The initial inventory contained **56 Python files / 13,120 lines**. Twenty-three
original files were removed (twelve implementations/helpers and eleven test
modules). One native-tool compatibility binding remains. The intermediate Python
normalization library was also removed after its consumers moved to Rust.
The remaining **34 files / 7,488 lines** consist of 15 production/helper modules
and 19 test/fixture modules: about 43% fewer Python lines overall. The current
production/helper and test/fixture split is 3,276/4,212 lines. This count
includes the latest merged VM observer/cleanup checks, native dynamic envelopes
and original-journal/paired-directory integration. Layout consolidation removes
349 lines from its immediate parent (`75bc034`, 8,486 lines). The
stream-view migration removed its Python test module after all eleven original tests
passed against Rust. Its six pure checks moved to native tests; five controller
integration tests remain in the layout suite and the shared fixture remains in
`layout_fixture.py`. Native worker batching has a separate binding regression.
The subsequent source-ledger migration removes another 225 Python lines. Its
eight pure verify-scratch/stream-core tests passed against the Rust replacement
before moving to native tests; capture, overlay and replay tests remain active.
The dynamic allocation migration removes another 328 Python lines. Its six pure
checks move with their Rust model; controller, source-copy and replay integrations
remain. Decoder metadata and workload unwrapping now share the native catalog.
Whole source-worker validation and release-row contracts now share the Rust
model. Per-worker checks use one frame, borrowing rows/views and preserving the
bounded immutable cache used by evidence replay. This step removes another 73
Python lines; the concurrently integrated build-policy test adds 14 lines.
Layout analysis now consumes each complete request in Rust. Slot histograms reuse
the formal position validator directly, without synthetic Vec fixtures or one IPC
call per bucket. Verify walks the fixed grid without an expected-row collection
and borrows its fixed provenance fields. A primed-cache synthetic comparison
retains identical complete outputs and inputs while reducing CPU analysis from
48 frames to one and Slot analysis from 24 frames to one. These counts describe
those fixtures, not codec throughput. All 38 original subsystem tests pass before
17 pure cases move to 13 native tests. An additional native regression and Python
raw-evidence replay test protect the unqualified diagnostic branch; 22 controller
tests remain in Python. The paired protocol and thresholds now come from the
same Rust implementation used for both qualified and diagnostic comparisons.
Whole-profile sampling now reuses the native row validator without Python
normalization callbacks. The helper returns stable input indices; bindings retain
the original selected rows and grouped objects. Nine-point distributions share
fixed-array statistics with layout analysis. Baseline service ranks, repeat
categories, adjacent schedules and wall-window bounds use the same Rust model.
The unchanged v15 protocol has its original canonical digest, and Python reads
native spread/repeat constants. This removes another 149 Python lines relative
to `4f74e0c`. Two pure checks move to Rust after all 84 original subsystem tests
pass; an additional Python integration checks one frame and original object
identity. A primed-cache complete-catalog comparison drops 126 selection frames
to four (one per profile) while retaining complete values and original inputs.
VM process/counter statistics and formal service-time summaries now share native
models. Sampling prepares independent validation copies and reports in one frame;
calibration copies use bounded batches of 64 records. Hardware reads and process,
source and resource ownership stay in the existing controller. All 45 original
VM/frequency/qualification tests passed before eleven pure checks moved to Rust.
The obsolete point/window wrappers and unused Python clock constants are removed.
A binding regression checks one request, bounded copies and raw-input independence.
Including the separately committed reset fix, this removes 175 Python lines from
`e7d1142`. Retained/stream structural inventories subsequently remove another 24
lines, including added binding checks. All fifty old/native comparisons retain
the original complete reports and refusal decisions.
Complete calibration and campaign analysis subsequently remove another 44 lines
relative to `c9ad921`, including a separately committed finite-ratio fix and new
binding coverage. All 44 original sampling tests passed before two pure
rehearsal checks moved to Rust; 42 sampling integration tests remain. Complete
calibration validation borrows the original rows and checks the CPU reservation
in the same pass. Baseline replay, adjacent controls and release verification
remove their redundant per-record CPU-validation calls. Native campaign analysis
keeps all 252 repeats, 504 comparisons and simultaneous diagnostic failure
classes; the controller still owns source/file checks and regression programs.
Finite input metrics can overflow their quotient, so nonfinite repeat and
candidate ratios are refused before a JSON report is constructed.
Native baseline directory replay subsequently removes another 52 lines relative
to `9210509`, including new CLI/cache/encoding checks and three service cases
moved to native full-file tests. At that snapshot the count is 3,553 production/helper and
4,122 test/fixture lines, including the reporter checks retained from main and
the shared source-plan binding regression. The baseline Python entry is now a native call; its
remaining code owns explicit trusted historical replay and the original source,
snapshot and resource checks for that adapter.

`analyze-baseline-capture` executes without Python or Cargo on `PATH` after the
tool is built. It reads 32 MiB-bounded regular members with a 256 MiB total byte
budget, hashes the original encoded bytes used for decoding, and checks member hashes and
the optional phase set again before returning. The shared reader retains legacy
UTF-8/16/32 JSON encoding detection. Duplicate object keys, nonfinite numbers and
linked members are refused. Both the standalone command and old Python binding
retain complete report fields and the existing source-fingerprint contract; the
tool reads the matching source tree to calculate those fingerprints. Captured
source bytes are never rewritten. The historical frozen-controller adapter
continues to require an explicit trusted source and isolated Python invocation.

Runtime validation shares the native libc/allocator/page/kernel-policy checks.
Its binding and memory-policy validation share a successful-result cache keyed by
each serialized current input. At most 128 payloads smaller than 64 KiB are
retained, keeping cached serialized text below 8 MiB. Larger inputs bypass the
cache. Live policy capture continues to use a fresh uncached process.
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
| Campaign and machine qualification | `release_evidence`, `release_sampling`, `analyze_baseline_variability`, `diagnose_adjacent_baseline`, `diagnose_release_vm` | Protocol, complete calibration, grouping/selection, campaign/baseline/VM distributions and counter windows are native; retain process/resource ownership, hardware reads, platform adapters, trusted frozen replay and immutable evidence contracts |
| Native probes and source overlays | `diagnostic_overlay`, `diagnose_fixture_layout`, `diagnose_verify_layout`, `diagnose_shard_slot`, `diagnose_cpu_placement` | Layout calculations, shared buffer validation, probe programs and templates are native; retain rollback/cancellation behavior and original-source digests |
| Allocation/lifecycle audits | `dynamic_buffer_audit`, `stream_core_audit`, `stream_view_audit`, `verify_scratch_audit` | All four allocation/source/view ledger models, dynamic plan/envelope, bound numeric journals, decoder catalog, source transforms and overlays are native; retain original-file/Git binding, capture/process ownership and integration orchestration |
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
These active inline fragments are separate from the 34 `.py` files counted above
and remain outside both crates. No Python-2/Sage executable remains.

## JSON inventory and retention

`audit-json` checks tracked and non-ignored untracked JSON files in the current
worktree, including files outside the codec. Deleted files are skipped until staging. It rejects
duplicate object keys, non-finite/out-of-range numbers and trailing input, and
reports exact-byte duplicate groups without modifying files.

The initial audit found **575 files / 2,807,430 bytes**, all valid. These archives
and WASM inputs remain unchanged; an active tool contract and the deliberately
retained helper-profile/complete-worker comparisons, the merged service-drift
review, complete Linux diagnostic review, full dispatch/GFNI mechanism controls
and subsequent source-bound matrix/GF16 reviews bring the current total to
**590 files / 3,464,634 bytes**:

| Category | Files | Policy |
| --- | ---: | --- |
| Raw Criterion evidence | 448 | Preserve phase-local records and manifest paths |
| Versioned evidence | 44 | Preserve schema and digest-bound contents |
| Historical reports/evidence | 94 | Repository-only; do not reinterpret old runs as current acceptance |
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

The latest owned-row/harness implementation is frozen at `adbac76` on main
`5ca2c20`, following the merge of PR #80 and the GF16 norm optimization. Codec,
build inputs, dependencies and lockfiles match that main. All 15 final checks
pass: 255 native, 252 Python and 538 top-level codec tests, ten doctests,
strict tooling Clippy, Linux tool compilation, no_std, published/local
compatibility, all audits and actual package reconstruction. The extracted
package passes 372 library tests. See the [owned-row/harness review](../docs/global-review-9.1.0-2026-10-23.md#owned-release-rows-and-native-harness-fingerprints).

The preceding summary/statistics integration is frozen at `882efff`. Its
[summary/statistics review](../docs/global-review-9.1.0-2026-10-23.md#original-summaries-and-borrowed-campaign-statistics)
retains the validation and transport scope from before its merge.

The preceding sampling integration is frozen at `b3f7341`. Its source-bound
[complete sampling review](../docs/global-review-9.1.0-2026-10-23.md#native-complete-sampling-campaign-replay)
retains its original raw-member, journal and transport scope.

The preceding fingerprint implementation is frozen at `a4976bd`. Its source-bound
[fingerprint review](../docs/global-review-9.1.0-2026-10-23.md#native-source-and-overlay-fingerprints)
retains the original generated-file, digest and transport scope.

The preceding paired replay implementation is frozen at `c2bde58`.
Its source-bound [paired directory review](../docs/global-review-9.1.0-2026-10-23.md#native-paired-layout-directory-replay)
retains its original validation and transport scope.

The preceding journal implementation is frozen at `12144ea` on main `a773e74`.
Its source-bound [original journal review](../docs/global-review-9.1.0-2026-10-23.md#original-diagnostic-journals-in-native-worker-frames)
retains its original validation and transport scope.

The preceding envelope integration is frozen at `af1598f` on main `6a15c5b`.
Its source-bound [dynamic envelope review](../docs/global-review-9.1.0-2026-10-23.md#native-dynamic-plans-and-capture-envelope-contracts)
retains its original validation and transport scope.

The preceding journal integration is frozen at `cea646b` on main `8d59e77`.
Its source-bound [journal review](../docs/global-review-9.1.0-2026-10-23.md#native-campaign-journals-and-single-read-profile-evidence)
retains its original validation and logical-read scope.

The preceding worker replay is represented by `d1d3402` on main `a771c04`;
its source-bound [worker replay review](../docs/global-review-9.1.0-2026-10-23.md#shared-original-worker-and-frequency-replay)
retains its original validation and transport scope.

The preceding preparation implementation is frozen at `b5ab95e` on main
`0a85348`; its subsequent digest test at `ccedab1` passes 215 native tests.
The source-bound [preparation review](../docs/global-review-9.1.0-2026-10-23.md#shared-preparation-projections-in-native-batches)
retains its complete validation and equivalence scope.

The preceding bound I/O review is frozen at `71d951e` on main `549b0ac`; its
original 206 native, 246 Python and 519 codec test totals remain source-bound.
It binds original values and regression inputs to declared bytes and moves
obsolete per-file source checks beside the native generators. See the
[bound I/O review](../docs/global-review-9.1.0-2026-10-23.md#bound-original-file-io-and-source-interface-review).

The earlier integrated review is bound to `709c40b` after main `e3aca32`.
Its 194 native and 247 Python tests, strict tooling Clippy, Linux metadata
compilation and 167 old/native comparisons pass within their stated scope.
The codec/build inputs match main. The source-bound historical reviews below
retain their own totals; see the [current complete review](../docs/global-review-9.1.0-2026-10-23.md#current-main-integration-and-complete-native-diagnostic-boundary).


The cleanup starts from `022c0a7` and is grouped into functional commits for
dependency upgrades, crate contents, native tools, controller migration,
compatibility checks and documentation. The existing dependency upgrade is
preserved in its own commit. Codec sources and `build.rs` are unchanged from
that base.
The duplicate median/spread implementation has been
consolidated in `release_sampling.select_profile`: strict mode rejects short
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

Validated on the native macOS ARM64 host with stable Rust 1.99.1. The latest code
snapshot is `0e44c58`; earlier capture campaigns below retain their individual
source bindings. Production codec sources remain unchanged:

| Check | Result |
| --- | --- |
| Core `--all-features --lib --tests` | 517 passed; 43 existing benchmark/manual tests ignored |
| Core all-feature doctests | 10 passed |
| Rust tooling unit/executable tests | 168 passed on optimized `0e44c58` (140 unit, 28 executable); earlier snapshots retain their separate bindings |
| VM/counter model | 763 original/native comparisons agree without changing inputs; 45 original subsystem tests passed before consolidation; the independent `d7c4c77` reset fix passes all 33 VM/frequency tests |
| Per-thread reset review | A shared thread's backwards run/wait/timeslice counter is refused before aggregation; another thread's progress cannot conceal it |
| Profile selection and baseline replay | 2,316 original/native checks agree, including all 126 cases, protocol identity, interleaved cases, malformed raw rows, single/paired phases and frozen adjacent service reports |
| Floating-point timing review | A finite metric/iteration product overflow previously passed tolerance comparison; the original Python and corrected Rust refuse it. Nonfinite quotients also fail before becoming null JSON fields |
| Complete layout migration | All 38 original subsystem tests passed before consolidation; 2,115 original/native data and protocol comparisons plus six raw-evidence diagnostic replays agree without input mutation |
| Native row normalization | Ten archived JSON artifacts / 672 rows match the original Python results |
| Exact numeric transport | All 2,000 numeric pairs retain their original float values |
| Sampling contract migration | All 126 workload/fixture contracts retain the old model digest; 5,206 acceptance/value comparisons match |
| Stream-view ledger migration | 1,305 acceptance/summary comparisons match; all eleven original Python tests passed before consolidation |
| Verify-scratch and stream-core migration | 3,941 comparisons: 3,937 identical decisions/summaries and four intentional refusals of negative/out-of-u64 timestamps; all 22 original subsystem tests passed before eight pure tests moved to Rust |
| Real stream-view workers, both versions | Four adapter tests and all 15 workers passed per version; 360 observations validated across published 9.0.0 and the candidate |
| Real verify-scratch/stream-core workers, both versions | 58 workers, 696 observations and 98,448 events pass capture/replay and match the original Python ledger summaries; 30 stream workers also exercise native view validation |
| Dynamic allocation migration | 1,583 original/native ledger checks plus 72 supplement-header checks agree; the large 50,625-event request preserves its summary and worker framing |
| Full dynamic capture/replay against both sources | All 516 workers pass, including 6,192 observations and 1,491,044 events; every complete summary and all twelve actual supplement headers also match the original Python model |
| Independent complete-worker comparison | 105 old/new complete-output/refusal checks agree without input mutation; the retained synthetic stream fixture uses 1 frame instead of 55 across all 48 warmup/sample blocks |
| Whole diagnostic workers and release rows | 7,503 original/native checks agree: 562 complete workers, 6,744 raw row batches and 197 mutated inputs; one measured worker uses 1 framed request instead of 42 |
| Final merged capture/replay | Clean `9b75e02` completes all 574 workers / 6,888 observations; `39f6ee7` retains identical Rust/codec sources and passes the optimized-helper agreement checks |
| Controller worker lifecycle | Reuse after validation errors, concurrent calls, crash recovery and immutable caches are exercised by Python tests |
| Real diagnostic compilation/execution | ShardSlot: 2 native checks; CPU placement: 19 native checks; verify-layout: 2 native checks passed on the host; the Linux-only timing worker remains ignored here |
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
| Remaining Python suite | 247 passed on frozen `0e44c58`, following the reset regression, eleven pure-test migrations and the independent-copy/batch integration |
| Published 9.0.0/local 9.1.0 probe with locked dependencies | 200 exact parity comparisons, 200 cross-decoding cases per direction and four retained Sage arithmetic vectors on each version passed |
| Core and tooling Clippy, all targets, `-D warnings` | Passed |
| Core `--no-default-features` check | Passed |
| Formatting, shell syntax and whitespace | Passed |
| Strict JSON audit and package allowlist audit | Passed |
| Offline Cargo package with extracted-source build verification | 67 files; 191.9 KiB compressed; extracted all-feature and no-default-feature checks also pass |
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
