# GF16 butterfly coefficient work-count probe

This is a test-only diagnostic against `a2d6bba`, not a timing result or a
production change. It examines whether a proposed `log_m == 0` identity
butterfly fast path would apply to the recovery shapes in the recent benchmark.

The [instrumentation](instrumentation.patch) counts calls to the shared
forward and inverse radix-2 butterfly helpers by zero, modulus sentinel, or
other multiplier. Atomic counters include work on Rayon threads. A single
selected test resets them immediately after encoding and before each recovery,
uses four Rayon workers, and checks every recovered byte. It also scans the
complete generated `fft_skew` table. Instrumentation changes the test binary;
counts describe work performed, not latency or the CPU cost of each operation.

On AArch64 macOS with stable Rust, the full skew table contained **zero entries
equal to zero**. All eight tested GF16 recovery cases likewise had zero
butterfly calls with `log_m == 0`. The modulus sentinel was common: for example,
10+4 with a 1 MiB shard recorded 480 sentinel and 544 other calls; 16+8 recorded
992 sentinel and 1,248 other calls. The [complete observations](observations.txt)
retain both shard sizes and missing counts. These results concern butterfly
operations, not input/output scaling coefficients or arbitrary synthetic calls.

For these benchmark cases, a zero-multiplier butterfly branch would add code
without executing. The existing modulus shortcut has different semantics and
must not be conflated with multiplication by `g^0`. Do not add the proposed
zero fast path based solely on this hypothesis; other call sites and workloads
would need their own evidence.

To repeat from a clean repository with dependencies available offline:

```sh
repo=$(pwd)
scratch=$(mktemp -d)
git archive a2d6bba | tar -x -C "$scratch"
git -C "$scratch" apply "$repo/benchmarks/gf16-butterfly-coefficients/2026-10-30/instrumentation.patch"
RUSTUP_TOOLCHAIN=stable RAYON_NUM_THREADS=4 CARGO_TARGET_DIR="$scratch/target" \
  cargo test --offline --locked --release --manifest-path "$scratch/Cargo.toml" \
  -p rustfs-erasure-codec --lib \
  core::leopard_gf16::ops::coefficient_work_probe::observe_butterfly_multiplier_categories \
  -- --exact --nocapture --test-threads=1
```

Confirm that one test ran. The selected benchmark's parallel decoder used four
workers; a different scheduler configuration can change work distribution.
