# GF16 duplicate padding clear evaluation

GF16 chunk preparation clears `work_size..n`; the following IFFT also initializes
its input suffix, which includes this padding. Two independent candidates were
reviewed against baseline `7805dc7`:

* `input-padding.patch` removes only the preparation-stage padding clear and
  leaves the IFFT unchanged.
* `ifft-padding.patch` retains complete input initialization and removes the
  IFFT's redundant suffix clears. It depends on the existing single caller's
  initialization contract. Neither candidate is production code.

The patches add a test that fills every work lane with nonzero data, runs the
chunk pipeline and compares every lane with a clean-workspace execution. It
covers six data/parity layouts, 64 B, 192 B and 64 KiB chunks, missing data and
parity, including an IFFT smaller than four lanes and non-power-of-two shard
counts. Native x86 and AArch64 tests passed. The input-padding candidate also
passed the all-features codec suite, 284 no_std library tests, 200 std old/new
cross-decoding configurations, and 126 no_std checks each on native AArch64
and WASM. The IFFT variant passed the 126-check native and WASM probes and the
x86 GF16-focused tests; it did not receive the complete release matrix.

At the source level the input-padding candidate removes `(n - work_size)`
whole-lane clears per chunk; for 1 MiB shards that is 2 MiB of clear requests
for 10+4 and 8 MiB for 16+8. An AArch64 optimized build of the chunk function
had seven `bzero` call sites versus eight in the baseline. These are work-count
observations, not measurements of memory-controller traffic or throughput.

## Performance

Same-path baseline and candidate release binaries used locked dependencies
and Rust stable on an x86-64 Linux VM. Guest CPUs were reserved under a shared
lock and no competing heavy process was observed; physical-host isolation was
not verified. A1/B1/B2/A2 compares the average of the two Criterion means per
side. Initial screens used ten samples, 0.2 s warmup and 0.6 s requested
measurement; follow-ups used 20 samples, 0.5 s warmup and 2 s requested
measurement. Negative percentages mean lower latency. Drift was recorded but
did not gate the runs.

| Candidate / control | Workload | Result |
| --- | --- | --- |
| [Input clear, GFNI recovery](../benchmarks/gf16-padding-clear/2026-11-01/results-recovery-gfni-focused.json) | 64 KiB and 1 MiB, selected Option and retained-slot cases | −5.09% to +1.09% across 12 cases |
| [Input clear, GFNI encode first run](../benchmarks/gf16-padding-clear/2026-11-01/results-encode-gfni.json) | 10+4 and 16+8, 64 KiB and 1 MiB | −2.04% to +3.53% |
| [Input clear, GFNI encode repeat](../benchmarks/gf16-padding-clear/2026-11-01/results-encode-gfni-repeat.json) | Same four cases | −0.28% to +1.63% |
| [Input clear, no_std follow-up](../benchmarks/gf16-padding-clear/2026-11-01/results-nostd-focused.json) | 1 MiB, 10+4 and 16+8, one or four missing | median +11.47%, range +5.05% to +13.31% |
| [IFFT clear, no_std follow-up](../benchmarks/gf16-padding-clear/2026-11-01/results-ifft-nostd-focused.json) | Same four cases | median +11.07%, range +7.51% to +13.78% |

The no_std probe uses a std harness with the codec's std feature disabled and
checks the reconstructed bytes before and after timing. GFNI recovery uses
four Rayon workers and the existing retained-slot/optimized-Option probe;
GFNI encode uses a single job. The JSON records each stage's raw samples,
estimates and A1-to-A2 baseline change.

Neither variant is promoted. The clear count drops but the measured no_std
large-shard latency increases materially. The mechanism was not isolated; the
results do not establish whether compiler decisions, layout or another factor
caused the regression. No general throughput or release claim follows from
these screens.
