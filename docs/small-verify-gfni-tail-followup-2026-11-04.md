# Small Vec verification and GFNI single-byte tail follow-up

The earlier [concrete-Vec workspace verification prototype](../benchmarks/verify-vec-validation/2026-11-02/candidate.patch)
avoids repeated validation for `Vec<u8>` inputs. This follow-up used a broader
[probe](../benchmarks/gfni-tail-followup/2026-11-04/verify-probe.rs) covering
valid and corrupted Classic shards at 64 and 257 bytes and Leopard controls.
Its production runtime matched `b61c1f1` for the files under test. The candidate was not
adopted: under GFNI, Classic 4+2/257 B verification changed +6.81% valid and
+7.26% corrupt; 10+4 changed +4.98% and +7.21%. An independent 256/257 B
follow-up measured 4+2/257 B at +8.28% and +8.26%, and 10+4 at +7.23% and
+9.37%. The four baseline drifts for that repeat ranged from −3.69% to +2.07%.
The same initial AVX2 cases ranged from −1.61% to +0.85%.

The candidate passed the existing concrete workspace contract and partial
resize tests on local ARM64, `no_std` library tests and all-target all-feature
Clippy. The full workspace test did not pass: two `xtask` diagnostic tests bind
generated output to production source and reported changed digests. The
experimental code was not integrated, and those tests were not updated.

## One-byte GFNI tail experiment

CPU-clock sampling of 4+2/257 B GFNI verification placed approximately
19.66%–19.84% of baseline samples in byte-view preparation, 19.66%–19.94%
in the GFNI row kernel, and 11.03%–11.74% in its scalar tail. The
[sampling summary](../benchmarks/gfni-tail-followup/2026-11-04/sampling-summary.json)
records both candidate and baseline shares. Sampling proportions do not prove
the source of the workspace candidate's regression.

The independent [single-byte tail prototype](../benchmarks/gfni-tail-followup/2026-11-04/initial.patch)
accumulates the one remaining byte locally before writing it. A native x86-64
reference test checked all 256 coefficients and 256 input values, including
zero and unit coefficients, a nonzero offset and output guards. The
[probe](../benchmarks/gfni-tail-followup/2026-11-04/tail-probe.rs) also checks
valid and damaged verification and encoded parity before timing.

In the GFNI focused comparison, Classic 10+4/257 B verification changed
−6.91% valid and −6.59% corrupt; baseline drift was −2.25% and −4.45%. A
separate repeat measured −5.76% and −3.24%, with drift −4.87% and −10.54%.
However, 4+2/64 B valid verification, which does not use a one-byte tail,
changed +3.86% and +1.94% in those two comparisons. The GFNI vector kernel's
size was unchanged, while the scalar-tail function grew by 160 bytes and both
functions moved in the binary. Other unchanged-length and Leopard controls also
moved. This evidence does not establish how much of the measured change comes
from the tail algorithm versus compiler or layout effects.

An [outlined single-byte variant](../benchmarks/gfni-tail-followup/2026-11-04/outlined.patch)
also passed the native exhaustive reference test. On ARM64 its workspace
all-feature tests, `no_std` library tests and all-target all-feature Clippy
passed; that architecture does not execute the x86 tail. Its performance run stopped
when monitoring detected another CPU-intensive workload; the guest CPU partition
and shared lease were restored. No performance conclusion is drawn from that
incomplete run. In a later idle window, 40-sample GFNI and AVX2 comparisons
completed under the same lease and resource checks. GFNI 10+4/257 B verification
changed −2.89% valid and −4.19% corrupt, with baseline drift −5.81% and
−4.06%. Its 64 B controls changed +0.39% for 4+2 and +2.34% for 10+4, with
drift −3.39% and −6.35%. AVX2 4+2 encoding at 257 B and 258 B changed −8.21%
and −8.31%, even though that selected backend does not call the changed GFNI
tail. These shifts preclude attributing the observed GFNI gain to the tail
change alone. In the measured builds, the GFNI scalar-tail body grew by 12
bytes and the outlined helper occupied 155 bytes; the vector kernel size was
unchanged, but its address moved. Neither tail variant was integrated into
production. Physical host isolation was not established.

## Measurement scope

Native x86-64 Linux VM runs used Rust stable 1.99.1, locked dependencies, one
worker and A1/B1/B2/A2 ordering. The first tail screen used 20 Criterion
samples, 0.5 s warmup and a requested 2 s measurement per case; other completed
comparisons used 40 samples, 1 s warmup and a requested 3 s measurement.
A shared lease and exclusive guest CPU partition were held and restored for
each completed run. Physical host isolation was not established. Percentages
compare mean candidate and baseline latencies, where negative means faster;
baseline drift is A2/A1−1 and did not stop runs at the user's request. Complete
[stage estimates](../benchmarks/gfni-tail-followup/2026-11-04/measurements.json)
are retained. These observations are specific to the measured VM and workload.
