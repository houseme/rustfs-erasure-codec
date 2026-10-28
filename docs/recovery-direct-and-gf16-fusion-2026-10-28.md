# Recovery direct destinations and GF16 decode fusion

This is a compatibility and allocation review of a candidate based on
`6111b62`. Runtime changes were subsequently committed as `f2f57a0` and
`71443ac`. The observations below were made against the preceding dirty working
tree; their source identity must be checked before reuse for either commit or a
later revision. They are not accepted performance or release evidence.

## Scope

The Leopard implementations of the `reconstruct_opt`, `reconstruct_data_opt`,
and `reconstruct_some_opt` APIs decode directly into concrete `Option<Vec<u8>>`
destinations. Existing inputs are validated first, only requested missing slots
are initialized, and a decoder error or unwind rolls those initialized slots back. The
generic `ReconstructShard` APIs retain their existing staged publication,
including their behavior when a later destination rejects initialization.

`ShardSlot::as_inner` exposes the backing bytes even while a slot is missing.
The staged generic decoder leaves these bytes unchanged when decoding returns
an error or unwinds. Fault-injection tests exercise both cases after partial
decoder output. Directly writing that storage before success would change this
observable behavior; preserving it would require restoring the old bytes or a
separate opt-in contract. This is a compatibility boundary, not a claim that
the injected decoder failure is reachable for validated inputs today. The same
fault-injection test passed against the `6111b62` baseline after adding only
the test source to that baseline checkout.

Little-endian AArch64 fuses GF16 decode Step 1 and Step 5 layout conversion and
fixed-multiplier calculation using baseline NEON. Standard-library x86_64 uses
fused AVX2 or SSSE3 when detected. Non-NEON `no_std` targets use portable
scalar fusion; other standard-library targets retain the previous implementation.
The `MODULUS16` identity sentinel uses the existing layout conversions. Scratch remains
necessary for FFT operations and for the fallback.

## Compatibility evidence

- Current generic and Option-specific entrypoints are compared for Leopard GF8
  and GF16 over erasure patterns, selected outputs, invalid lengths, and chunk
  boundaries. A decoder-error injection checks rollback of initialized Options.
- Both selective entrypoints are also checked directly against original bytes
  for every missing mask of up to three shards and every subset of those missing
  shards at 64 bytes. Additional data/parity combinations cross a chunk boundary
  with one- and two-job limits. These cover 1,692 calls on little-endian builds.
- Custom containers exercise differing immutable/mutable lengths and a
  `FromIterator` implementation that creates a short destination. Existing
  partial-publication semantics remain covered.
- Fused kernels are compared with the existing separate operations for block
  boundaries, unaligned byte views, multiple multipliers, and every GF16
  element for selected multipliers. Separate tests cover all 65,536 multiplier
  values on representative blocks; they ran on AArch64 NEON and on emulated
  x86 AVX2, SSSE3 and `no_std` scalar paths.
- On AArch64 macOS, the full all-features suite, the no-default-features plus
  `simd-accel` library suite, the 9.0.0 compatibility probe, and the targeted
  Option cross-version test passed. The latter tests full recovery and selective
  outputs using parity produced by each version. Published 9.0.0 recovers all
  missing Leopard shards even for selective APIs; the current API deliberately
  keeps unrequested shards missing. A short final GF8 chunk caused an assertion
  in the published 9.0.0 build, so the cross-version test uses complete chunks;
  the current implementation has separate short-tail regression tests.
- x86_64 macOS `cargo check` passed for standard and no-default-features builds;
  no-default-features checks also passed for x86_64 Linux, 32-bit Linux, and
  WASM. The x86_64 Linux fused-kernel tests and selective Option recovery tests
  ran in an offline emulated Ubuntu container that reported SSSE3 available and
  AVX2 unavailable. GF16 library tests passed 35 with one ignored, the three
  selective Option integration tests passed, both the published-9.0.0
  cross-version Option test and 200-configuration probe passed. After portable
  scalar fusion was added, 30 `no_std` GF16 tests passed in the same emulator.
  This is emulation, not native x86 evidence.
- Offline QEMU with `AVX2=true` passed 39 GF16 library tests, with one ignored,
  all three Option recovery tests, the 9.0.0 Option cross-version test and the
  200-configuration compatibility probe. AVX2 kernel comparisons covered every
  field element for selected multipliers, every non-identity multiplier on a
  representative block, unaligned buffers and chunk boundaries; the separate
  dispatch test also covers the identity multiplier. SSSE3-only emulation was
  rerun after AVX2 dispatch was enabled. QEMU is functional evidence only.
- An offline QEMU `qemu64` run reported `SSSE3=false, AVX2=false`. Four
  fused-wrapper/scalar equivalence tests, all three Option recovery tests, and
  the injected generic-slot error/unwind test passed. The 9.0.0 cross-version
  Option test and the 200-configuration compatibility probe also passed. The
  SSSE3-specific test returned without executing because that feature was
  unavailable.
- `cargo-semver-checks` against the `6111b62` source baseline passed 229 checks
  and skipped 31; this is an API check, not proof of every downstream behavior.
- A dirty-tree, offline `cargo package --allow-dirty` verified the 73-file core
  package, including the new fused sources. This is not an immutable release
  archive or permission to publish it.
- The actual dirty-tree candidate archive was extracted and its 66 Rust source
  files compared byte-for-byte with the working tree. With both 9.0.0 and the
  extracted candidate built with codec default features disabled, a real Node
  WASM run passed 126 GF16 encoding and recovery checks. The 64-byte, 65,536-byte
  and 65,600-byte short-tail cases cover parity equality, both cross-decode
  directions, data-only recovery, and selective data/parity masks. The probe
  checks the respective versions' distinct treatment of unrequested shards.
  `cargo tree` confirmed neither codec enabled `std`. The recorded run
  used Rust 1.99.1, wasm-bindgen 0.2.129 and Node 26.12.1; it is functional
  evidence for that dirty archive, not a release archive or browser test.
- The same 126-check probe and lock also passed with `std` disabled in both
  codecs when executed as an AArch64 macOS binary and as an x86_64 Linux binary
  under offline QEMU `qemu64`. Feature-tree checks verified both builds. The
  cross-linked x86 run emitted a linker warning about an ignored deprecated
  optimization setting; it is functional emulation, not native or timing data.
- A bounded 10-second `fuzz_codec_contracts` run completed 149,436 executions
  without reporting a failure. After adding unwind rollback, a seeded 10-second
  run completed 144,229 executions without reporting a failure. These are smoke
  runs, not exhaustive fuzzing.

The warmed, single-worker allocation regression compares generic and optimized
Option recovery for GF8 and GF16: 4+3 with two 64 KiB missing outputs, 10+4
with four 1 MiB missing data outputs, and 16+8 with two 64 KiB missing data and
parity outputs. Each recovered shard is checked against the original. The
optimized path requests at least one missing-shard size less per target: at
least 128 KiB, 4 MiB and 128 KiB respectively in these cases. This counts
process-wide requested allocation bytes, not peak memory or latency. In the
recorded AArch64 release-mode run, both codec families requested 256 KiB versus
128 KiB, 8 MiB versus 4 MiB, and 256 KiB versus 128 KiB respectively. The same
allocation and recovered-byte assertions also passed with the same requested
byte counts in offline x86 QEMU with AVX2 enabled and with both AVX2 and SSSE3
unavailable; these are not native x86 performance measurements.

The `leopard_recovery_comparison` Criterion benchmark contains 96 cases: GF8
and GF16; 10+4 and 16+8; 64 KiB and 1 MiB; one through four missing data
shards; and retained slots, generic Option, and optimized Option entrypoints.
Option input setup uses `BatchSize::PerIteration` and is outside the measured
operation; it is not allocation-count evidence. The retained slot case keeps
its buffers between operations and includes marking them missing in its timing.
Compare revisions within the same entrypoint, not the slot and Option timings
as if they included identical preparation. Each case checks restored bytes in preflight.
The same harness completed all 96 Criterion `--test` smoke cases on both
baseline and candidate. Their `--list` outputs contain the same case IDs in the
same order; no timing was collected.
The baseline smoke used an unchanged `6111b62` codec with this same benchmark
harness and its manifest declaration added solely for the test. A timed run
must record both the harness and codec identities.

The pinned [probe](../scripts/diagnostics/wasm_no_std_gf16_compat.rs) and
[runner](../scripts/run-no-std-gf16-compat.sh) can repeat the checks against an
extracted candidate crate in WASM or native execution. The runner uses the
adjacent lock fixture rather than replacing the workspace lock, verifies the
no-`std` feature trees, and runs offline. Both modes require Node; WASM also
requires the wasm32 target and wasm-bindgen 0.2.129. The probe is GF16-specific:
the historical GF8 short-tail assertion is not treated as a GF16 failure.

```bash
cargo +stable package --offline --locked --allow-dirty -p rustfs-erasure-codec
mkdir -p /tmp/rse-candidate-package
tar -xzf target/package/rustfs-erasure-codec-9.1.0.crate -C /tmp/rse-candidate-package
scripts/run-no-std-gf16-compat.sh wasm /tmp/rse-candidate-package/rustfs-erasure-codec-9.1.0
scripts/run-no-std-gf16-compat.sh native /tmp/rse-candidate-package/rustfs-erasure-codec-9.1.0
```

Use a new extraction directory for each run and retain the actual crate archive
and source/lock provenance separately when the result is used as evidence.
Native mode defaults to the Rust host target; set `RSE_COMPAT_TARGET` and the
corresponding Cargo target runner for cross-target execution.

## Post-commit validation and structural follow-up

The clean `3727d40` commit was packaged and verified with its own clean VCS
identity; all 66 packaged Rust sources matched the checkout. The all-features
suite, `no_std` library suite, 9.0.0 Option cross-version test and 126-check
WASM, native AArch64 and emulated x86 `no_std` package probes passed against
that committed source. This is functional evidence for `3727d40`, not formal
performance acceptance for a later tree.

The subsequent validation refactor removes the parallel Option-only validator
and reuses the generic validator before either path initializes destinations.
The generic loop and its error ordering are retained; `Option<Vec<u8>>` uses the
same checks and no longer needs a separate scan for requested missing shards.
A paired diagnostic built the `3727d40` package and the candidate source in
separate directories with identical dependency locks. For both Leopard families,
it compared the complete result and every shard byte across 46,080 cases:
all seven-shard missing masks, five API/mask choices, and one or two malformed
present lengths. Outputs matched byte-for-byte. Dedicated error-precedence and
unwind tests, the all-features suite, `no_std`, packaged probes and x86
SSSE3/AVX2 emulation checks also passed. The diagnostic outputs were local
functional artifacts, not latency measurements.

After the refactor was committed as `5e6c37e`, its clean 73-file crate archive
recorded that exact revision and all 66 packaged Rust sources matched the
checkout. The package allowlist and the existing shard-slot diagnostic passed.
The archived crate also passed the 126-check `no_std` cross-version probe in
Node/WASM, native AArch64 macOS and x86 QEMU. The last is emulated, and its
cross-linker emitted the warning noted above. These checks do not establish a
performance result or transfer release qualification to later revisions.

The fused backends share `NibbleTables16` generation, but each keeps its own
architecture-specific plane operations and final layout stores. There is still
similar arithmetic in the existing multiply kernels; extracting common vector
helpers should be assessed with generated-code checks and controlled benchmarks
before changing those hot paths. The decoder's error-locator values are fixed
across chunks, while fused calls outside the `MODULUS16` fast path rebuild
nibble tables per shard and chunk. Reuse is a possible follow-up, not a measured bottleneck: any design
must bound per-plan memory, handle cache eviction and concurrency, and prove no
regression for small shards and cold plans before adoption.

A [test-only work-count diagnostic](../benchmarks/recovery-table-work/2026-10-28/)
confirms that the observed table-construction count scales with chunk count for
three serial AArch64 `10+4` recovery cases. The diagnostic is source-bound and
does not measure CPU time, cache behavior or end-to-end performance. Isolated
per-call prototypes show both a potential reduction in table builds and storage
tradeoffs; none is included in the production path or accepted as a speedup.
The [parallel prototype follow-up](../benchmarks/recovery-table-work/2026-10-28/parallel/)
also records an instrumentation-isolation failure and an unresolved workspace
allocation outlier rather than treating them as performance evidence.

## Outstanding evidence

The generic container path is not fused. No latency or throughput claim follows
from these functional tests. Controlled, resource
isolated baseline/candidate ABBA and a passing A2 baseline drift gate are needed
before attributing a performance change; an actual x86 execution is required
before making x86 runtime claims. These results do not satisfy the repository's
release preflight or authorize a release.
