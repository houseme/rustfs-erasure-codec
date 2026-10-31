# Published 9.0.0 compatibility probe

This unpublished tooling workspace member links the actual crates.io
`rustfs-erasure-codec =9.0.0` and the repository's exact `9.1.0` path package.
A shared probe is instantiated against each API; no old crate is added to the
codec dependency graph or distributable. Both renamed dependencies inherit their
exact version/path/features from the root `[workspace.dependencies]`; the root
`Cargo.lock` is the only workspace lockfile. The probe is not a default member.

```bash
cargo run --locked --manifest-path scripts/release-compatibility/Cargo.toml --target-dir target
```

All 200 configurations must encode/verify and roundtrip a missing data shard.
The probe also decodes new parity with the published old decoder and old parity
with the candidate decoder. Each direction loses a data shard and, where at least
two parity shards are available, a parity shard; the complete recovered data and
parity must equal the original shards.
Every complete parity array is compared element for element as typed byte vectors from the built-in
u8/[u8;2] element representation. The probe covers five layouts, four matrix options,
Classic GF8/tower GF16, Leopard GF8/GF16 and aligned/partial lengths, with
simd-accel on both versions. Fast-one-parity mode is disabled: this compares valid
normal formats, not the previous inconsistent fast/incremental behavior.

The root lockfile records the consumer dependency graph. Exact version
requirements prevent silently reusing this evidence for another release. CI runs
this executable in the release-tooling job. Its result is functional compatibility,
not a latency or throughput benchmark.

The probe also preserves the former Python-2-style Sage worksheet's four fixed
tower-field arithmetic vectors. Addition, multiplication, division and inverse-X
results run on both 9.0.0 and 9.1.0, using the existing GF(256) generator and
extension polynomial; the obsolete Sage executable has been removed.

It additionally checks 36 Classic GF8 cases around the 32 KiB shard boundary:
three generated-code shapes, four matrix modes including a valid custom matrix,
and lengths 32767, 32768 and 32769. Complete parity must match between versions,
and both versions must verify and reconstruct shards encoded by the other.
The workspace's normal `simd-accel` features may select GFNI or AVX2, so this
functional probe alone does not prove an AVX-512-only generated path ran.
Another 12 cases check 6+2 at 4095, 4096 and 4097 bytes with the same four
matrix modes and bidirectional recovery. The currently unmerged 6+2 AVX2
candidate requires a separate instrumented build to prove its path ran.

The optional resource report extends the original 15-case catalog with 30
incremental-update cases: GF8/tower GF16, four lengths, zero/one/two changed shards,
and a 33-parity boundary case for each field/change count. One existing command
and allocator counter cover all 45 cases and preserve existing files. Reports
retain two warmups and all three counted repetitions per version. Update results
are checked against full re-encoding on each version and against one another;
input, parity cloning and expected-output construction stay outside counting.
These are allocation/reallocation request counts and requested bytes, not RSS,
peak live memory, latency or formal release evidence.

```bash
RUSTUP_TOOLCHAIN=stable RAYON_NUM_THREADS=4 RS_PARALLEL_POLICY_MAX_JOBS=4 RUSTFLAGS='' \
  cargo run --locked --quiet --release \
  --manifest-path scripts/release-compatibility/Cargo.toml --target-dir target \
  -- --allocation-report /path/to/new-resources.json
```

## Verification workload resource report

The separate `--verify-resource-report FILE` mode leaves the existing 45-case
report and its schema unchanged. It compares the published and current codecs
on the same Classic Vandermonde fixtures with fast parity disabled and
`max_parallel_jobs=1`: GF8 and tower GF16, six element-count/layout controls,
ordinary `verify`, caller-buffer verification and retained-workspace verification,
and valid / first-parity-head / last-parity-tail inputs.

The 108 observation cases keep two warmups and three counted calls per version
(648 counted windows). All returned booleans are checked; each fixture's complete
encoded parity is compared across versions. Fixtures are paired and dropped one
at a time to avoid retaining full parity copies for every observation. Field
elements and bytes are recorded separately. Buffer/workspace setup stays outside
counting while ordinary verification retains its own API scratch lifecycle.
No allocations are asserted to equal today's implementation: future candidates
can reduce them while preserving the same validation and corruption oracles.

```bash
cargo +stable run --release --locked -p rse-release-compatibility \
  -- --verify-resource-report /path/to/new-verify-resources.json
```

Both modes preserve existing report files. The original 200-configuration
functional and cross-decoding preflight still runs first. The new report is a
request-volume measurement, not RSS, latency, a production workload profile or
performance acceptance. Bind the generated file to the exact source, lockfile,
compiler and invocation before comparing it with another run.
