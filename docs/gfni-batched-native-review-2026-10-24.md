# GFNI batching: exact-source native compatibility review

Main `544bb06932c3c0aea6b3b8f08dad5ac1e828da9b` includes the four-vector GFNI
row revision from `727690f`. Its independent native validation is now complete.
The [aggregate report](../benchmarks/release-9.1.0/2026-10-24/batched-main-native-functional-review.json)
binds that executed source and retains every one of the 29 command outcomes.
The earlier `3a62879` native proof and the refused `e3a1c1c` / `b1a10e4` timing
cohort retain their own sources; neither is substituted for this result.

## Encoding, decoding and publication compatibility

Both GFNI AVX2 and AVX-512 row kernels execute positively. Their scalar oracles
cover dense 32-term rows, zero/unit/general coefficients, unaligned prefixes and
suffixes, chunk boundaries, four-vector groups, remaining vectors and scalar
tails. Malformed views still refuse before output writes. The full native suite
passes 405 library, 160 integration and ten doc tests (575 parent tests, 43
existing ignored); five isolated filtered child executions are recorded
separately. The no_std suite passes 261 tests. Twelve backend-environment stress
rounds each pass 15 parent tests.

Published 9.0.0 compatibility retains 200 normal configurations, 162 custom
configurations / 972 cross-recoveries and 72 wide GF16 configurations. Six
backend settings also pass all 1,620 selective configurations / 15,840
cross-recoveries. Normal bytes, bidirectional data-only/full recovery and the
absence of unrequested missing data remain checked. This result supports the
batched implementation's functional compatibility, including its shared coding
entry used by encoding and recovery; it does not establish throughput.

## Shared resources and independently verified finalization

The new validation uses the existing nonblocking shared reservation protocol.
All 38 tests of its process visibility, ownership, continuous observation and
recovery controls pass. The reservation spans the actual native checks,
source/original-service verification, remote archive verification and independent
download acknowledgement. No original service or cpuset configuration is changed;
this functional-only run makes no formal performance-isolation claim.

The first finalization fails with `JSONDecodeError`. Source-order review locates
the failure in ACK parsing, consistent with the reader observing the file after
creation but before its non-atomic upload completes; the complete payload later
parses and matches the verified archive.
This is a control error, not a codec failure, statistical refusal or hardware
capability result. The completed 29-command results and first failed terminal,
reservation fence and first independent-reader refusal all remain intact.

Explicit recovery is restricted to that exact failed owner/fence. It checks the
unchanged source, original service identities and cpuset inventory, then verifies
the already-complete original ACK and a fresh recovery archive independently.
The new ACK is uploaded under a temporary name, fully parsed/verified and synced,
then published by atomic rename. Only then is the original task's reservation
closed. No native command is retried, no frozen controller/source is edited, and
no external process is signalled. Independent replay checks all 82 inputs
unchanged and exactly reproduces all native summaries/required markers, original
failure and explicit recovery evidence with zero codec calls.

## Remaining performance and release work

This proof has zero performance observations. It does not overturn the
[complete 72-case repeatability refusal](selective-parity-planning-2026-10-24.md#complete-72-case-api-result-repeatability-refusal)
or any earlier complete-campaign refusal. No new runtime variant, size/shape
tuning, allocator control or altered benchmark is introduced here.

The source contains four-vector matrix reuse; accepted throughput improvement
still requires the intended final source/tool/lock freeze, complete accepted
published-9.0.0 ABBA/paired-layout evidence and actual strict positive release
preflight. Ordinary CI, this native proof and successful reservation recovery do
not approve publication. No tag, registry upload or automatic PR merge occurs.
