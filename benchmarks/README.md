# Repository evidence catalog

These archives stay in GitHub and are excluded from the published codec crate.
The WASM Rust crate also excludes repository JSON, TypeScript and browser tests;
its separate npm build still uses those files in the repository.
New runs write under ignored `target/`; archive them only with source, workload,
environment and raw-observation provenance. A saved JSON file or a successful
syntax audit does not by itself qualify a performance claim.

The current JSON inventory contains **570 benchmark files**:

| Directory | JSON files | Contents |
| --- | ---: | --- |
| [aarch64-darwin](aarch64-darwin/) | 2 | Historical ARM64 smoke results |
| [decode-idx](decode-idx/) | 1 | Decode workspace evaluation |
| [hotpath-review](hotpath-review/) | 9 | Raw observations, resource counts and descriptive stacks |
| [incremental-update](incremental-update/) | 16 | Incremental-update functional/resource review evidence |
| [recovery-paths](recovery-paths/) | 521 | Recovery comparisons, complete phase-local Criterion records and qualification/refusal summaries |
| [release-compatibility](release-compatibility/) | 3 | Native functional/resource comparison evidence |
| [small-file](small-file/) | 6 | Historical small-file case matrices |
| [stream-path](stream-path/) | 0 | Historical stream evidence retained in other formats |
| [x86_64-simd](x86_64-simd/) | 12 | Machine summaries, run metadata and raw benchmark arrays |

The rest of the project has two historical JSON backtest files in `docs/`,
three required WASM configuration/lockfiles, and one active shared tool contract
in `xtask/contracts/benchmark.json`. None is a codec runtime input. The shared
contract is compiled into `xtask` and read by the remaining Python capture
library; WASM configuration remains JSON for its JavaScript build tooling.

```bash
cargo xtask audit-json
cargo xtask audit-package
cargo xtask summarize-simd --summary
```

The audit includes tracked files and non-ignored untracked files, skips deleted
worktree paths, validates JSON strictly and lists byte-identical groups. Existing
phase-local copies and digest-bound paths remain intact: identical Criterion
metadata in different phases is expected, and later reviews can deliberately
retain independent resource-report copies. Do not rewrite or move these files
without updating and revalidating their manifests and report links.

For interpretation and validation boundaries, use the
[performance index](../docs/README-performance-index.md) and
[tooling migration record](../xtask/README.md). Archived failed campaigns remain
failed; the native gates do not promote them to current release evidence.
