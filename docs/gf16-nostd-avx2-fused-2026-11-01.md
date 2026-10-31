# GF16 no_std AVX2 fused conversion

On little-endian x86-64 `no_std` builds whose compilation target explicitly
enables AVX2, GF16 decode now reuses the existing AVX2 fused conversion and
fixed-multiplier kernels for chunks of at least 512 bytes. Shorter chunks keep
the scalar fused path. The target feature is a compile-time guarantee; this
path does not perform runtime CPU detection. `std` builds, targets without
AVX2, AArch64 and WASM retain their previous dispatch. The decode workspace,
output publication, public API and encoded bytes are unchanged.

The nibble tables use the existing basis expansion on this target, and the
AVX2 table setup is kept out of the short-block path. Both directions operate
on the same complete, disjoint 64-byte blocks checked by the existing wrapper.
Implementation: `a7c803b`; baseline: `7805dc7`.

## Performance

Same-path baseline and candidate release binaries used locked dependencies,
Rust stable and `RUSTFLAGS="-C target-feature=+avx2"` on an x86-64 Linux VM.
The [probe](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs) is a
std Criterion harness using a codec built with `--no-default-features`; Cargo
feature trees were checked to confirm `std` was not enabled for the codec. It
measures serial 10+4 and 16+8 GF16 recovery with one or four missing retained
slots and verifies every recovered byte before and after timing.

Guest CPUs were partitioned under a shared lock; monitoring saw no competing
heavy process and the reservation was restored. Physical-host isolation was
not verified. A1/B1/B2/A2 compares the average of the two Criterion means on
each side; negative values mean lower latency. Drift was recorded but did not
gate these runs.

| Run | Shard size | Median latency change | Range across four cases |
| --- | ---: | ---: | ---: |
| [Small follow-up](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-outlined-64.json) | 64 B | −0.17% | −0.91% to −0.08% |
| [Small screen](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-outlined-small.json) | 4 KiB | −6.61% | −7.98% to −5.28% |
| [Large follow-up](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-outlined-large.json) | 64 KiB | −22.16% | −23.08% to −21.27% |
| Large follow-up | 1 MiB | −24.87% | −29.30% to −21.84% |

The follow-ups used 20 samples, 0.5 s warmup and 2 s requested measurement per
case. The [threshold screen](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-boundary.json)
covered 448, 512, 576 and 1,024 B with ten samples, 0.2 s warmup and 0.6 s
requested measurement: 448 B remained near baseline while the three lengths
at or above 512 B improved in all four shapes. A separate
[256-vs-512 B experiment](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-threshold-256-vs-512.json)
did not show a consistent advantage for the lower threshold. These results do
not establish an optimal threshold for every CPU or workload.

An initial inline-setup prototype had a repeatable 3%–4% slowdown in two 64 B
cases ([samples](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-small-focused.json));
that structure was replaced by the measured out-of-line setup. The final
candidate's `+avx2` benchmark `.text` bytes matched the measured outlined
candidate. The ordinary x86 no_std and std benchmark `.text` bytes also matched
their respective baselines. The [ordinary no_std control](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-control-default.json)
and [std GFNI control](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/results-control-std.json)
retain their complete samples. Code-section comparisons apply only to these
builds and do not claim complete binary identity.

The [allocation probe](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/allocations.json)
found identical allocation calls and requested bytes before and after for
8 KiB, 64 KiB and 1 MiB, using Option and retained-slot outputs. It counts the
whole reconstruction call; existing output and plan allocations remain.

## Correctness and compatibility

Native x86 `no_std +avx2` passed 291 library tests with `simd-accel` and strict
Clippy. The AVX2 kernels are checked against independent scalar operations for
all 65,536 field values at selected multipliers and every multiplier at
representative values; wrapper tests cover 448, 512 and 576 B, unaligned
buffers, the identity sentinel and chunk boundaries. Native `no_std +avx2`
old/new compatibility passed the existing 126 checks plus
[114 threshold and cross-decoding checks](../benchmarks/gf16-nostd-avx2-fused/2026-11-01/boundary-compat.rs).
On an AVX2-capable x86-64 host, the latter can be reproduced with its pinned
manifest and lockfile:

```bash
RUSTUP_TOOLCHAIN=stable RUSTFLAGS="-C target-feature=+avx2" cargo run --release --locked \
  --manifest-path benchmarks/gf16-nostd-avx2-fused/2026-11-01/Cargo.toml
```

The ordinary x86 no_std fallback passed GF16 tests with and without
`simd-accel`; the std x86 GF16 suite passed with one pre-existing ignored
test. AArch64 codec all-features and no_std suites, native and WASM 126-check
old/new probes, and the std 200-configuration bidirectional release probe
passed. The all-features workspace suite, excluding the fuzz package, passed
including 370 xtask tests, as did all-target all-feature Clippy and formatting.
The x86 host did not have `cargo-fuzz` or a nightly toolchain; fuzz was not run.

After commit `9bc151b`, `cargo package --no-verify` produced a 73-file crate
archive whose recorded VCS revision was `9bc151b`; all 66 packaged Rust source
files matched the checkout. The extracted archive passed the 126-check no_std
old/new probe on native AArch64 and WASM, and both the 126-check and the
114-check boundary probe on native x86 with `+avx2`. No crate was published.
