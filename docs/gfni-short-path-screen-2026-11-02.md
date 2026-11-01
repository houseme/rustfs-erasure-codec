# GF16 64-byte XOR and short-path investigation

After the bounded GFNI matrix cache, 64 B recovery still spends time in the
butterfly pipeline and its XOR-only sentinel steps. This follow-up screened
several candidate changes against the current code, retaining only changes
whose correctness and performance could be verified.

## Investigated candidates

A counter-only [probe](../benchmarks/gfni-short-path/2026-11-02/identity-count-probe.patch)
on AArch64 observed no `log_m == 0` butterfly in 64 B encodes for 6+2, 10+4,
16+8 or 32+16, or in the tested 10+4 and 16+8 recovery masks. The modulus
sentinel and zero logarithm have different butterfly semantics; they cannot be
interchanged. These counts describe the selected workloads, not all codecs.

Three GFNI-specific candidates were rejected:

- [Explicit short-vector unrolling](../benchmarks/gfni-short-path/2026-11-02/short-unroll.patch)
  changed 64 B encoding by +0.64% and −2.06% for 10+4 and 16+8, with small
  changes at 1 MiB. The screen did not establish enough gain for the added
  control flow.
- [Caching the canonical table pointer](../benchmarks/gfni-short-path/2026-11-02/canonical-naive.patch)
  regressed 64 B encoding by 26.31% and 31.89%. Software CPU-clock sampling
  showed that the compiler outlined the thread-local wrapper in the candidate;
  its added call was observed in the samples. This is a compiler/target-specific
  finding, not a general claim about thread-local storage.
- [Separating cold cache filling](../benchmarks/gfni-short-path/2026-11-02/canonical-cold.patch)
  restored inlining and improved the first screen by about 2.8%–2.9%, but a
  40-sample follow-up changed 64 B encoding by +2.24% and +0.42%. It was not
  retained.

These exploratory binaries were built from the 9f529f5 runtime source. Their
[stage estimates](../benchmarks/gfni-short-path/2026-11-02/estimates.json)
and [software-sampling summaries](../benchmarks/gfni-short-path/2026-11-02/sampling.json)
are kept separately from production code.

## 64-byte XOR path

GF16 butterflies frequently use the sentinel XOR operation. On std x86-64,
exactly 32 `u16` elements can be XORed through fixed-size safe array views,
without the generic byte-view conversion and runtime AVX2 dispatch. All other
lengths and all non-std or non-x86 targets continue through the existing path.
This is an XOR operation only: field multiplication, wire layout, public API,
and recovery publication behavior are unchanged.

The change was measured against runtime source `4d05c36`; the subsequent
`444a23b` commit contained benchmark records only. Each comparison used the
same source path, stable Rust and locked dependencies on a Linux x86-64 VM,
with one worker and a guest CPU partition under a shared lease. The lease was
restored after every A1/B1/B2/A2 run. Physical-host isolation was not verified,
and baseline drift was recorded but did not gate the runs. Changes below
compare the mean of candidate phases with the mean of baseline phases; negative
means lower latency.

The initial GFNI recovery screen used 20 Criterion samples, 0.5 s warmup and
2 s requested measurement. Its three 64 B warm cases improved 8.53%–9.65%.
A 40-sample follow-up with a 1 s warmup and 5 s requested measurement tested
10+4 and 16+8 with four missing shards: 64 B warm recovery improved 9.07% and
9.29%, and cold recovery improved 3.74% and 6.50%. The four 1 MiB recovery
cases ranged from −0.02% to +1.55%. The largest absolute baseline drift in
these eight focused cases was 1.70%.

The initial GFNI encoding screen suggested an 8.75%–11.00% improvement at
64 B, but the 40-sample follow-up measured −1.63% and −2.33%; the initial
encoding gain is therefore not treated as established. The same follow-up's
1 MiB encoding changes were +0.50% and −0.42%. A separate four-shape screen
covering 6+2 through 32+16 ranged from −14.82% to −0.60% at 64 B, −1.83% to
+0.34% at 4 KiB, and −2.22% to +2.48% at 1 MiB. The +2.48% case had +5.14%
baseline drift; the spread across screens limits conclusions about encoding.

With the `rust-avx2` GF8 backend explicitly selected instead of GFNI, a
40-sample follow-up measured 64 B warm recovery improvements of 5.27% and
3.78%; the two 1 MiB warm cases changed −0.60% and +0.23%. Initial AVX2
screens had baseline drift up to +7.77% for 64 B recovery. Backend selection
controls GFNI eligibility, but GF16 dispatch for other operations still follows
CPU capabilities. Results are specific to this VM and do not establish
performance on every x86 processor.

## Validation

The full all-feature workspace suite excluding the fuzz package and
all-target/all-feature Clippy passed on AArch64; 290 no-std library tests
passed. The x86 all-feature library suite passed 462 tests with five ignored.
A direct test covers lengths around 32 `u16` elements with multiple offsets
and unchanged guards. The 1,708-pattern old/new recovery probe passed on x86
with scalar-rust, rust-avx2 and rust-gfni-avx2 selected, on AArch64 with NEON,
and on AArch64 and WASM in no-std mode. All-feature `cargo check` passed for Linux and Windows x86-64 targets; Windows
was not run natively. No physical x86 machine without AVX2 was available.

On the tested x86 binary, the final GFNI recovery, GFNI encoding and AVX2
benchmark `.text` sections matched the measured prototype byte for byte. A
no-std AVX2 benchmark `.text` section matched the baseline. The original
[recovery probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/std-fwht-probe.rs)
and [encoding probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/encode-only-probe.rs)
were used; complete [stage estimates](../benchmarks/gfni-short-path/2026-11-02/estimates.json)
are retained alongside the exploratory patches.

An unpublished crate archive built from commit `c96fdbc` contained 67 Rust
source files matching its clean checkout byte for byte. The extracted codec
passed the 1,708-pattern std probe on x86 with scalar-rust, rust-avx2 and
rust-gfni-avx2 selected for GF8, on AArch64 with rust-neon, and the no-std probe
on AArch64 and WASM.
