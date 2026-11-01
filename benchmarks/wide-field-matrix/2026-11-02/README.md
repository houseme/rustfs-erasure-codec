# Wide-field matrix stack follow-up

The 512-element variant was subsequently integrated behind the default-off
`experimental-matrix-inline512` feature. The measurements below describe the
historical experiment against committed source `09b7e58`; performance acceptance
remains outstanding. The 256-element variant remains unapplied. This study
investigates the existing constructor stack overflow documented in the
[small-update investigation](../../small-update/2026-11-02/README.md).

`Matrix<F>` stores `SmallVec<[F::Elem; 1024]>`. For a valid custom GF8 field
whose element occupies 256 bytes, the inline array alone is 262,144 bytes.
The local release constructor probe emitted a 2,097,584-byte AArch64 wrapper
frame, excluding callees, and the normal test thread overflowed while creating
the codec. Two disposable variants reduce the inline capacity to 256 and 512
elements (`inline256.patch` and `inline512.patch`). In the same local release
wide-field test, both variants completed constructor, encoding, update and
independent parity comparison with `RUST_MIN_STACK` unset. This is a single
host, compiler and field representation, not a universal stack guarantee.

For inline512 the same constructor wrapper emitted 1,049,008 bytes on AArch64
and 1,048,936 bytes on x86-64 Linux cross-compilation, excluding callees and,
on x86, the return address. See `constructor-prologues.txt` and
`rustc-version.txt`. No runtime stack high-water measurement was collected.
The patches change a shared generic storage representation and could alter code
layout, allocation behavior and constructor latency even for small elements.

## Allocation and correctness tradeoff

`allocation-probe.rs` constructs 4+2, 6+2, 10+4, 16+8 and 32+16 codecs in
Vandermonde, Cauchy, Jerasure-like and valid custom-matrix modes, for GF8,
tower GF16 and a three-byte custom field. Two warmups and three counted
constructor calls precede full parity verification and a missing-data recovery.
`allocation-results.json` contains all 60 cases in `no_std` for the baseline,
inline256 and inline512, plus the same 60 cases in `std` for baseline and
inline512. Only calling-thread allocation requests and requested bytes during
construction are counted; setup, encoding and recovery are outside the window.
The counts do not establish throughput, RSS or peak live memory.

In the local Vandermonde sample, GF8 4+2 through 16+8 retained one allocation
with inline512, while requested bytes fell from 1,072 to 560. For 32+16, the
same change increased construction from four requests / 6,192 bytes to six /
7,728 bytes. GF16 32+16 increased from four / 12,336 to six / 15,408 bytes.
Across all 180 counted `no_std` windows, inline512 increased request count in
nine and matched it in 171; requested bytes decreased in 171 and increased in
nine. The 256-element alternative increased request count in 45 windows.
These are observations for the listed fixtures and build, not upper bounds or
latency claims.

The inline512 variant passed 289 `no_std` library tests, 424 all-feature
library tests with five ignored, and the published 9.0.0 compatibility probe
(200 general, 36 large GF8, 12 6+2 and 32 small verification cases), all on
Apple Silicon. These tests used the earlier `09b7e58` baseline; the variant
must be rebased and revalidated against the current integrated features before
any adoption. The x86 stack result is cross-compiled only.

To reproduce, create a disposable `09b7e58` checkout, copy
`allocation-probe.rs` to `tests/wide_matrix_allocation.rs`, run it in release
with and without `--no-default-features`, and apply at most one patch using
`git apply --unidiff-zero`. The wide-field runtime test and standalone
constructor probe are in the linked small-update investigation. Native
baseline preflights and complete A/B/B/A across small and wider layouts remain
necessary before choosing any capacity or making a performance claim.

## Integrated feature and native follow-up

The default-off `experimental-matrix-inline512` feature now selects the
512-element storage; without it the original 1,024-element storage remains.
An explicit 512/513 boundary test checks actual inline versus spilled storage,
and a wide-field integration test checks four matrix modes, two layouts,
encoding and missing-data recovery. Its portable case uses a 16 MiB thread
stack. On local AArch64 macOS release builds a separate child-process test
also checks the observed default-test-stack reproduction; this is not a stack
limit guarantee on other configurations.

On an isolated `444a23b` snapshot with the feature, the release workspace
all-feature test suite (excluding fuzz), workspace all-target/all-feature Clippy
with warnings denied, `std` and `no_std` library tests, and the published
9.0.0 compatibility probe passed. The source-plan diagnostic snapshot was
updated to reflect the conditional matrix layout. After the branch advanced to
`c96fdbc`, focused `std`, `no_std`, wide-field and source-plan checks passed
on a refreshed isolated snapshot. The portable wide-field test uses a 16 MiB
thread stack; the release-only AArch64 macOS child test separately covers the
local default-stack reproduction.

On the reserved native x86 Linux VM, x86 test binaries cross-compiled from an
isolated `444a23b` source snapshot plus the feature were run after an idle
check. With `RUST_MIN_STACK` unset and core files disabled, the baseline wide
field test aborted with a stack overflow; the feature-enabled equivalent and
wide-field functional suite passed. The native output is in
`integrated-feature-run/native-results.txt`. The feature-enabled `std` result
is functional evidence. A subsequent native check used cross-compiled test
binaries from `c96fdbc` plus the feature: the 512/513 capacity boundary and
the wide-field suite passed in `std`, and the wide-field suite passed in a
`no_std` AVX2-feature build. See
`integrated-feature-run/native-final-head-results.txt`.

A separate no-default-features resource comparison on a `c96fdbc` source
snapshot plus the feature toggle repeated the 60-case catalog: among 180
counted windows, request count increased in nine, while requested bytes fell
in 171. Complete paired request data is retained in
`integrated-feature-run/resource-results.json`. These remain allocation
observations, not latency measurements.

The native constructor timing setup used `paired-probe.rs` with two source
copies differing only by temporary Cargo package metadata. The candidate build
enabled `experimental-matrix-inline512` only on its second copy. Both builds
passed ten parity and bidirectional recovery checks without timing. The
controller then completed nine cases in its first same-source baseline round
before detecting a named competing process and stopping. Across 352 monitor
observations, CPUs remained 0–15, one named-process observation was active,
no off-target busy streak was recorded, and monitored throttle/steal counters
did not change. There was no second baseline round or candidate timing, so the
whole run is invalid as performance evidence. Samples and redacted monitoring
are in `integrated-feature-run/`.

`runner-matrix.patch` applies after the previously documented cross-arm and
small-case runner patches using `git apply --unidiff-zero`. It changes the case
catalog and requires an explicit expected backend; synthetic checks rejected
backend mismatches, phase drift, same-source bias and cross-round drift.
`paired-Cargo.toml.example` and `paired-Cargo.lock` describe the two-copy
setup. Native timing still requires a new stable preflight and complete A/B/B/A
before either speedup or regression can be attributed.

## Wide-layout construction prototype

The 32+16 allocation increase was traced to the materialized top square and
inverse square spilling after the capacity change. A disposable prototype
forms the augmented elimination matrix directly and reads its inverse half as
row views during the final multiplication. The no-std resource probe observed
GF8 32+16 Vandermonde construction decrease from six requests / 7,728 bytes to
four / 5,680 bytes; in 180 measured windows across the existing 60-case catalog,
request count decreased in nine and matched in 171, with no requested-byte
increase. Full parity and recovery checks were retained.

The first prototype skipped the two temporary matrices' `Clone` calls. A safe
custom `Copy` element with an observable `Clone` implementation exposed a
change in six of 24 operation-order witnesses despite equal matrix contents.
`builder-no-clone.patch` retains that rejected version. A second prototype
replays the expected heap-fill clone calls without allocating and matched all
24 recorded field/clone operation witnesses in both `std` and `no_std`; the
independent scalar matrix reference also passed at data-shard counts 16, 22,
23, 24 and 32 for GF8, GF16 and a three-byte custom field. The witnesses and
no-std witnesses, std witnesses and resource results are in
`builder-algebra-results.txt`, `builder-algebra-std-results.txt` and
`builder-resource-results.json`.

`builder-replay.patch` is also **unapplied**. Its clone replay depends on the
current `SmallVec` and standard-library fill behavior and could become invalid
if those implementations change; it is not a generally proven compatibility
solution. Neither prototype has native performance evidence. A safer stable
contract or a separately justified type-specific path, full feature-combination
validation, and passing performance gates are required before adoption.
