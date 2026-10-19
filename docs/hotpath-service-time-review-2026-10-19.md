# Hot-path and service-time review — 2026-10-19

This review starts from codec main `94c6951` (merged PR #24). It changes measurement
placement and extends the existing diagnostic probe, not codec algorithms, encoded
formats, production APIs or release thresholds. Backlog #2897 remains open.

## Actual caller and profiling boundaries

Two source snapshots were inspected: local RustFS `3f85abdd5` and the authorized
Linux consumer `902c97ce`. Both depend on published codec 9.0.0 and construct the
modern codec with `ReedSolomon::new`, selecting Classic. Neither inspected path
selects Leopard. Source reachability does not establish request frequency or the
engine configured in a running deployment.

The RustFS streaming engine's `reconstruct_into` recovers complete shards when
missing data has surplus source parity, then `verify_source_parity` calls ordinary
`codec.verify`. Its supplied decode workspace contains only `shard_len` and is
unused by reconstruction; verification creates a reference Vec and library scratch
again. Reader state already owns one workspace, which could own reusable scratch
without introducing a shared mutex. However this engine is selectable and defaults
to `legacy` in the inspected source. The legacy/default reconstruction-verification
path retains source parity, reconstructs/re-encodes and compares it. Workspace reuse
in the optional engine cannot be described as a speedup for every default GET.
Healing also calls `verify_data_and_parity`.

Historical controlled software profiles used consumer `902c97ce` and codecs
`8f49bd8` / `c68c257`, not current main. They contain 7,495 / 7,437 cpu-clock leaf
samples: HighwayHash append about 55.0% / 53.8%, generated 4+2 AVX2 encoding about
11.6% / 11.5%, and resolved malloc about 0.83% / 0.55%. Unresolved leaves remain
18.1% / 18.9%, so the malloc fraction is not an upper bound on allocator cost.
These profiles guide investigation only; they are not current runtime proportions,
a current verification hotspot proof or version-performance attribution. A fresh
request-level profile identifying the selected engine is needed before changing
consumer runtime code.

## Formal placement correction

The formal sampler previously accepted any endpoint CPU within the reservation;
exact measuring-TID pinning existed only in the separate placement diagnostic.
Protocol 13 makes preparation common to every formal case:

1. Keep the complete reserved CPU mask while constructing caller fixture/codec.
2. Initialize the global four-worker Rayon pool and check every inherited mask.
3. Pin only the measuring TID to the minimum reserved CPU. Check pool masks again.
4. Check actual measuring-thread affinity before/after every timer using bounded
   stack storage. Export fixed CPU, constructor budget, pool width and checks.
5. Require matching placement through calibration and all four measured phases.

Initializing the pool before pinning matters: lazy workers otherwise inherit the
single CPU even if the codec's cached budget was constructed on the full set.
The caller inventory now follows this same contract rather than pinning its Python
controller before source construction. No per-operation clone/allocation/I/O loop
is removed. Pool preparation and taskset run before warmups; per-sample mask checks
are outside the inner timer and independently checked for zero heap activity.

This is an explicit workload/observer change. Previous protocols remain historical
and new calibration/evidence is required. Earlier unchanged-9.0.0, same-buffer,
exact-fixed-CPU encode controls still changed from 304.64 to 351.99 microseconds
(+15.54%), while accounted thread runtime changed about +15.36% and runnable wait
was below 0.04%. Pinning closes a comparability gap; it does not uniquely explain
frequency, cache, memory or invisible host effects. No failed full timing matrix is
repeated here, and no service-time or release success is inferred from native checks.

## Verification resource screening

The existing `recovery_paths` executable now contains six cases: ordinary verify,
caller buffer and VerifyWorkspace, at 4+2/16 KiB and 10+4/32 KiB per shard. These
mirror source-reachable verification APIs without claiming a complete GET pipeline.
Each layout shares retained source/reference storage across API variants. Scratch
and workspace preparation are outside measurements. Valid parity and a corrupted
parity tail must be distinguished before/after measurement. The ordinary API case
continues to include its own scratch allocation.

```sh
RUSTUP_TOOLCHAIN=stable RAYON_NUM_THREADS=4 \
RSE_RECOVERY_CASE_FILTER=verify. RSE_RECOVERY_SAMPLE_MS=1 \
cargo bench --locked --features 'std simd-accel' --bench recovery_paths
```

The short sample setting above is for allocation/correctness screening only. Its
latencies do not qualify duration, distribution, drift or throughput. Local
macOS/ARM64 screening observes:

| Layout | Ordinary verify requests/bytes | Caller buffer | VerifyWorkspace |
| --- | --- | --- | --- |
| 4+2 / 16 KiB | 1 / 32 KiB | 0 / 0 | 0 / 0 |
| 10+4 / 32 KiB | 1 / 128 KiB | 0 / 0 | 0 / 0 |

These count synchronous API allocation/reallocation requests. Caller reference
construction, scratch initialization and cold workspace cost are excluded. The
current downstream function also constructs a reference Vec per stripe; that cost
is not included in this table. Ordinary verify uses contiguous scratch, while the
reusable APIs retain separate parity buffers: equal output semantics do not imply
identical scratch placement or a matched-latency comparison. No percentage speedup,
RSS or peak-memory improvement is claimed.

## Leopard owned-output feasibility

Current generic recovery allocates a decoded Vec for each requested missing shard,
finishes decoding, then initializes and copies to each destination. This preserves:

- Every already-published output contains valid decoded bytes when a later fixed
  destination rejects initialization; the failing slot retains its state/storage.
- Missing unrequested shards are neither initialized nor published.
- Validation and decoder failure occur before publishing recovered outputs.

A recovered 32 KiB owned output plus a new 32 KiB destination accounts for the two
warmed requests observed for both Leopard families. `ReconstructShard` exposes
borrowed slices and fallible initialization, not owned-buffer adoption. Also,
`Field::as_gf8_slice` establishes a borrowed byte view, not proof that a Vec of bytes
may be reinterpreted as a Vec of arbitrary field elements.

An eventual u8-owned Option<Vec<u8>> entry could decode into owned outputs and move
them after successful decoding, retaining the generic path as the fallible-storage
adapter. This can be implemented without an unchecked Vec layout cast, but needs
an actual Leopard consumer, lifecycle profile and complete compatibility/error
oracles before a runtime/API change. Existing selective/multichunk-tail and
late-fixed-destination regressions remain the required contract. Removing generic
scratch or marking caller buffers present before decoding is not the proposal.

## Decision

Deliver the formal placement correction and reproducible verification resource
screening. Preserve production code while request-frequency/current-profile and
service-time evidence remain incomplete. Evaluate consumer-owned verification
scratch first if that selected engine is a measured hotspot; defer Leopard API
changes until a real consumer warrants them. No task-6 waiver, accepted performance
snapshot, tag or registry upload follows from this review.
