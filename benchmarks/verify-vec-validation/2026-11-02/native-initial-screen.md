# Native GF8 concrete-workspace initial screen

This diagnostic uses baseline commit `5a70a4c` and the adjacent isolated
`candidate.patch`, with the `verify.small_vec_opt/4x2/64` case added in
`9f73161`. Both Rust executables were cross-compiled from the same source and
target directories with stable 1.99.1, locked dependencies and `simd-accel`.
The candidate differs by the concrete GF8 workspace-validation prototype.
The executables were pinned to one Linux x86 CPU with one Rayon worker; their
backend names were checked. Per phase, the benchmark collected three 500 ms
samples and reported the median. Source snapshots and binaries were checked
before the run; embedded Git metadata was unavailable in the relocated binary.

For each backend, both binaries first passed the complete 16-case small-Vec
functional smoke catalog, including valid and corrupt inputs. Two same-source
A1/B1/B2/A2 preflights then passed a 5% phase-drift, same-source-bias and
cross-round gate before the candidate A1/B1/B2/A2 run. The candidate's A
reference was checked against both baseline rounds. CPU reservation,
competing workloads, off-target activity, steal and throttling were monitored;
any detected change stopped the owned child process.
The controller did not retain its per-interval monitor stream, so the resource
gate cannot be independently replayed from the retained benchmark samples.
The [sanitized phase samples](native-initial-samples.json) and
[gate summaries](native-initial-gates.json) retain all 24 measured phase
records and six summaries. Phase and cross-round calculations were replayed
locally from those records; that replay does not reconstruct resource history.

| Backend | Baseline phase mean | Candidate phase mean | Change | Baseline A drift | Candidate B drift |
| --- | ---: | ---: | ---: | ---: | ---: |
| AVX2 | 56.665 ns | 56.005 ns | -1.16% | +0.12% | +1.89% |
| GFNI/AVX-512 | 47.275 ns | 46.130 ns | -2.42% | +0.83% | -0.61% |

This is one shape and one length with short samples, not acceptance for the
optimization. In particular, it does not cover other shapes, the 256/257-byte
boundary, dense versus damaged parity, Leopard, larger inputs, or non-x86
platforms. A broader predeclared catalog was prepared. Its first launch stopped
before sampling when another workload changed the CPU reservation. A later
attempt exposed a controller variable-scope error and produced no valid
candidate comparison. After the scope fix, AVX2 4+2/64 passed both baseline
rounds, but a competing workload stopped the run during the second 4+2/256
baseline; the broader catalog never reached candidate timing. Do not use the
initial result to enable the prototype. The numerical values above are phase
medians and means, not confidence intervals or production latency.
A further restart stopped at the reservation precheck before taking samples.

A separate x86 cross-build with symbol stripping disabled measured the
`verify_with_workspace_opt` entry point at 644 bytes in the baseline and 742
bytes in the prototype. This is a code-size observation for that build, not a
latency result; the measured binaries used the normal stripped profile and
the helper and callees must also be considered. It differs from the earlier
AArch64 diagnostic, where the same candidate reduced the entry-point size.
