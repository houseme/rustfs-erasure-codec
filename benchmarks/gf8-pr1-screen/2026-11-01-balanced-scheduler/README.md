# Balanced output-group experiment

This is an experimental patch against `e49c796`, not active product code. The
existing scheduler groups cache chunks with a uniform rounded-up group size.
For one output, four permitted jobs, 1,048,576 bytes and a 209,664-byte cache
chunk, six chunks become three groups. The final chunk is only 256 bytes. The
worker-distribution diagnostic in the adjacent directory observed this case.

`balanced-groups-experiment.patch` recursively divides disjoint, cache-aligned
chunk ranges using Rayon joins, with at most the assigned number of leaf tasks.
This creates four groups for that case and retains each chunk's original byte
offset. It affects the shared scheduler for all fields and workloads where
outputs are fewer than jobs. Extra joins, group-size changes, work stealing and
cache effects can reduce performance; four groups do not imply four active
workers or a faster operation.

On an isolated source copy on Apple Silicon, the candidate passed the scheduler
byte-coverage regression, GF8 integration tests, release compatibility probe
(200 standard, 36 large GF8 and 12 six-plus-two cases, plus 200 published-version
cases), all-features library Clippy, and the no-default-features build. The
all-features library suite also passed with the temporary instrumentation patch
applied and test threads serialized: 415 passed and five ignored.

`leaf-count-instrumentation.patch` is an additional test-only modification. An
isolated test observed four leaf tasks and checked the complete output, including
the short tail. Its shared counter is intended only for the isolated serialized
test, not production or a concurrent test suite. On native x86 with the AVX2
feature, the uninstrumented candidate passed the scheduler byte-coverage test,
the five applicable parallel-codegen regression tests, and the six-plus-two
compatibility test. These are functional checks, not performance measurements.

The previous strict preflight was rejected for -5.10% cross-round drift. No
candidate timing has been run here. Before enabling this generic scheduler
change, use a fresh exact-source A/A preflight with resource monitoring, then
A/B/B/A comparisons across the affected GF8 and GF16 encode and recovery
workloads, sizes, job counts, and output counts, with byte-level compatibility
and no regression on the small cases. Reject the candidate if those gates fail.
