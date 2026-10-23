# Scripts

Build, benchmark, release validation, and report generation scripts for `rustfs-erasure-codec`.

Repository-wide tooling inventory, packaging policy and the Rust migration
boundaries are documented in [`xtask/README.md`](../xtask/README.md).

## Quick Reference

```bash
# Rust repository tools (no Python needed for these commands)
cargo xtask check-release
cargo xtask audit-json
cargo xtask audit-package
cargo xtask audit-workspace
cargo xtask check-sampler-telemetry
# Linux only: read-only THP/mTHP and process policy
cargo xtask memory-policy
# Pure sampling validation: {"operation":"catalog"} or a typed validation request
cargo xtask sampling-contract --input request.json
# Read-only local smoke report; actual DIT4 strategy is not exported by this schema
cargo xtask summarize-smoke --input target/benchmark-smoke
# Read-only stream I/O/window/residency validation, with optional summaries
cargo xtask validate-stream-views --input stream-views-request.json
cargo test --locked --package xtask

# Developer checks (fast profile; not release acceptance)
bash scripts/release-check.sh

# Extended checks (includes SIMD and configured baseline comparisons)
VALIDATION_PROFILE=extended bash scripts/release-check.sh

# Collect x86_64 SIMD benchmarks for all backends
bash scripts/collect_x86_simd_benchmarks.sh

# Verify Leopard GF8 on x86_64
bash scripts/verify-x86_64.sh
```

## Directory Structure

### Release & Validation

| Script | Language | Description |
|---|---|---|
| [`release-check.sh`](release-check.sh) | Bash | Main release validation entry point. Runs `fast`, `extended` or strict `release` profiles with regression gates (smoke, backend consistency, small-file, reconstruction hotspot). |
| [`check_backend_consistency.sh`](check_backend_consistency.sh) | Bash | Sweeps all SIMD backends on the host architecture and runs override metadata tests. |
| [`serial-test-check.sh`](serial-test-check.sh) | Bash | Runs the full test suite with `--test-threads=1` for serial execution debugging. |
| [`verify-x86_64.sh`](verify-x86_64.sh) | Bash | End-to-end x86_64 verification: architecture check, build, unit tests, benchmark smoke, strategy validation. |

### Benchmark Collection

| Script | Language | Description |
|---|---|---|
| [`collect_x86_simd_benchmarks.sh`](collect_x86_simd_benchmarks.sh) | Bash + Rust | Runs Criterion benchmarks and release smoke tests for all x86 SIMD backends, then generates machine reports and metadata through `xtask`. |
| [`run_x86_backend_smoke_matrix.sh`](run_x86_backend_smoke_matrix.sh) | Bash | Runs release smoke tests for all x86 backends and saves per-backend CSV files. |
| [`run_aarch64_backend_smoke_matrix.sh`](run_aarch64_backend_smoke_matrix.sh) | Bash | Same as above but for aarch64 backends (auto, scalar, rust-neon). |
| [`run_small_file_benchmark_matrix.sh`](run_small_file_benchmark_matrix.sh) | Bash | Runs small-file EC benchmark matrix with configurable profile and iterations. |
| [`archive_x86_simd_machine.sh`](archive_x86_simd_machine.sh) | Bash | Full x86_64 machine archival workflow: runs smoke matrix, renders summary and ledger entry Markdown files. |
| [`run_galois_backend_abba.sh`](run_galois_backend_abba.sh) | Bash + Rust | Freezes one Rust executable before switching revisions; extracts fresh Criterion records and rejects attribution for the entire campaign if either repeat drifts. |

### Regression gates (Rust)

| Script | Language | Description |
|---|---|---|
| `cargo xtask check-benchmark` | Rust | Compares baseline/current JSON or CSV, enforcing case completeness, provenance and per-operation regression limits. Used by `release-check.sh`. |
| `cargo xtask check-hotspot` | Rust | Checks every baseline reconstruction scenario, comparable provenance, regression limits and speedup floors. Used by `release-check.sh`. |

The fields, aliases and default thresholds are defined once in
[`xtask/contracts/benchmark.json`](../xtask/contracts/benchmark.json).
Capture/replay and native gates use the same Rust normalization model through
`native_tools.py`; the Python normalization implementation is removed.
The binding reuses one owned JSON-lines worker, bounds frames, serializes
concurrent callers and caches immutable pure results. Release replay invokes the native gates
through the controller's explicit tool manifest, so evidence directories need
not contain Cargo aliases or their own workspace.

Formal sampler generation, diagnostic overlays and their source observers now
use Rust through `native_tools.py`. This binding builds a
locked release-profile host tool once per Python controller and exchanges JSON
over stdin/stdout. The optimized executable and its dependencies are reused from
Cargo's cache; no compiler or process startup runs per pure-data request.
Rust tools do not launch Python. `cargo xtask check-sampler-telemetry` runs the
separate Rust allocation probe and removes only its unchanged owned input,
including after a failed or cancelled check.

`cargo xtask memory-policy` reads Linux THP/mTHP and inherited process policy.
`--input FILE --base-page-size BYTES` validates recorded JSON instead. Missing
controls, malformed choices and failed queries remain errors. Capture/replay uses
the same Rust module through the binding; only successful pure validation is
cached, and live policy is read each time.

### Report Rendering

`cargo xtask summarize-simd --machine-json FILE --machine-slug NAME --date DATE`
replaces the Python machine-report generator. Add `--root PATH` to read another
checkout; `--summary` summarizes archived machine report objects. Raw result
arrays and run metadata are excluded from that archive summary.

`cargo xtask perf-stack-audit` replaces the offline Python perf stack inventory.
Its `--input`, `--records`, `--record-log`, `--record-exit-code`, `--source`,
`--event` and `--output` flags are retained. `perf-record-stats --input FILE
--output FILE` converts native aggregate statistics to its record-count input.
Existing inventories cannot be overwritten. These tools do not approve release.

`cargo xtask stream-allocator-audit --trace FILE --samples FILE --output FILE`
replaces the Python syscall ledger. It reconstructs split writes and resumed
calls, binds all original timing/sample journals, and reports anonymous mapping
lifetimes without addresses. It also refuses to overwrite an existing ledger.

| Script | Language | Description |
|---|---|---|
| [`render_x86_simd_benchmark_summary.sh`](render_x86_simd_benchmark_summary.sh) | Bash + jq | Renders x86_64 SIMD benchmark summary Markdown from machine JSON. |
| [`render_x86_simd_ledger_entry.sh`](render_x86_simd_ledger_entry.sh) | Bash + jq | Renders x86_64 SIMD benchmark ledger entry Markdown from machine JSON. |

## Dependencies

### Shell scripts

- `bash` 4+
- Standard Unix tools: `awk`, `sed`, `grep`, `date`, `uname`
- `cargo` (Rust toolchain)

### Python scripts

- Python 3.8+
- Standard library only (`json`, `csv`, `argparse`, `pathlib`, `statistics`, `platform`, `subprocess`) — no third-party packages required

### Report rendering

- `jq` (JSON processor) — required by `render_x86_simd_benchmark_summary.sh` and `render_x86_simd_ledger_entry.sh`

## Environment Variables

| Variable | Used By | Description |
|---|---|---|
| `VALIDATION_PROFILE` | `release-check.sh` | `fast` (default), `extended`, or strict `release` |
| `RSE_SMOKE_BASELINE` | `release-check.sh` | Path to baseline smoke results for regression gate |
| `RSE_SMALL_FILE_BASELINE` | `release-check.sh` | Path to baseline small-file results |
| `RSE_SMALL_FILE_METRIC` | `release-check.sh` | Metric for small-file gate: `throughput_mb_s` or `ns_per_iter` (default) |
| `RSE_RECONSTRUCTION_HOTSPOT_BASELINE` | `release-check.sh` | Path to baseline reconstruction hotspot results |
| `RSE_STREAM_PATH_BASELINE` | `release-check.sh` | Path to baseline stream path results |
| `RSE_STREAM_PATH_METRIC` | `release-check.sh` | Metric for stream path gate: `ns_per_block` (default), `ns_per_iter`, or `throughput_mb_s` |
| `RUN_BACKEND_CONSISTENCY` | `release-check.sh` | Set to `1` to enable backend consistency sweep |
| `RUN_SIMD_ACCEL_TESTS` | `release-check.sh` | Set to `0` to skip SIMD tests |
| `RUN_SMALL_FILE_GATE` | `release-check.sh` | Set to `1` to enable small-file regression gate |
| `RUN_RECONSTRUCTION_HOTSPOT_GATE` | `release-check.sh` | Set to `1` to enable reconstruction hotspot gate |
| `RUN_STREAM_PATH_GATE` | `release-check.sh` | Set to `1` to enable stream path regression gate |
| `RSE_BACKEND_OVERRIDE` | Various | Force a specific SIMD backend at runtime |
| `RSE_STRICT_BACKEND_OVERRIDE` | Various | Set to `1` to fail if override is not honored |
| `SMOKE_PROFILE` | `serial-test-check.sh` | Smoke profile for serial tests (default: `quick`) |
| `RSE_SMALL_FILE_PROFILE` | `run_small_file_benchmark_matrix.sh` | Small-file benchmark profile (default: `fast`) |
| `RSE_STREAM_PROFILE` | `release-check.sh`, `benchmark_stream_paths` | Stream benchmark profile (`quick`, `fast`, or `extended`) |
| `RSE_STREAM_IO_MODE` | `release-check.sh`, `benchmark_stream_paths` | Stream I/O mode (`auto`, `serial`, or `parallel`) |

## Strict release inputs

Tag CI always selects `VALIDATION_PROFILE=release`; it never falls back to an
optional profile. All four baseline variables above must point to readable,
nonempty files on the runner. Missing inputs fail before any Cargo command.
Release mode forbids `RSE_BENCHMARK_ALLOW_MISSING_CASES` and forces the required
SIMD, backend and regression checks even if their switches were disabled.
`extended` and `release` produce the full extended smoke matrix in release mode;
`fast` retains the quick developer smoke. Golden vectors execute without the
benchmark-only `--ignored` filter.

```bash
cargo xtask check-release
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
cargo publish --dry-run --locked --registry crates-io --package rustfs-erasure-codec
```

The last command builds and verifies the distributable without uploading. It
can succeed for an already published version; it does not establish permission
or readiness to publish a new version. Tag preflight additionally requires the
exact core version with an optional `v` prefix, consistent package/workspace/
lock versions, and all real baseline files. See the [current readiness report](../docs/erasure-codec-release-readiness-2026-10-17.md)
for the uncompleted acceptance and version decisions.

## Synchronous VM service counters

[`diagnose_release_vm.py --native-service-clock`](../docs/native-service-clock-diagnostic.md)
adds read-only, worker-synchronous guest counters to an independent diagnostic
source copy. `--probe-only` checks capability with zero codec calls and never
starts calibration. Keep it separate from a currently running controlled
capture; missing capabilities are not new release gates. Ordinary sampling and
production code are unchanged.
