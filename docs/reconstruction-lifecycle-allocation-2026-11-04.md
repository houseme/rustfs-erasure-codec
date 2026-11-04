# Reconstruction lifecycle allocation investigation

The archived [9.0.0 comparison](../benchmarks/encode-decode-abba-9.0.0/2026-11-04/README.md)
clones every encoded shard inside each timed reconstruction operation, then
sets one data shard and one parity shard to `None`. Its recovery numbers include
that complete caller-side copy and subsequent output allocation. They describe
that lifecycle, not reconstruction kernel time alone.

The separate [Rust probe](../benchmarks/encode-decode-abba-9.0.0/2026-11-04/reconstruction-lifecycle-probe.rs)
uses the published 9.0.0 crate and the 9.1.0 source at `730d555`. It checks
byte-identical parity across versions and complete recovered outputs. Before
each reused-slot check it damages both destination buffers, so simply marking
stale buffers present cannot satisfy the oracle. Both codecs use a one-job
budget and are warmed twice. Three calling-thread allocation windows per
version and fixture separately count: full input cloning only; cloning followed
by reconstruction; cloning only the surviving inputs, with and without
reconstruction; and marking two preallocated `ShardSlot` buffers missing
followed by reconstruction. The two `Option<Vec<u8>>` preparations are checked
for equal contents before reconstruction. Setup of the reused slots is outside
the window.

On local AArch64, with `rust-neon`, stable Rust 1.99.1 and a single Rayon
thread, all 42 windows completed. The complete
[observations](../benchmarks/encode-decode-abba-9.0.0/2026-11-04/reconstruction-lifecycle-observations.json)
contain 4+2 and 10+4 configurations from 64 bytes through 1 MiB per shard.
For 10+4 with 1 MiB shards, each version's full cloning requested 15 allocations
totaling 14,680,400 bytes; cloning plus reconstruction requested 17 totaling
16,777,552 bytes. Skipping clones of the two known-missing shards requested 13
allocations / 12,583,248 bytes for preparation, and 15 / 14,680,400 bytes
including reconstruction. The warmed reusable-slot call requested zero
allocations on the measured calling thread. At 64 bytes, preparation was
15 / 1,232 bytes for full cloning and 13 / 1,104 bytes for survivor-only
cloning; including reconstruction yielded 17 / 1,360 and 15 / 1,232 bytes.
The reusable-slot count was again zero. The three observations per case were
identical in this environment.

These are allocator request counts and requested bytes, not latency, RSS,
peak memory or a production-workload profile. Survivor-only cloning avoids
creating copies that the historical fixture discards; it changes the measured
workload and has not been established as a latency improvement. Reusable slots retain their
buffers between operations; `Option<Vec<u8>>` has a different ownership and
publication lifecycle. Callers should use buffer reuse only when that API and
lifetime fit their correctness and memory requirements. This comparison does
not establish a speedup, nor does it change the historical measurements or
justify replacing one public API with another.

To reproduce, copy the archive's `Cargo.toml.example` and `Cargo.lock` into a
separate temporary package, adjust the `next_codec` path to this checkout, and
copy the probe to `src/main.rs`. Run with `RAYON_NUM_THREADS=1` and the stable
Rust toolchain in release mode with offline, locked dependencies. Use the same
source revision, backend and target when comparing request counts; allocator
and target differences can change the observations.
