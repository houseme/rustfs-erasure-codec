# Recovery ownership and residency review

This review separates caller preparation from reconstruction using completed
instrumented records. It adds an executable allocation regression for existing
buffer-reuse APIs. No codec algorithm, allocation policy, public API, formal
sampler or historical acceptance limit changes.

## Complete historical replay

The [complete aggregate](../benchmarks/release-9.1.0/2026-10-24/owner-residency-checkpoint-review.json)
retains all 348 workers across three 29-case conditions, both roles and both
calibration/sample modes: 4,176 observations and 4,608 ordered reconstruction
checkpoint intervals. Run lengths encode consecutive identical intervals
losslessly; their expansion was checked against every original interval.

An independent reader verifies all raw log/JSON/journal pairs, original worker
summaries, allocation generations, live owner views, frees, call boundaries and
process-counter bounds. All 1,054 input files remain unchanged. It launches no
subprocess, helper or codec worker. Two earlier reader failures are retained:
the frozen controller's import attempted an unavailable compiler lookup, and an
initial reader incorrectly combined timing and sample records. Neither failure
belongs to the original collection. The successful reader independently checks
the recorded events and compares each timing witness with its sample record.

The actual candidate is `c81c477`, versus published 9.0.0. Its `src`
tree matches historical `022c0a7`, but manifests/configuration/lockfiles differ.
These are two-iteration source-copy diagnostics, with explicit
`formal_performance_geometry_equivalence=false`. They are not measurements of
the current [GFNI candidate](gfni-direct-affine-review-2026-10-24.md).

## What the checkpoints locate

For every measured call of `reconstruct_data:10:4:65536` in the original and
explicit 128 KiB trim conditions:

| Process-counter interval | Baseline minor faults | Candidate minor faults |
| --- | ---: | ---: |
| Last retained caller view to first cloned dynamic view | 0 | 192 |
| Last cloned view to last missing view | 0 | 0 |
| Last missing view to first recovered view | 0 | 0 |

The 4+2/64 KiB data-only case similarly records 64 candidate faults before
decoder entry and zero in the missing-to-recovered interval. The original
10+4/64 KiB candidate has twelve cloned owner allocations whose allocation-time
snapshots are partly resident; the two subsequently recovered output allocations
are fully resident. The baseline's corresponding allocation snapshots are fully
resident. This supports a caller-lifetime/residency mechanism under these
instrumented conditions. It does not support labeling the 192-fault increment as
an independently measured cost inside reconstruction arithmetic or output
initialization.

The full record also contains counterexamples. Under explicit trim conditions,
larger 4+2 shards have nonzero missing-to-recovered increments on both versions:
66 at 128 KiB, 130 at 256 KiB and 258 at 512 KiB. Other reconstruction cases
remain zero. The complete result retains these cases, both modes and warmups.
Stream verification has allocation/view records but no equivalent internal
post-call checkpoint in this observer, so its 256-fault contrast cannot receive
the same interval localization from this data.

Counters are process-wide, and checkpoint intervals include observer work and
potential activity on other threads. A residency snapshot neither identifies a
physical page nor proves that a later access cannot fault. These observations do
not uniquely explain the original formal timing gap. The full campaign's
[zero-fault drift counterexamples](complete-linux-diagnostic-2026-10-24.md)
also remain. No global malloc setting, temporary heap anchor or formal workload
change follows from this analysis.

## Executable reuse contract

Repeated callers can retain their actual output buffers with `ShardSlot<T>` and
reuse verification scratch with `VerifyWorkspace`. Borrowed shard views can stay
in bounded inline metadata; marking a slot missing preserves its owned buffer.
`Option<Vec<_>>` still retains its existing allocation/publication contract.
Retaining buffers is an explicit caller ownership choice, not a replacement
benchmark workload or a hidden codec cache.

The new [resource regression](../tests/recovery_buffer_reuse.rs) runs both GF8
and tower GF16, 4+2 and 10+4, and 1/65/65,536-element shards. After warming both
erasure patterns, eight cycles of encode, mixed data/parity recovery, data-only
recovery and verification issue zero allocation/reallocation requests on the
calling thread. Recovered bytes, presence flags and original buffer addresses
are checked. The contrasting owned-clone workflow issues at least `k+m+3`
requests, including caller clones, the outer collection and two missing outputs.
It is explicitly not codec-only accounting. The test uses serial APIs and
bounded metadata for these shapes; it does not assert the same budget for every
shard count, parallel policy or first-use backend initialization.

```sh
cargo test --locked --release --features simd-accel --test recovery_buffer_reuse
```

The new resource test passes on ARM64 with stable Rust in debug SIMD, release
SIMD and release default-feature configurations. Thirty existing Classic
recovery/workspace compatibility tests and targeted strict Clippy also pass.
These local results are not positive execution evidence for another architecture.

This is a resource contract and compatibility regression, not a wall-clock
speedup claim. No production implementation is changed by this follow-through.
The complete GFNI component comparison still retains its original controller
failure, one distribution refusal, eleven repeat refusals and unqualified
comparison. Final frozen-source full performance acceptance and strict positive
release preflight remain outstanding; 9.1.0 is not approved for publication.
