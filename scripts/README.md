# Repository workflows

All capture, replay, validation and source-generation code is Rust in the
unpublished `xtask` package. Shell scripts only coordinate existing commands.
The workspace uses the environment's stable Rust toolchain. Root Cargo.toml owns
all crate dependency versions.

```bash
cargo xtask --help
cargo test --locked --package xtask
cargo xtask audit-no-python
cargo xtask audit-json
cargo xtask audit-package
cargo xtask audit-workspace
```

## Common workflows

| Workflow | Command |
| --- | --- |
| Manifest/tag consistency | `cargo xtask check-release --tag 9.1.0` |
| Benchmark regression | `cargo xtask check-benchmark --baseline OLD --current NEW` |
| Hotspot regression | `cargo xtask check-hotspot --baseline OLD --current NEW` |
| Complete release evidence | `cargo xtask release-evidence verify --manifest FILE --report REPORT` |
| Strict input materialization | `cargo xtask release-evidence materialize --manifest FILE --env-file FILE` |
| Formal collection | `cargo xtask release-evidence capture --help` |
| Complete diagnostic collection | `cargo xtask release-evidence diagnose-complete --help` |
| Shared sampler checks | `cargo xtask release-sampling --check --check-workers` |
| Stream views on both sources | `cargo xtask diagnose-fixture-layout --check-stream-views --baseline-ref 9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6` |
| Verify source ownership | `cargo xtask verify-scratch-audit --check --output NEW_DIRECTORY` |
| Stream source ownership | `cargo xtask stream-core-audit --check --output NEW_DIRECTORY` |
| Dynamic storage inventory | `cargo xtask dynamic-buffer-audit --check --output NEW_DIRECTORY` |
| CPU placement checks | `cargo xtask diagnose-cpu-placement --check` |
| Slot fixture checks | `cargo xtask diagnose-shard-slot --check` |
| Paired layout checks | `cargo xtask diagnose-verify-layout --check` |
| Ordinary sampler allocation check | `cargo xtask check-sampler-telemetry` |
| Native VM/counter diagnostics | `cargo xtask diagnose-release-vm --help` |
| Adjacent baseline analysis | `cargo xtask analyze-baseline-variability --help` |

All `--check` workers are correctness/structural checks. Fixed-count diagnostic
runs and instrumented source copies cannot approve release performance.
The [native tooling architecture](../xtask/README.md) documents resource ownership,
source stamps, original-byte readers and frozen Rust replay.

## Shell entry points

- `release-check.sh`: fast, extended or strict release validation.
- `verify-x86_64.sh`: portable verification and local smoke summaries.
- `check_backend_consistency.sh`: forced backend consistency.
- `run_galois_backend_abba.sh`: source-bound backend ABBA collection.
- `collect_x86_simd_benchmarks.sh`, `archive_x86_simd_machine.sh`: machine reports.
- `run-no-std-gf16-compat.sh {wasm|native} EXTRACTED_CANDIDATE_CRATE`: run the
  pinned 9.0.0 and candidate GF16 probe without enabling either codec's `std`
  feature. Both modes require Node.js for manifest generation. WASM also needs
  the wasm target and matching wasm-bindgen CLI; native can use
  `RSE_COMPAT_TARGET` and Cargo's target runner. Dependencies must be cached.
- `run_small_file_benchmark_matrix.sh`: complete small-file matrices.
- `serial-test-check.sh`, `run-full-benchmark.sh`: developer orchestration.
- `write-ci-status.sh`: native CI journal with a fixed negative fallback.

`jq` remains required by the two SIMD report-rendering shell scripts. Capture
requires the Linux facilities described in its corresponding guide. Live MSR
reads are read-only and require the device permissions supplied by the operator.
No controller changes THP policy or claims physical-host exclusivity.

## Strict release inputs

`VALIDATION_PROFILE=release` requires these readable, nonempty baseline files:

| Variable | Artifact |
| --- | --- |
| `RSE_SMOKE_BASELINE` | Full smoke matrix |
| `RSE_SMALL_FILE_BASELINE` | Small-file matrix |
| `RSE_RECONSTRUCTION_HOTSPOT_BASELINE` | Reconstruction hotspots |
| `RSE_STREAM_PATH_BASELINE` | Stream paths |

`RSE_RELEASE_EVIDENCE` selects a complete controlled manifest. Rust verifies it
before staging original A1/B2 quantitative results. Strict mode cannot omit cases
or disable SIMD/backend/regression gates. Golden-vector and functional tests remain
required even when quantitative results are reused. Failure stops subsequent
commands. The `fast` profile retains quick optional checks and selects only the
codec for no_std, avoiding workspace feature unification.

```bash
VALIDATION_PROFILE=fast bash scripts/release-check.sh
VALIDATION_PROFILE=release RSE_RELEASE_EVIDENCE=/path/to/manifest.json bash scripts/release-check.sh
cargo publish --dry-run --locked --registry crates-io --package rustfs-erasure-codec
```

The final command reconstructs the actual distributable without uploading.
Repository JSON archives retain their recorded paths and byte hashes. Current
commands and the release acceptance boundaries are documented in
[release evidence](../docs/release-evidence.md),
[caller fixtures](../docs/release-caller-fixtures.md),
[CPU placement](../docs/cpu-placement-diagnostic.md) and
[native service clocks](../docs/native-service-clock-diagnostic.md).
