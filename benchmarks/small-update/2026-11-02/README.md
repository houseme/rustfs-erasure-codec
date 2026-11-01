# Small incremental-update scratch investigation

This is an **unapplied experiment**, not an accepted optimization. Allocation,
view, stack and compatibility comparisons used committed base
`09b7e5847693411fd8a063d9b071417941733d17` in disposable copies. The paired
probe's check-only run used a `9e78f6b` archive whose `src/`, `build.rs`, root
`Cargo.toml` and root `Cargo.lock` match that base. No timing comparison or
native x86 execution was completed.

## Allocation and parity observations

Classic `update` creates a `Vec` for changed delta chunks. `inline256.patch`
replaces it with `SmallVec<[F::Elem; 256]>`. `isolated-inline256.patch` instead
uses a separately framed helper only for shard lengths at most 256 elements
and element sizes at most three bytes; all larger cases retain the original
`Vec` loop. The thresholds are experimental, not a proven optimum. The helper
duplicates the existing update loop and introduces a call, so allocation savings
alone do not establish speed or justify its maintenance cost.

`allocation-probe.rs` checks GF8, tower GF16 and valid three- and four-byte
custom GF8 representations, 4+2 and 10+4 layouts, seven lengths (1, 64, 255,
256, 257, 1,024 and 16,385 elements), and no change / identical / tail / head /
two changed shards. Each full parity result must match an independent full
encode. After one warmup it records three calling-thread allocator request
windows per case and source variant. The complete 280-case, 840-window-per-
variant data is in `allocation-results.json`. Setup and expected outputs remain
outside counting; counts do not measure latency, RSS or live allocation peak.

For example, on local Apple Silicon, 4+2 with one changed 64-element shard
requested one allocation of 64 / 128 / 192 / 256 bytes for GF8 / GF16 /
three-byte / four-byte fields. The isolated candidate requested zero / zero /
zero / 256 bytes respectively. At 257 elements it retained the baseline request
for all four fields. For a 16,385-element shard with only its one-element tail
changed, the isolated candidate also retained the baseline request because it
limits by full shard length; the unbounded prototype avoided that request.
Across all 840 counted windows, the isolated candidate reduced request count in
216 and matched the baseline in 624, with no observed requested-byte increase.
These are fixture-specific observations, not latency claims. To reproduce,
copy `allocation-probe.rs` to `tests/small_update_allocation.rs` in a disposable
base checkout and run `cargo +stable test --release --locked -p
rustfs-erasure-codec --test small_update_allocation -- --nocapture
--test-threads=1`, then apply at most one candidate patch with `git apply --unidiff-zero` and
repeat.

Twelve extra one-parity fast-path controls (four fields, 64/256/257 elements)
matched parity and recorded zero requests for all three versions; see
`fast-update-results.json`. The no-op and explicitly identical inputs also
retained their parity and request checks in the main catalog.

## Generic fields and view behavior

A 256-byte custom field revealed a stack risk in the unbounded prototype.
`stack-probe.rs` monomorphizes a non-inlined update caller. In Rust 1.99.1
release code with no explicit SIMD features, the emitted static caller frames
(including saved registers; x86 excludes the return address) were:

| Variant | AArch64 Darwin | x86-64 Linux cross-compiled |
| --- | ---: | ---: |
| Baseline | 448 B | 392 B |
| Unbounded inline | 65,984 B | 65,880 B |
| Isolated inline | 448 B | 392 B |

The prologues and compiler version are retained in `stack-prologues.txt` and
`rustc-version.txt`. These figures exclude callers, callees and dynamic stack;
they are not runtime high-water measurements. The unbounded prototype is
rejected for this compatibility risk.

The same 256-byte field's **baseline** runtime test aborted with a stack overflow
on the local test runner when `RUST_MIN_STACK` was unset. Instrumentation printed
entry into the test but not completion of `ReedSolomon::new`, so this observation
is before the update operation and is not attributed to the candidate. Both
the baseline and isolated candidate passed the same field's forward update
and independent full-encode comparison when run with
`RUST_MIN_STACK=16777216`; the isolated candidate also passed without the
codec's default features at that stack setting. The existing matrix stores
1,024 elements inline, but this observation alone does not isolate the complete
call chain or establish a general stack-size guarantee.

`view-probe.rs` uses safe, counted `AsRef`/`AsMut` wrappers. With GF8, no SIMD
features, three lengths (64, 256, 257), and six no-op, changed and shrinking
view conditions, baseline and isolated candidate had identical access counts,
return values and complete parity arrays for all 18 cases. The full records are
in `view-results.txt`. They do not prove equivalence for every possible custom
view or field implementation.

## Functional verification and caveats

With `isolated-inline256.patch` applied, the unmodified existing
`incremental_update_regressions` suite had seven passes and three failures:
the three failures expected **exactly** one allocation even when the candidate
made zero. In a disposable copy only, `test-resource-upper-bounds.patch` changed
those allocation assertions to the same upper limits (apply with
`git apply --unidiff-zero`); all 10 tests then passed
in default and no-default-features release builds. No production test has been
changed or weakened. The test-only patch is not included in the code candidate.

The isolated candidate also passed the local released 9.0.0 compatibility probe
(200 general, 36 large GF8, 12 6+2 and 32 small verification cases), release
library Clippy with warnings denied, and a no-default-features AVX2 x86 Linux
cross-check. The isolated candidate additionally passed all 280 resource
fixtures in a no-default-features build. Cross-compilation does not establish
native execution.

`paired-probe.rs` prepares forward and reverse updates, verifies complete parity
against independent encodes before timing and after every phase, and compares
4+2 and 10+4 at 64, 256, 257, 4,096 and 1 MiB per shard, plus no-op and
fast-parity controls. Each measured operation is one forward and one reverse
update. Samples batch 16 operation pairs for shards up to 64 KiB to amortize
clock reads, and one pair for the 1 MiB control. `--check-only` ran all 12 fixtures successfully on Apple Silicon for
both same-source and candidate pairs; those checks collected no timing. The
probe cross-compiled to Linux x86; it has not run there.

The prepared `runner-update.patch` changes only the expected case catalog of
the previously retained streaming runner. Reconstruct that runner from commit
`70898c1`, apply `cross-arm-gate.patch`, then `runner-small-cases.patch`, then
this patch with `git apply --unidiff-zero`. Synthetic checks covered all 12 IDs and confirmed A drift,
same-source bias and cross-round drift above 5% are rejected. Native timing
requires the original two baseline rounds, resource monitoring and complete
A/B/B/A gates before any candidate timing; none were performed in this study.

For reproduction, archive the base commit into sibling `base` and `same`
folders. Change only `same`'s package version to `9.1.0+diagnostic` so Cargo
can link both copies, and copy `paired-probe.rs`, `paired-Cargo.toml.example`
and `paired-Cargo.lock` into a sibling `probe` as `src/main.rs`, `Cargo.toml`
and `Cargo.lock`. First build and run `--check-only` with identical sources.
Use separate output directories for the unchanged binary and the candidate
binary; apply only `isolated-inline256.patch` with `git apply --unidiff-zero` to `same`
for the latter. Do not
use the instrumented resource build for latency. The stack probe has its own
`stack-Cargo.toml.example` and `stack-Cargo.lock`; arrange a sibling `codec`
source copy, and use `RUST_MIN_STACK=16777216` for its runtime test.
