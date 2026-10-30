# GF16 formal-derivative batching screen

This is an isolated prototype, not production code. The existing formal
derivative updates a destination lane once per XOR source. For a power-of-two
lane count, its schedule can be regrouped by destination: for each zero bit
of `dst`, the source is `dst | bit`. Processing destinations in ascending order
keeps every higher source lane unchanged until it is consumed. A separate
scalar model compared both schedules on 1,800 deterministic random vectors
for lane counts 1 through 256.

The [prototype](prototype.patch) tests this grouping only for little-endian
x86-64 `std` with AVX2, at most 32 lanes and at least 16 elements. It checks
actual work-buffer bounds, borrows disjoint lane views and combines up to five
sources in one vector pass. Other configurations use the original schedule.
Native AVX2 tests compared the full work buffer with the original schedule for
power-of-two and non-power-of-two counts, lengths around vector boundaries,
the 32/33/64-lane dispatch boundary, and an excessive requested lane count.
The `no_std` path, all-features library and integration suite, 200 old/new
cross-decoding configurations and 200 published-to-candidate non-fast cases
passed. AArch64 GF16 tests and x86 `no_std` compilation passed independently.
All 96 benchmark functional cases passed on native x86.

## Performance screen

The Linux x86-64 screen used an AMD EPYC 9V45 virtual machine, Rust stable
1.99.1, identical locks and benchmark harnesses, `simd-accel` and four Rayon
workers. A was the production computational source at `5f0acae`; B was A plus
the prototype. The user requested that drift not stop the run. The shared lock
and an exclusive guest CPU partition covering CPUs 8–15 were held, and the
controller reported no competing heavy process in its periodic checks. The
lock and partition were released afterward. Physical-host isolation was not
verified.

The [initial screen](initial.json) covered 96 cases in each of A1/B1/B2/A2,
ten samples per case, 0.2 seconds warmup and 0.6 seconds requested measurement.
Its potential 1 MiB regressions were followed by a [focused repeat](focused.json)
of two cases, with 30 samples, one second warmup and eight seconds requested
measurement per case. Those two cases showed −2.25% and −3.13% respectively
on the initial prototype. The bounds checks were then added and the complete
96-case screen was repeated; [final.json](final.json) contains every final
estimate and raw sample. The focused repeat does not measure the final version.

For the final screen, average A1/A2 and B1/B2 Criterion mean estimates per
case, then compute `(prototype / baseline - 1) * 100`. Each row reports the
median and range over eight cases: 10+4 and 16+8, each with one through four
missing shards. Negative values indicate lower elapsed time.

| GF16 recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 64 KiB | −0.24% | −7.17% to +8.55% |
| Retained slots | 1 MiB | −2.83% | −5.28% to +5.51% |
| Generic Option | 64 KiB | −1.34% | −5.02% to −0.40% |
| Generic Option | 1 MiB | −3.84% | −8.05% to +5.14% |
| Optimized Option | 64 KiB | −2.62% | −8.50% to +2.04% |
| Optimized Option | 1 MiB | −3.60% | −8.55% to +3.25% |

GF8 control-group medians ranged from −0.29% to +3.74%. The view helper's
observed native x86 entry reserved 3,288 stack bytes; this is one generated-code
observation, not a complete stack bound. The short final screen has mixed
individual outcomes, and the extra view setup and stack use need consideration.
It does not establish a sufficiently consistent gain to justify adding this
specialization to production. This is not a formal release qualification or proof
that every machine has the same outcome.

To reproduce the source, extract `5f0acae` to a clean directory and apply
`prototype.patch` with `git apply`. Run the exact native AVX2 tests under
`core::leopard_gf16::decode::derivative_batch_prototype_tests`; confirm that
both tests run and that the AVX2 execution marker is printed. The fallback test
also exercises a requested count larger than the available lanes.
