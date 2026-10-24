# Shared tooling architecture review — 2026-10-24

The recommended implementation is the current shared model with borrowed and
owned projections. This pass found no additional supported defect or architectural
reason to add another validator, scheduler, fallback or codec tuning policy.
Further implementation changes need a measured workload or a concrete failing
contract rather than a requirement to produce another patch.

## Reviewed source

This metadata follow-up to merged PR #101 is based on main
`c26f242f2e2410aab99122eba70c657c2d6e1ac0`, which also contains the GFNI scalar-tail
frame change. The reviewed implementation commit is
`7797045` with its original review record replayed as `16a3464`. The four tooling
implementation files are byte-identical to local snapshot `9007748` and the
previous measured implementation `fcf12ca`; their historical resource observations
retain their original source identities.

The follow-up changes repository tools only. Codec source, observer templates,
encoding formats, manifests, dependencies, lockfiles and historical JSON members
match this main exactly. Existing release acceptance boundaries remain separate.

## Why the boundaries are retained

| Responsibility | Owner | Review conclusion |
| --- | --- | --- |
| Scalar, alias and row normalization | `benchmark_contract` | One rule set; compile-time borrowed or owned projection before collection. |
| Raw scalar types and source-bound release requirements | `release_rows` | Retain original-row checks alongside permissive benchmark normalization. |
| Complete worker schedule, fixtures, telemetry and placement | `sampling_contract` | Check every warmup and observation before selecting a result. |
| Exact stable median and spread | `batch` and `statistics` | Keep original scalar types and stable observation identity. |
| Persistent results and external report schemas | Index/report adapters | Own data at retention/output boundaries; preserve raw hotspot provenance. |
| Decoder planning and ISA-specific execution | Codec planner and SIMD backend | Retain validated shape specializations and checked vector bounds. |

The generic projection has two concrete uses, with no runtime mode switch and no
second parsing or validation algorithm. It avoids copying metadata for discarded
worker views while keeping ordinary owned-report allocation counts unchanged.
The earlier allocation probe therefore supplies a concrete reason for its modest
type-level complexity. This pass does not relabel that probe as a new timing or
codec-throughput measurement.

The raw release checks and normalized benchmark checks intentionally enforce
different contracts. Collapsing them would weaken original scalar/source checks.
Likewise, checking only the selected median would miss malformed nonselected rows.
Those layers are preserved for their observable behavior.

The current GFNI entry checks equal lengths and trivial/short inputs before native
writes. Full vectors remain bounded by complete chunks; nonempty scalar tails are
outside the native vector frame. No additional failure was established in these
inspected boundaries, so this follow-up leaves the native implementation intact.

## Real retained-data comparison

Separately built release-mode native helpers from main `c26f242` and candidate
`16a3464` replayed all ten retained JSON artifacts that directly contain ordinary
benchmark row arrays (`throughput_mb_s` and `ns_per_iter` on the first row).
These contain 672 original rows. The 2,036 framed replies are byte-identical:

- Ten whole ordinary reports are accepted by both helpers.
- Ten whole strict reports remain refused by both helpers.
- All 672 per-row provenance queries return the same complete outcomes.
- All 672 individual ordinary row reports are accepted with identical output.
- All 672 individual strict row reports retain the same refusal.

The strict historical refusals remain refusals; no incomplete old report is
promoted to formal evidence. Input member hashes, case catalog, exact requests,
complete replies, source-bound helper binaries and summary are retained under
ignored `target/global-review/`. This extends the prior synthetic/wire comparisons
without changing original artifact bytes. It covers these ten direct row artifacts,
not every JSON schema stored in the repository.

## Current integration validation

All of the following pass on the combined source, using stable Rust:

- 316 tool tests (277 unit, 29 CLI and ten controller CLI tests).
- 555 codec tests and ten doctests; 43 existing tests remain ignored.
- Strict host tooling Clippy, codec-only no_std and x86 Linux codec library compilation.
- JSON, package, workspace and no-Python audits; 623 historical/contract JSON files
  remain 6,587,511 bytes, codec/WASM package allowlists remain 71/6, and all five
  workspace members inherit root dependencies.
- Formatting and diff checks.

The x86 codec test cross-check was attempted but its `alloca` development dependency
requires the unavailable `x86_64-linux-gnu-gcc`. The subsequent library-only x86
check passes. This is an environment-limited test check, not a codec diagnostic;
x86 native execution and isolated performance capture were not performed here.
Existing main CI results do not replace verification of this local follow-up.

No production implementation changes were added in this review pass. The metadata
optimization is retained; this document records its current-main integration and
compatibility evidence.
