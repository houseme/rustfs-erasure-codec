# Wide-field matrix stack follow-up

This is an **unapplied experiment** against committed source `09b7e58`; it is
not part of either opt-in feature and has not passed a performance gate. It
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
