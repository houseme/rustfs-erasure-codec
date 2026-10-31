# Byte-balanced output-group experiment

This experiment is based on `f3c6ed647392c8c54d8393ba41adf0461f0f9428`.
It is a test-only patch and is not enabled in production. It follows the
[earlier chunk-balanced experiment](../2026-11-01-balanced-scheduler/README.md).

For one output of 1,048,576 GF8 elements, chunk length 209,664 and four jobs,
the existing scheduler makes three groups of 419,328, 419,328 and 209,920
bytes. The earlier recursive candidate creates four groups but still has a
419,328-byte largest group. More groups therefore do not establish a shorter
critical path.

`byte-balanced-experiment.patch` instead computes a group boundary from the
output length and per-output job budget, rounded up to 64 *elements*, then
processes each group in chunks no larger than the existing chunk length. The
above case becomes four 262,144-byte groups. For wider elements the byte
counts scale with element size; the 64-element rounding is not a claim about
pointer alignment. The patch adds a temporary test checking all offsets and
contents for both `u8` and `[u8; 2]`, including the original short-tail case.
It changes callback boundaries: the example invokes eight chunks rather than
six. Additional calls, shorter SIMD operations, cache behavior and scheduling
overhead may outweigh any improvement. There is no performance claim.

The candidate was applied to separate copies of the exact base revision. On
Apple Silicon, the all-features library suite passed 415 tests with five
ignored, including the additional boundary test; the GF8 parallel/codegen,
GF16-compatible verification and reconstruction-workspace integration tests,
all-features library Clippy, and no-default-features build also passed. The
release compatibility probe passed the standard 200, large GF8 36, six-plus-two
12, and published 9.0.0 non-fast 200 cases. These tests prove only their covered
inputs and environments.

On native x86 Linux, the candidate passed the scheduler coverage and new
byte/wide-element boundary tests, the AVX2 parallel-codegen, six-plus-two,
verification and reconstruction-workspace tests, and the AVX-512-only encoder
integration tests. The standard release compatibility probe passed there too.
The AVX-512 test exercised the generated encoder's threshold dispatch.

No candidate timing was run. The last instrumented A/A preflight was rejected
for 5.10% cross-round drift; merely changing the candidate cannot repair that
baseline. A fresh production-context, resource-monitored baseline must pass
before any A/B/B/A attribution. Acceptance also needs GF8 and GF16 encode and
recovery across sizes, output counts, jobs, and relevant backends, plus checks
for regressions from the increased callback count. Until then, retain this as
an experiment and keep the existing production scheduler.
