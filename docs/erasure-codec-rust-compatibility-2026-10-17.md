# Rust declaration and compatibility review — 2026-10-17

This follow-up to [PR #7](https://github.com/houseme/rustfs-erasure-codec/pull/7)
removes `workspace.package.rust-version = "1.98.1"` and the three packages'
`rust-version.workspace` inheritance entries. Removing only the workspace field
would leave invalid member inheritance. Edition 2024, resolver 3, dependency
versions, Cargo.lock, package versions and Rust runtime sources are unchanged.

Cargo metadata confirms all three packages parse with `rust_version: null`.
The locked host normal/build dependency graph still includes declared Rust
requirements up to 1.85.0 (hashbrown) and 1.85 (hashlink); this is dependency
metadata, not a verified minimum compiler for this project or every SIMD target.
The workspace intentionally makes no older-toolchain support promise. Historical
compiler labels in test fixtures and earlier reports remain accurate records.

## Executed validation

Local compiler: stable Rust 1.99.1 on macOS ARM64. Counts use the last parent
summary per executable and exclude nested child summaries.

| Check | Result |
| --- | --- |
| Locked workspace std + SIMD + metrics | 495 passed, 43 ignored: core 488 plus WASM Rust tests 7. |
| Locked core no_std + SIMD | 297 passed, 20 ignored; core selected independently. |
| All workspace targets/features Clippy, warnings denied | Passed, including fuzz compilation. |
| Locked WASM crate check on wasm32-unknown-unknown | Passed. |
| Manifest metadata, formatting, typos and diff checks | Passed. |

No compatibility defect was found in these tested configurations. No historical
Rust compiler was installed or tested for this follow-up. The final source's
platform/compiler CI, actual package verification and generated Node/Chromium
integration are checked on the PR before the requested rebase merge; their exact
SHA and merge outcome are recorded in PR/backlog rather than substituting older
CI evidence. This manifest change does not create a tag or release, or replace
the [uncompleted performance acceptance](erasure-codec-recovery-metadata-validation-2026-10-17.md).
