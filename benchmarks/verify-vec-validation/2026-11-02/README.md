# Concrete GF8 workspace verification investigation

This experiment starts from commit `241351e`. `verify_with_workspace_opt`
accepts concrete `Vec<u8>` shards, yet forwards to the generic parallel
workspace API, which validates input lengths before preparing scratch and
then validates the same input and scratch again in `verify_with_buffer_par`.
The generic API cannot generally skip those checks: a custom `AsRef` can
return a different view on a later call. Concrete `Vec<u8>` references retain
their lengths while borrowed.

The [isolated prototype](candidate.patch) adds a GF8-only concrete helper. It
checks the shard count and every input length before preparing the workspace,
then uses the existing Leopard or Classic encoding path. It leaves the generic
APIs unchanged. This is not integrated in production: fewer source-level
checks do not establish an elapsed-time benefit.

The dedicated `tests/verify_vec_workspace_contract.rs` compares the existing
optimized entry point with the generic parallel workspace API. It covers Classic
with ordinary and fast single parity, Leopard GF8 and Leopard GF16; lengths
64, 128, 256 and 4096; valid and corrupt data, too few or too many shards,
empty first shard, late size mismatch, and corruption followed by a late size
mismatch. The test compares complete results, workspace contents and inputs
for all 112 cases. It passed both on the baseline and the prototype, under
default and all-feature builds. On local AArch64, the prototype also passed the existing 26 workspace and encoding
regression tests, release-library Clippy, and a no-default-features library
compilation check. No native x86 execution was performed.

## Baseline quality investigation

The [paired Rust probe](paired-probe.rs) uses two source copies, one worker,
short warmups, three samples per phase and A1/B1/B2/A2 ordering. It stops
immediately if same-source bias or either phase drift exceeds 5%. The copies
come from the same commit; the second package has a diagnostic version so
both can be linked. Before sampling it encodes and verifies the input using
both codecs and separate workspaces. This is an unreserved local AArch64
screen, not the native acceptance harness.

The first 4+2/64 same-source case had +8.81% relative bias and the probe
stopped. No candidate timing was performed. The separately compiled
single-function control used the same codec implementation for both arms and
separate workspaces; it stopped on 6.17% B-phase drift. These are isolated
observations, not enough to assign the original bias to code layout or to a
specific system cause. They do establish that neither run is suitable for
attributing an optimization.

In the two-copy baseline binary, both optimized entry-point functions were
1,000 bytes and occupied different positions in the executable. Of 250 raw
instruction words, 237 matched; the remaining 13 involved PC-relative calls
or static-address materialization. Their entry alignment modulo 32 differed.
Those are properties of this compiler build, not a performance explanation;
the rest of the called code and the runtime environment were not controlled.
The [observations](screen.txt) retain the failed screens.

To reproduce the baseline, archive `241351e` into sibling `base` and `same`
directories, change only the second package version to `9.1.0+diagnostic`, and
copy `paired-probe.rs`, `Cargo.toml.example` and `Cargo.lock` to a sibling
`probe` as `src/main.rs`, `Cargo.toml` and `Cargo.lock`. Build with the stable
Rust toolchain and locked dependencies. The probe stops on the first failed
gate. A same-function diagnostic substitutes the base codec for the second
codec; it is a control only, not a candidate comparison.

Native acceptance requires a fresh reserved window, complete independent
same-source preflights, cross-round checks and guarded A/B/B/A with both
supported x86 backends. Dense, invalid, corrupted, Leopard and larger controls
must remain covered. Do not use these local screens to enable the prototype.
