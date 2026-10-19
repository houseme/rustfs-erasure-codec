# Stream scratch page lifecycle and service-time review

This review follows backlog #2897's comparability/service-time-first order. The
starting main is `aaebcb3835d6767fce44e4fbdbd3389664729713` (PR #29 merged).
Production codec code, algorithms, format, dependencies and generic reconstruction
publication/failure/selective semantics are unchanged. No VerifyWorkspace or
Leopard owned-output change is justified by these diagnostics.

## What the existing stream call actually owns

Published 9.0.0 `encode_stream` reserves ten data and four parity buffers at the
selected 4 MiB block size: 56 MiB per API call. Read buffers become initialized in
`prepare_read_buffer`; parity buffers are resized before encoding. Both groups
are destroyed before the API returns. The benchmark independently reserves four
16 MiB Vec writers before each API and destroys them afterwards. None of these
allocations, zeroing, copies or frees have been moved out of the original loop.

Consequently, equal modulo-page positions do not establish equal page backing
state across calls. A reserved address range can remain mapped after its allocator
has discarded pages. Conversely, unmapping a large PROT_NONE reservation does not
mean that its entire virtual size had been resident.

## Observed allocator and release paths

The targeted default-allocator trace retains all 12 observations / 72 calls of
`encode_stream:10:4:16777216:4194304`, with six original calls per observation.
The measuring task executes 416 observed VM syscalls. Its ledger includes 72
128 MiB and 36 64 MiB anonymous PROT_NONE mappings, and 36 successful
`MADV_DONTNEED` requests of 46,006,272 bytes (43.875 MiB). The task shows no brk
growth. This is consistent with arena heap creation/release, rather than evidence
of a changed coding matrix or of permanent application allocations of that size.

The ledger tracks mmap intervals, partial munmap, MAP_FIXED replacement and advice
against mapping IDs. Full addresses and task IDs never enter its public output.
Advice bytes and unmapped bytes are requests/virtual ranges, not measured physical
RSS, unique physical pages or simultaneous memory usage. mprotect and physical
residency are explicitly unobserved.

[glibc 2.39 arena.c](https://raw.githubusercontent.com/bminor/glibc/glibc-2.39/malloc/arena.c)
implements heap shrinking with MADV_DONTNEED or remapping, and whole-heap release
with munmap. The observed trace is consistent with those paths; it does not prove
which individual malloc arena owned every Vec.

The whole-process CPU profile has 2,730 samples, zero lost/throttle records and
873 unresolved leaves (31.98%). Positive source/function context classifies 299
samples in the stream read path, 217 in the stream write path and 106 in encode
cleanup. Cleanup includes kernel unmap/zap/free-pages frames returning through
the encoding function's closing source line. These are whole-profile sample
counts, including preparation/warmups/export, not timed-loop CPU percentages or
expected throughput savings. Unmatched and unresolved records remain present.

## First-touch attribution remains bounded

Frame-pointer records preserve 857,298 minor-fault events, but can omit an
immediate libc caller. They support read-path context, not an exact separation of
read initialization, copying and parity initialization. Exact initialization
roles require a source-bound statement; names appearing only inside Rust generic
type arguments do not prove that a function is executing.

A 512-byte DWARF diagnostic preserves 857,293 events without loss/throttle, but
855,247 have no expandable callchain (99.76%). Those events remain explicit
unknowns; they are not removed or guessed into a scratch category. A subsequent
compressed full-stack diagnostic exits zero while reporting EFAULT writes and
compressed-data corruption. It is rejected as unusable, including for attribution.
All its numerical observations and restoration outcomes remain separate. No more
unchanged profiler retries follow this failure.

The capture-log validator therefore requires both a successful exit and usable
capture diagnostics. It rejects zero-exit corruption, write failure and loss.
C-side aggregate record statistics include MMAP2 and must conserve the total;
Python need not dump/scan every raw user-stack byte just to count records. The
stack inventory interns repeated frames once, retains every event/actual period
and keeps missing callchains explicit. The VM accepts a requested minor-fault
period but emits actual period 1; no count is multiplied by the requested value.

## Longer original service-window control

After observing the allocator phases, one separate default-build/default-allocator
same-version control predeclared 64 original calls per observation, with the same
three warmups and nine samples. It uses no perf, strace, view adapter, frame-pointer
build flag or allocator override. It does not alter formal protocol 14 or the
original duration-calibrated six-call count.

Both fresh workers retain all 24 observations / 1,536 API calls. They match their
caller fixture, workload and placement contracts. The existing central-width
calculation gives 3.391681% / 3.461520%; median repeat drift is 1.621819%. Thus this
one longer observation window meets the existing 10% / 5% point-control limits.
It does not qualify all 126 cases, long campaign epochs, dynamic layout matching,
16-layout controls or a candidate release. Timings from different iteration
counts or profiler modes are not compared as speedups.

This supports investigating finite-window mixing of allocator page states before
changing codec runtime. It does not prove that increasing the observation window
alone repairs every failure. The next measurement-contract change must be shared,
predeclared, source-bound and independently qualified; do not special-case a
favorable case/count or silently replace per-call allocation/free with reuse.

## Replay and review boundaries

`perf_stack_audit.py` only analyzes a saved perf-script stream, aggregate record
counts and the exact codec source. `stream_allocator_audit.py` only reads a saved
syscall trace and its original sample JSON. Neither executes a benchmark or
modifies codec/workspace state. Their tests cover unknown/inline frames, raw/decoded
conservation, real periods, Rust generic-name ambiguity, corrupted captures,
partial mappings, failures, resumed calls, split stdout and journal mismatches.

The trace's actual output order is twelve immediate timing journals, followed by
twelve deferred sample exports. Ledger observation intervals run between timing
writes and include boundary telemetry/journal overhead; the first includes startup.
They are not asserted to be exact individual API boundaries or pure codec CPU.

Backlog #2897 remains open: 1/2/3/5 partial, 4 within its declared normal-format
compatibility scope, 6 unaccepted. Read/parity first-touch residency and exact
per-allocation lifetime attribution remain incomplete, as do other dynamic paths.
These diagnostics are audit data, never an accepted release manifest. No tag,
upload, gate weakening or candidate throughput claim follows from this review.
