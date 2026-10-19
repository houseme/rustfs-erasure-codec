# Protocol-14 native ordinary-observer check — diagnostic only

Frozen tool `82d918861c88456455f42305f0a134c63ee23f98`, actual published 9.0.0
baseline. The native heap executable passes 15 tests, including 16 consecutive
ordinary telemetry reads with 0 alloc/realloc/dealloc and real unsigned process
fault fields. The actual old-codec generated sampler passes 17 active tests and
Clippy with warnings denied. Shared tests are not summed as unique coverage.

One original stream encode worker uses the count six inherited from the original protocol-13 control for this structural
diagnostic, normal
nonallocating telemetry only, and all 3 warmups + 9 measured samples. Every timing
journal matches its completed sample; masks, preparation and counter contracts
validate. No complete calibration, 126-case qualification, candidate comparison or
release approval is claimed. This source/protocol cannot inherit the older
protocol-13 failure as passing evidence.

The initial d56ae4b attempt failed before measurement: a standalone probe referenced
an undeclared serde_json dependency. Its status/cleanup remain preserved. The fix
validates exported integer fields without adding dependencies. The corrected
reservation completed 43 isolation checks and restored all original resources.
No raw performance snapshot was overwritten or retried until passing.

The ordinary nonallocating observer reproduces an oscillating page-fault/time
pattern: measured observations range from 18,438 to 110,604 process minor faults,
with zero major faults, and approximately 139.22 to 228.78 ms per observation.
All source/placement/counter contracts match and all observations remain retained.
This localizes a changing guest-process page-lifecycle cost without the allocating
thread observer. It does not identify which internal buffer or underlying allocator,
VM or physical mechanism causes every transition, nor explain all nine full-A1
failures. Increasing warmup, changing the workload or removing original allocation
costs is not justified by these data.
