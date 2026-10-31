# GF16 GFNI butterfly

The x86 GF16 forward and inverse butterflies can multiply a 16-bit field value
using four byte-plane GF(2) affine matrices and two AVX2 GFNI instructions.
Matrices are built from the existing GF16 multiplication tables, so the field
polynomial and encoded bytes do not change. The kernel operates on two disjoint
lanes in place, with scalar handling for a final partial vector.

Dispatch requires an existing GF8 backend selection of `rust-gfni-avx2` or
`rust-gfni-avx512`, detected AVX2 and GFNI CPU features, and a lane of at least
4,096 u16 elements (8 KiB). Smaller lanes do not amortize matrix construction.
Other backends, unsupported CPUs, `no_std`, and a GF8 backend that has not yet
been selected retain the existing implementation. In particular, GF16 work
must not initialize the global GF8 backend: doing so could change when a later
GF8 operation reads `RSE_BACKEND_OVERRIDE`. Reading the environment on every
butterfly also caused hundreds of small allocations, so the implementation
only observes an already-selected backend without reading the environment.
This intentionally leaves GF16-only processes on AVX2 until GF8 is selected.

The native GFNI kernel was checked against scalar arithmetic for every
non-sentinel multiplier and each of the 16 field basis vectors, both butterfly
directions, plus every field element for six representative multipliers.
Additional tests cover unaligned views, partial vectors, guard values, and the
size cutoff. Child-process tests verify GF16 encode and recovery leave the GF8
backend uninitialized under scalar, AVX2, and GFNI overrides. The x86
all-features suite and GF16 `no_std` tests passed, as did 200 old/new
cross-decoding configurations and 200 published-to-candidate non-fast cases.
Warm GF16 recovery allocation counts matched the baseline with and without an
already selected GF8 backend, including explicit `auto` and GFNI overrides.

## Performance screen

The screen used an AMD EPYC 9V45 Linux x86-64 VM, Rust stable 1.99.1,
`simd-accel`, identical dependency locks and four Rayon workers. The baseline
was `a0f74a0`; the candidate adds the GFNI butterfly and its selection guard.
Both sides used the same benchmark harness. For this mixed-use screen the
harness called `galois_8::active_backend_id()` before registering cases; all
four stages reported `RustGfniAvx512`. The candidate uses AVX2 GFNI
instructions even when the selected GF8 backend is AVX-512.

A shared lock and exclusive guest CPUs 8–15 were held during the run. Monitoring
reported no competing heavy process; the partition and lock were released.
Physical-host isolation was not established. A1/B1/B2/A2 each ran 16 cases,
with ten samples per case, 0.2 seconds warmup and 0.6 seconds requested
measurement. Per case, the two Criterion mean estimates on each side are
averaged and `(candidate / baseline - 1) * 100` is calculated. Each row is the
median and range of four cases: 10+4 and 16+8, each with one or four missing
shards. Negative values mean lower latency.

| GF16 recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 64 KiB | −15.43% | −16.34% to −14.87% |
| Retained slots | 1 MiB | −9.68% | −11.77% to −5.46% |
| Optimized Option | 64 KiB | −15.63% | −16.38% to −14.91% |
| Optimized Option | 1 MiB | −3.17% | −5.94% to +0.04% |

The [mixed-use samples](../benchmarks/gf16-gfni-butterfly/2026-10-31-linux-x86/mixed.json)
contain all estimates and individual samples. A separate
[screen without explicit GF8 initialization](../benchmarks/gf16-gfni-butterfly/2026-10-31-linux-x86/uninitialized.json)
showed 64 KiB median changes of +0.02% and +0.06%, and 1 MiB median changes
of +0.55% and +0.89%, for retained slots and optimized Option respectively.
These short VM screens are workload- and machine-specific, and do not guarantee
performance on every supported CPU. At the user's request, drift did not stop
the runs. The benchmark's startup initialization is outside the measured
recovery operations; compare each entrypoint with its own baseline.

A subsequent [matrix-construction optimization](gf16-gfni-matrix-transpose-2026-10-31.md)
removed the 8 KiB cutoff described above; the selection and compatibility
constraints remain unchanged.
