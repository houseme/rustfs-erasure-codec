# Sparse GF16 error locator for no_std recovery

For `no_std` Leopard GF16 recovery, the erasure locator is normally built with
a 65,536-element Walsh–Hadamard convolution. The transform cost dominates
small-shard recovery even when only a few shards are missing. The plan builder
now uses direct sparse evaluation when its work size is at most 256 and the
number of erased plus padded parity positions is at most 16. The original full
transform remains the fallback; std builds retain their existing plan path.
Public APIs, target publication behavior and shard bytes are unchanged.

## Calculation and bounds

Let `E` be the erased data positions, erased parity positions and implicit
padding positions in the decoder's work layout. The log-table convolution at a
used index `i` is `sum(log_lut[i XOR e] for e in E) mod 65535`. The Walsh
transform is self-inverse up to 65,536, which is one modulo 65,535, so this
produces the same residue as the existing transform. At most 256 × 16 table
lookups are performed; the sum is bounded by 16 × 65,535 and fits `u32`.
Only indices below `work_size` are read by the recovery path; the remaining
private plan array entries are zero. The original complete locator function is
unchanged and remains an independent test reference.

The full transform can represent modular zero as `65535`, while direct
reduction returns `0`. Both represent multiplication by one in the GF16
scaling paths. Tests explicitly exercise the difference for both a missing
output and a present input and verify the recovered bytes. Non-power-of-two
parity padding, the 256/257 work-size boundary and the 16/17 term boundary are
also covered.

## Performance

The [recovery harness](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs)
compares same-path baseline and candidate release builds with Rust stable,
locked dependencies, `--no-default-features` and compile-time `+avx2`. The
codec feature tree was checked to confirm that std was not enabled. It uses 10+4 and 16+8 with one or four missing retained slots, checking recovered
bytes before and after timing. Linux x86-64 VM phases A1/B1/B2/A2 each used
20 samples, 0.5-second warmup and two seconds requested measurement per case.
An exclusive guest CPU partition and shared lease were restored after each run;
no competing intensive process was observed. Physical-host isolation was not
verified, and drift was recorded without gating the run. Changes below compare
the average of candidate means with the average of baseline means.

| Shard size | Latency change across four recovery cases | Example baseline → candidate |
| ---: | ---: | ---: |
| 64 B | −98.15% to −97.40% | 10+4, one missing: 257.40 µs → 4.76 µs |
| 4 KiB | −94.88% to −90.10% | — |
| 1 MiB | −7.01% to −2.92% | — |

The [boundary harness](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/threshold-probe.rs)
checks work sizes 256 and 257, term counts 2, 16 and 17, and 16 missing shards.
At 256 work lanes and 16 terms, 64 B recovery improved 83.15%; with 17 terms
or 257 lanes the full-transform fallback remained in use. A longer 40-sample,
one-second-warmup, five-second follow-up of four fallback cases ranged from
−0.42% to +0.07%; one baseline drift was +2.17%. The work-size-48, 16-term
initial screen showed large improvements but baseline drift of +7.94% and
+10.52%; these observations are retained rather than treated as isolated
proof. [All x86 estimates](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/x86-estimates.json)
include A1/B1/B2/A2 and allow drift to be recalculated.

An initial x86 encoding control had a +3.41% 10+4, 64 B result. In a longer
40-sample follow-up, the two 64 B encoding cases were −1.55% and +0.91%; their
baseline drift was +2.35% and −2.58%. Four identified encoder hot functions
had identical machine-code bytes between the initial baseline and candidate.
A separate std + SIMD benchmark's entire `.text` section was identical between
baseline and candidate. The final source after test-only adjustments had
byte-identical encoding and recovery benchmark `.text` sections to the measured
final binaries, for both baseline and candidate.

A shorter, non-isolated native AArch64 screen used 10 samples, 0.25-second
warmup and 0.75 seconds requested measurement. Its four 64 B cases improved
93.73%–95.27%; the 1 MiB cases ranged from −2.82% to −0.89%. This is an
exploratory cross-architecture result, not an isolated performance claim.
[Raw AArch64 estimates](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/aarch64-screen.json)
retain all four phases and baseline drift.

## Correctness and compatibility

The sparse locator is compared against the unchanged full transform modulo
65,535 for varied data/parity shapes and erasures, including the fallback
thresholds. Actual data and parity reconstruction is checked for both zero
representations. A pinned [cross-version probe](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/)
passed 1,708 erasure patterns in both decode directions against published
9.0.0: 1,470 exhaustively cover one through four missing shards for 10+4 at
64 B; the others include padded parity, threshold boundaries, parity-only
loss and larger shards. It also checks selective `ShardSlot` recovery,
`(Vec<u8>, bool)` data-only recovery and a rejected target-length case without
modifying other slots. The probe passed on native AArch64, WASM, Linux x86 with
AVX2 and Linux x86 with SSSE3 but AVX disabled. Both codec dependencies were
checked without their std features.

Linux x86 no_std passed 292 AVX2 debug library tests, 299 AVX2 release tests
with `simd-accel`, and 288 debug and release tests with SSSE3 but no AVX; default
x86 no_std passed 288 debug tests. AArch64 no_std passed 289 library tests;
no_std checks also passed for `aarch64-unknown-none` and `wasm32-unknown-unknown`.
The all-features workspace suite excluding the fuzz package and workspace
all-target/all-feature Clippy passed. The standard release compatibility probe
passed 200 configurations in both directions, 200 historical non-fast cases,
36 large GF8 cases, 12 6+2 cases and 32 small GF8/GF16 cases. Diagnostic source
bindings were regenerated for the modified source and their tests passed.

A crate archive built from commit `28aad06` recorded that revision with a clean
package state; all 66 packaged Rust source files matched the checkout. The
extracted crate passed the same 1,708-pattern no_std probe on native AArch64,
WASM, Linux x86 with AVX2, and Linux x86 with SSSE3 but AVX disabled, including
the generic target checks. The package was not published.

A dedicated [cross-version fuzz target](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/fuzz/)
builds both codec versions without std and exercises valid small GF16 shapes,
parity, and erasure patterns. On native AArch64 with the installed nightly and
AddressSanitizer, an initial 31-second run completed 38,718 iterations without
a crash; a 16-second rerun of the retained target with the accumulated corpus
completed 18,602 iterations without a crash. These are bounded fuzz runs, not
a proof of exhaustive correctness.

The native compatibility probe can be rerun with:

```sh
CARGO_TARGET_DIR=target/sparse-compat RUSTUP_TOOLCHAIN=stable \
  cargo run --offline --locked --release \
  --manifest-path benchmarks/gf16-nostd-sparse-locator/2026-11-01/Cargo.toml
```

For WASM, build that manifest with `--lib --target wasm32-unknown-unknown`,
generate Node bindings with `wasm-bindgen 0.2.129`, and call `validate()`;
it must return 1,708. A fresh short fuzz run can be reproduced with:

```sh
mkdir -p /tmp/rse-sparse-corpus /tmp/rse-sparse-artifacts
printf '\011\003\000\003\000\001\002\003' > /tmp/rse-sparse-corpus/seed
CARGO_NET_OFFLINE=true RUSTUP_TOOLCHAIN=nightly cargo fuzz run \
  --fuzz-dir benchmarks/gf16-nostd-sparse-locator/2026-11-01/fuzz \
  --target-dir /tmp/rse-sparse-fuzz-target sparse_cross /tmp/rse-sparse-corpus \
  -- -max_total_time=30 -artifact_prefix=/tmp/rse-sparse-artifacts/
```

Both probes depend on the old and current codecs with default features disabled.
