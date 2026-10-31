# GF16 basis-log follow-up

This follow-up evaluates the basis-log idea from the [paired-butterfly
record](gf16-radix-pair-evaluation-2026-10-31.md). No candidate in this
document is active production code. The baseline was `fc6795c`.

The first prototype stores 16 precomputed basis logarithms in the internal GF16
table state, then uses them when building GFNI affine matrices. It passed native
GFNI matrix, full-field-value and tail tests, including all non-sentinel
multipliers and 16 basis vectors. An initial isolated benchmark had to be
stopped when another CPU-intensive task appeared; those incomplete samples were
not used. A later full screen and a longer 64 KiB follow-up completed.

The second [constant prototype](../benchmarks/gf16-basis-log-followup/2026-10-31/constant-prototype.patch)
stores the same 16 fixed-field logarithms in read-only program data instead of
changing the table-state structure. A test checks them against the generated
field table. Native x86 GFNI matrix, full-field-value, tail and constant tests
passed. On AArch64, an independent scalar experiment compared all 65,536
multipliers and all 16 basis-vector products with the existing calculation.
This establishes arithmetic equivalence for that experiment, not a performance
claim or a full old/new release-compatibility run.

## Reproducible screens

Completed x86 runs used an AMD EPYC 9V45 Linux VM, Rust stable 1.99.1, four
Rayon workers and a shared lock with guest-exclusive CPUs 8–15. Monitoring
reported no competing heavy process during the retained screens; the partition
and lock were released after each run. Physical-host isolation was not
established. Each A1/B1/B2/A2 phase used ten samples, 0.2 seconds warmup and
0.6 seconds requested measurement, except the longer follow-ups with 20
samples, 0.5 seconds warmup and two seconds requested measurement. For each
case, the two Criterion mean estimates on each side are averaged before
calculating the percentage change. Each table row reports the median and range
over four cases: 10+4 and 16+8, with one or four missing shards. Negative
values mean lower latency.

To reduce source-path and build-layout confounding, the later runs compiled the
baseline and candidate sequentially from the **same source directory and same
Cargo target directory**, saving each binary separately. Their benchmark source
and runtime working directory were identical. All 32 relevant recovery
preflights passed on each side; the field-only confirmation used 16 cases.

| Candidate and backend | Entrypoint | Shard size | Median | Range |
| --- | --- | ---: | ---: | ---: |
| [Table field, GFNI, same path](../benchmarks/gf16-basis-log-followup/2026-10-31/field-same-path.json) | retained slots | 64 B | −2.69% | −3.10% to −2.43% |
| Same | retained slots | 64 KiB | +18.35% | +16.51% to +19.09% |
| Same | optimized Option | 64 KiB | +18.81% | +18.22% to +19.09% |
| [Constant, GFNI, longer follow-up](../benchmarks/gf16-basis-log-followup/2026-10-31/constant-gfni-focused.json) | retained slots | 64 B | −2.30% | −2.75% to −2.00% |
| Same | optimized Option | 64 B | −1.39% | −1.76% to −0.42% |
| Same | retained slots | 2 KiB | −0.23% | −0.88% to +0.67% |
| Same | optimized Option | 2 KiB | −0.10% | −0.65% to +0.39% |
| [Constant, AVX2 control](../benchmarks/gf16-basis-log-followup/2026-10-31/constant-avx2-control.json) | retained slots | 64 B | −2.11% | −3.08% to −1.97% |

The [initial same-path constant screen](../benchmarks/gf16-basis-log-followup/2026-10-31/constant-gfni.json)
showed 64 KiB medians of −0.32% and −0.25%, and 1 MiB medians of −1.28% and
−0.11%, for retained slots and optimized Option respectively. The AVX2 control
does not select the GFNI kernel, yet its 64 B retained-slot median also changed
by about 2%. That control leaves code-layout or measurement effects as possible
explanations for the similarly small GFNI result; it does not prove a GFNI
kernel speedup.

For the table-field prototype, the [separate-path screen](../benchmarks/gf16-basis-log-followup/2026-10-31/field-separate-path.json)
and [longer 64 KiB follow-up](../benchmarks/gf16-basis-log-followup/2026-10-31/field-separate-focused.json)
also showed roughly 18%–19% higher GFNI latency. The same-path screen repeated
the regression, so differing source-directory paths alone do not explain it.
The underlying cause remains unproven.

A local Rust 1.99.1 x86 cross-build from the same source and target directories
found that instruction counts differed for 11 matched GF16 functions in the
table-field version and three in the constant version. Equal counts do not
prove identical instructions, and this static comparison is not a throughput
measurement. At the user's request, baseline drift did not stop the screens.

The table-field version is rejected because of the large-shard regression. The
constant version is not promoted: its small measured benefit is comparable to
its AVX2 control and would add fixed constants that must remain aligned with
the field definition. Neither result establishes an improvement across CPUs or
workloads.
