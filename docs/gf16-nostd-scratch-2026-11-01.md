# GF16 no_std decode scratch

GF16 decoding previously allocated one `u16` scratch lane per workspace even
when the library was built without `std`. The no_std input/output conversions
already use fused scalar or NEON kernels, and the decode FFT functions do not
use their scratch argument. The no_std workspace now supplies an empty scratch
buffer. The std fallback, workspace reuse and retention limit are unchanged;
public APIs and encoded bytes are unchanged.

## Allocation evidence

The [probe](../benchmarks/gf16-nostd-scratch/2026-11-01/cold-allocation-probe.rs)
uses a std test harness with the codec built with `--no-default-features`, a
10+4 GF16 codec, four missing shards and a fresh thread per measurement. It
counts allocations during reconstruction, including allocations unrelated to
scratch. To reproduce, copy it to `tests/cold_scratch_probe.rs` in an isolated
checkout and run `cargo test --release -p rustfs-erasure-codec
--no-default-features --test cold_scratch_probe -- --nocapture --test-threads=1`.
Native AArch64 and x86 measurements gave the same counts. For each
case the candidate removed one allocation:

| Shard size | Option calls before → after | Slot calls before → after | Bytes saved per call |
| --- | ---: | ---: | ---: |
| 8 KiB | 14 → 13 | 10 → 9 | 8,192 |
| 64 KiB | 14 → 13 | 10 → 9 | 65,536 |
| 1 MiB | 14 → 13 | 10 → 9 | 65,536 |

The reduction is bounded by the decode chunk size. These numbers do not claim
zero allocations or a measured no_std latency improvement. `Option` output
allocations and other reconstruction allocations remain.

## Standard-build performance guard

Changes to std workspace allocation were considered separately and rejected.
One [lazy-scratch prototype](../benchmarks/gf16-nostd-scratch/2026-11-01/lazy-std-gfni.json)
materially slowed some GFNI recovery workloads. A different conditional-scratch
prototype had an approximately 22–26% GFNI encode slowdown in the [longer
follow-up](../benchmarks/gf16-nostd-scratch/2026-11-01/conditional-std-encode-gfni-focused.json).
The cause of that encoding effect was not established. Neither std prototype
is included in this change.

The no_std-only candidate was also screened with std enabled, using baseline and
candidate built at the same source and target paths on an x86-64 Linux VM,
with the GFNI backend selected and four Rayon workers. A shared lock and guest
CPUs 8–15 were reserved; monitoring saw no competing heavy process and the
reservation was restored afterward. Physical-host isolation was not verified.
A1/B1/B2/A2 uses the average of the two Criterion means on each side; negative
percentages mean lower latency. Baseline drift did not gate the screen.

* [Recovery](../benchmarks/gf16-nostd-scratch/2026-11-01/std-recovery-gfni.json):
  16 cases across 10+4 and 16+8, 64 KiB and 1 MiB shards, one and four
  missing shards, and Option and retained-slot entry points; ten samples,
  0.2 s warmup and 0.6 s requested measurement. Changes ranged from −2.91%
  to +0.55%. The recovery benchmark candidate differs from the final source
  only by a no_std Clippy allowance on the existing `Vec` argument.
* [Encode](../benchmarks/gf16-nostd-scratch/2026-11-01/std-encode-gfni.json):
  four cases across 10+4 and 16+8, 64 KiB and 1 MiB shards; 20 samples,
  0.5 s warmup and 2 s requested measurement. Changes ranged from −0.85% to
  +1.80%; the latter case had −4.25% A1-to-A2 baseline drift.

These measurements guard the tested std workloads; they do not establish a
speedup or guarantee performance on other machines and configurations.

## Compatibility

The no_std GF16 tests exercise an empty conversion scratch on native AArch64
and x86. The pinned 9.0.0/9.1.0 compatibility probe passed 126 old/new checks
on native AArch64 and WASM, with Cargo feature trees checked to ensure neither
codec enabled `std`. The AArch64 no_std library suite passed 281 tests. The standard release
probe passed 200 parity and bidirectional cross-decoding configurations, 200
historical non-fast cases, 36 large GF8 cases and 12 6+2 cases. The
all-features codec tests, formatting and all-target all-feature Clippy checks
also passed on AArch64. The std cache policy and its existing lifecycle tests
remain in place.
