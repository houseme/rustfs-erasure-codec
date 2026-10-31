# GF16 no_std AVX2 butterflies

Little-endian x86-64 `no_std` builds whose compilation target explicitly
enables AVX2 now reuse the existing AVX2 FFT/IFFT butterfly for equal-length
lanes of at least 64 `u16` elements (128 bytes). Shorter lanes keep the scalar
path. The compile-time target feature guarantees AVX2; disjoint mutable slices
and equal lengths are checked before the unsafe kernel call. The modulus
sentinel keeps its existing XOR-only path. This also accelerates GF16 encoding,
which uses the same butterflies. Public APIs, shard formats and publication
semantics are unchanged; std and other targets retain their previous dispatch.

Implementation: `bc881ce`; baseline: `4bd443a`, which already contains the
separate no_std AVX2 input/output fusion. The measurements below are incremental
to that earlier optimization.

## Performance and tradeoff

Same-path baseline and candidate release binaries used locked dependencies,
Rust stable and `RUSTFLAGS="-C target-feature=+avx2"` on an x86-64 Linux VM.
Both [recovery](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs)
and [encode](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/nostd-encode-probe.rs)
use std Criterion harnesses with the codec built without default features.
Recovery covers 10+4 and 16+8, one and four missing retained slots, and checks
all recovered bytes. Encode checks parity before and after timing. Cargo feature
trees were checked to ensure the codec's std feature was disabled.

Guest CPUs were partitioned under a shared lock; no competing heavy process was
observed and the reservation was restored. Physical-host isolation was not
verified. A1/B1/B2/A2 compares the average of two Criterion means per side;
negative percentages mean lower latency. Drift was recorded but did not gate
the runs. The selected recovery and focused encode runs used 20 samples,
0.5 s warmup and 2 s requested measurement per case.

| Operation | Shard size | Median latency change | Range |
| --- | ---: | ---: | ---: |
| [Recovery](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-selected-recovery.json) | 64 B | +1.93% | +1.14% to +2.47%, four cases |
| Recovery | 4 KiB | −10.20% | −15.23% to −6.92%, four cases |
| Recovery | 64 KiB | −51.59% | −57.23% to −45.73%, four cases |
| Recovery | 1 MiB | −62.16% | −69.64% to −54.58%, four cases |
| [Encode](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-selected-encode-focused.json) | 64 B | +1.75% | +1.41% to +2.09%, two cases |
| Encode | 1 MiB | −53.28% | −56.02% to −50.55%, two cases |

The 64 B regression is a measured tradeoff of the dispatch; it is not a
speedup. For medium and large blocks the gains are substantial on the tested
host. Results do not guarantee the same tradeoff on every AVX2 CPU or workload.

Threshold screens compared 512 B, 256 B, 128 B and 64 B. Lowering 512 B to
256 B improved 256–448 B encoding by approximately 29%–46%
([samples](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-threshold-128-vs-256-encode.json)).
Lowering 256 B to 128 B improved 128–192 B encoding by approximately 5%–29%
([samples](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-threshold-64-vs-128-encode.json)).
Lowering 128 B to 64 B slowed 64 B encoding by approximately 16%–24%
([samples](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-threshold-32-vs-64-encode.json)).
These are separate pairwise experiments, not a proof of a universal optimum.

The [ordinary x86 no_std control](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-control-default.json)
without target AVX2 ranged from −0.55% to +0.42% across 12 recovery cases.
The [std GFNI control](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-control-std.json)
had an 8% encode outlier in the initial screen; the
[longer follow-up](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/results-control-std-focused.json)
ranged from −1.65% to +0.33% across the six retested encode and warm-recovery
cases. Baseline and candidate `.text` contents matched for these ordinary no_std
and std benchmark binaries, including a std build with global `+avx2`; this is
not a claim that entire binaries match.
The [allocation probe](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/allocations.json)
showed unchanged allocation calls and requested bytes for the tested 8 KiB,
64 KiB and 1 MiB Option and retained-slot recoveries. Existing allocations remain.

## Correctness and compatibility

Native x86 `no_std +avx2` passed 286 library tests without optional features and
293 with `simd-accel`; strict Clippy also passed. Dedicated tests compare both
butterfly directions with scalar arithmetic for every multiplier, and exercise
non-aligned slices, tails, the modulus sentinel and lengths around the cutoff
in debug and release. The old/new no_std probe passed 126 checks on native x86
with `+avx2`; the pinned [boundary probe](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/boundary-compat.rs)
passed 228 parity and bidirectional recovery checks from 64 to 65,600 bytes.
The latter is reproducible on an AVX2-capable x86-64 host:

```bash
RUSTUP_TOOLCHAIN=stable RUSTFLAGS="-C target-feature=+avx2" cargo run --release --locked \
  --manifest-path benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/Cargo.toml
```

AArch64 all-features codec and no_std suites, the native AArch64 and WASM
126-check probes, and the std 200-configuration bidirectional release probe
passed. The all-features workspace suite excluding the fuzz package passed,
including 370 xtask tests; all-target all-feature Clippy and formatting passed.
The x86 host had neither `cargo-fuzz` nor nightly installed, so fuzz was not run.

After commit `e475fab`, `cargo package --no-verify` produced a 73-file crate
archive whose recorded VCS revision was `e475fab`; all 66 packaged Rust source
files matched the checkout. The extracted archive passed 126 no_std old/new
checks on native AArch64 and WASM, and both the 126-check and 228-check boundary
probes on native x86 with `+avx2`. No crate was published.
