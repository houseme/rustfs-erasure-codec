# Retained caller fixtures for release workers

Protocol 15 retains the caller contract introduced in protocols 9 and 12: it prepares one codec and caller fixture for the complete three-warmup,
nine-observation schedule of each small-file/stream worker. Caller byte storage
is allocated before version-dependent codec setup. Retained source/scratch buffers
are checked by full pointer identity, length and capacity before and after every
inner timer. Exported positions contain only offsets modulo 64, 4096 and the
controller's actual OS base page size; full addresses remain private.

The original benchmark sources remain unchanged. The shared renderer extracts
each original operation loop once and binds it to the retained fixture. It changes
only operand references and makes verification booleans observable. It rejects
unknown loop layouts. The original per-iteration clone, buffer drop, missing-output
allocation and I/O work remain timed. Stream setup codewords are checked against
the original encode-separate fixture in real Rust tests. Codec cache state now
persists across observations, an explicit workload change requiring fresh evidence.

| Profile operation | Actual call entry | Timed destination policy |
| --- | --- | --- |
| small-file encode | Vec of Vec | Update retained parity in place |
| small-file verify | Vec of Vec | Internal per-call scratch remains untracked |
| buffered verify | Vec of Vec plus scratch | Retain caller scratch for the worker |
| ordinary/optimized/selective/data recovery | Vec of Option of Vec | Clone each iteration, drop missing buffers, allocate outputs through the original API |
| Slot recovery | Vec of ShardSlot of Vec | Clone each iteration, retain storage when marked missing |
| stream encode | Slice readers to Vec writers | Recreate adapters and independently reserve writers each iteration |
| stream verify | Slice readers | Recreate readers; internal scratch remains untracked |
| stream recovery | Cursor of Vec | Clone cursors, replace missing entries with empty cursors each iteration |

Every one of the 88 small-file and 15 stream case records requires the API entry,
destination policy and exact missing/required masks in addition to its retained
caller geometry. Unknown/missing contracts, changed buffers and calibration/ABBA
layout mismatches are rejected. `entry` and `lifetime` describe recorded caller
storage; `call_entry` describes the API adapter. Policies ending in `untracked`
explicitly exclude dynamic destination/internal-scratch offsets. Equal caller
metadata does not establish equal layouts for those allocations.

All 126 cases now retain one codec for the worker, including 15 smoke and eight
hotspot cases. Every record requires the actual API/output policy, missing/required
and changed masks, seed, source lifetime and construction order. Smoke retains
its original per-iteration generated source, allocations and timer. Hotspot retains
per-iteration generated/encoded inputs, clones, missing-output allocation and the
original recovery timers, but borrows one worker-seeded SmallRng instead of
obtaining an entropy-seeded thread RNG for every byte. Generation remains a
per-byte u32-to-u8 loop. Its generated-byte count and non-consuming two-word state
preview use fixed storage. The validator checks consumption, continuity, final
calibration and all four phases. A native correctness check executes all 23 changed
workers against each actual lockfile and compares their complete input contracts;
it does not qualify timing. This is an explicit workload change requiring fresh
complete evidence, not a migration of older observations.

No codec algorithm, matrix, encoded format, production API, threshold or allocator
tunable changes. Dynamic destination/internal-scratch geometry remains untracked,
including smoke/hotspot source positions. The formal catalog remains 126 cases;
this is not a selected stable subset.

`diagnose_fixture_layout.py` inventories all 103 retained caller cases in all four
phases at one iteration, producing 4,944 observations and 618 pairwise checks. It
preserves mismatches and interruptions and always reports no performance approval.
It does not replace full minimum-duration baseline/candidate ABBA, the separate
16-layout 1/4 KiB acceptance, or remaining #2897 dynamic-layout work. Conditions
1/2/3/5 stay partially complete until their whole scope is actually validated.

Hotspot missing/required result patterns use bounded inline metadata in protocol
10. Their original labels are formatted only in deferred exporters, after all
observations finish. A separate executable tests exact bytes against the actual
original String formatter and counts zero allocation/reallocation/free requests
for constructing and formatting the replacement. Protocol 12 changes hotspot random preparation as described above; recovery calls
and inner timing remain original. Fixed-storage workload metadata also has a
separate zero-heap-activity test. These checks do not extend caller-layout coverage
to hotspot buffers.

Protocol 13 prepares the codec and Rayon pool with the full reservation before
pinning only the measuring thread. It checks pool inheritance and actual measuring
thread affinity, and binds the CPU/constructor budget/pool width across calibration
and phases. The structural inventory now uses this same construction order;
controller-wide pinning before codec/pool setup is removed. This extends execution
comparability, not dynamic-output/internal-scratch geometry or throughput approval.

Protocol 14 adds only bounded-stack process minor/major fault counters to ordinary
observations. These include worker faults but are endpoint differences, not actual
dynamic buffer-view geometry. They do not complete the untracked scope above or
make the allocating process diagnostic interchangeable with formal capture.

## Actual stream I/O views and dynamic caller storage

`diagnose_fixture_layout.py --stream-views` reuses the existing four-phase inventory
and original generated stream loops. All 15 stream cases retain every observation
and call. Encode/verify use explicitly declared `repr(transparent)` diagnostic
Reader/Writer adapters, with the same slice/Vec element layouts, independent writer
reservations and original delegate I/O. No codec source, allocator or formal
sampler is replaced. The bounded global recorder allocates no heap in callbacks;
its synchronization and memory touches still perturb timings and may affect layout.
These data never qualify formal performance geometry or release throughput.

The Read boundary exposes actual source prefixes and internal read scratch slices
for ten cases. The Write boundary exposes actual parity slices for five cases and
caller-owned writer length/capacity before and after each append. Full pointer
identity is checked privately across each reserved append; only modulo-64/4096/OS
page positions and private equality flags are exported. Borrowed view capacity is
null. Cursor reconstruction retains its exact original entry and records clone,
empty-missing and recovered owned buffers, then drops them at the original scope.

Both inventory modes reuse formal backend/features/I/O provenance and fixed
preparation/placement comparisons through all observations and phases.

Schema `stream-buffer-views-v2` introduced bounded windows around each original call and
its writer reservation, actual Vec append and original-scope writer drop. On Linux,
each window retains process minor/major fault counters (including pool workers,
excluding child processes), the calling thread's scheduler run/wait counters and
monotonic timestamps. Shared bounded procfs parsers are extracted unchanged from
the ordinary observer. Linux capture and offline inventory replay reject missing
counters; portability checks on other platforms explicitly record them unavailable.
The separate heap probe includes actual callbacks and these counter reads.

Validation requires the full reserve/append/drop sequence, matching byte/shard
identities from actual Write views, monotonic counters and nonoverlapping windows
inside each call. Parallel overlapping writer windows are rejected rather than
summed; current catalog Auto writers are serial. Reports partition each call into
those windows and `outside_writer_windows`. The latter includes codec work, reads,
other allocation/free, adapter metadata and the diagnostic probes themselves; it
must not be labeled pure encode CPU. Scheduler counters cover the calling thread,
not total process CPU. Fault reads bracket operations at different instants from
the timestamps, so these are diagnostic intervals, not exact fault-handler costs.
Window counters alone do not identify residency, allocation identity across calls
or the unique cause of a fault. Current schema `stream-buffer-views-v3` retains
these windows and adds separate native virtual-page snapshots at explicit I/O,
API-return and release boundaries. These adapters do not enter formal capture.
Protocol 15 changes the shared service-window contract; historical view-v1/v2
observations are not upgraded to native residency records.

The validator requires every call/drop completion, original source-prefix/EOF
consumption, transferred byte, reservation, cursor state and fixed observation.
Overflow, unknown capacities, missing or reordered lifecycle stages fail without
removing raw logs. Safe owned-overlay cleanup precedes the success report. Native
checks run the real APIs from each source/lockfile, compare normal/wrapped parity,
short tails and corrupted parity, and count observer I/O heap requests separately.

```sh
python3 scripts/diagnose_fixture_layout.py --check-stream-views \
  --baseline-ref 9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6
python3 scripts/diagnose_fixture_layout.py --stream-views --iterations 2 \
  --tool /path/to/frozen/tool --baseline /path/to/9.0.0 --candidate /path/to/candidate \
  --output /path/to/new-inventory --isolation-cgroup /sys/fs/cgroup/owned-partition
```

The I/O decorators alone do not expose internal stream verify/recovery owners.
The source-owner audit below covers those stream-layer buffers separately; codec
internal decode scratch and other dynamic paths remain outside that coverage.
Observed signatures may differ between versions. Neither a descriptive match nor
source instrumentation establishes physical-page equality or replaces full
per-layout and baseline/candidate acceptance. Backlog #2897 remains open.

The [focused scratch lifecycle review](stream-scratch-service-review-2026-10-19.md)
records the earlier allocator page-return evidence, rejected profiler coverage
and a separate longer-window same-version control. Subsequent native snapshots
cover the declared stream boundaries; neither capture qualifies a candidate release.

For native stream virtual-page boundaries and the uniform protocol-15 service
window, see [the native residency review](native-residency-service-window-2026-10-19.md).

## Stream byte-owner lifecycle coverage

`stream_core_audit.py` already observes the actual byte owners inside all three
stream APIs, in disposable copies of published 9.0.0 and the candidate:

| Observed owner | Declared coverage |
| --- | --- |
| Read buffers and encoding parity | Reservation, initialized length, visible prefix, coding and owner release |
| Stream verification scratch | Old per-block `verify_par` SmallVec versus current stream-retained parity buffers |
| Stream recovery buffers | Old Vec/Option ownership transfers and replaced former ranges versus current planned, retained buffers |

The original complete structural record binds candidate
`df255fb133f872eee35707d2645c67675fd79bb7` and published 9.0.0. It retains all
15 stream cases per source, 360 observations, 720 calls and 95,760 boundaries;
its complete numeric capsule independently replays with the current validator.
[PR #35](https://github.com/houseme/rustfs-erasure-codec/pull/35) delivered this
coverage. Do not keep these byte-owner paths on a list of unimplemented observers.

The [main CI at 4f2799e](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39394001627/job/118532102062)
also completed the real two-source stream-owner check. That is new source-bound
structural execution, not a migration of the earlier raw records or accepted
throughput. Checks retain original allocation/free statements, API results,
source/lock bindings and every declared lifecycle boundary. No new raw stream
artifact is uploaded by this step.

```sh
python3 scripts/stream_core_audit.py --check --output /path/to/new-source-owner-check
python3 scripts/stream_core_audit.py --replay /path/to/completed-source-owner-check
```

The unified remaining-storage inventory below observes codec-internal decode
scratch, non-stream dynamic destinations and smoke/hotspot source positions.
Formal performance geometry/paired-layout acceptance remains separate. Physical allocation identity and exact first-
fault instruction are not established. Source instrumentation and page/counter
queries perturb execution, so these structural checks never qualify latency or
release acceptance. Conditions 1/2/3/5 therefore remain partially complete.

## Every raw observation must remain comparable

The release controller, strict offline preflight and structural inventories share
one provenance comparison over every raw row, including all calibration rehearsals,
warmups and nonmedian/outlier samples. Backend/target/features/I/O metadata and
worker placement must match their case reference throughout calibration and each
phase. Candidate calibration must also match the baseline reference. Matching
median rows or only observation zero is insufficient. Metrics, proposed iteration
counts, source revisions and normalized metadata aliases can differ as declared;
no raw values are rewritten or observations removed.

This validation-only tightening does not change sampler protocol 15, worker code,
overlay/implementation digests, original timed work or quality thresholds. Frozen
captures are checked offline with the stricter validator; they are not remeasured
solely because the verifier changed. Any actual mismatch rejects attribution.

## Ordinary verify scratch: diagnostic source copies

`verify_scratch_audit.py --check` observes all three smoke and eleven small-file
ordinary Classic verify cases in actual published 9.0.0 and candidate source
revisions with their own lockfiles. It checks out disposable source copies and
adds reversible hooks only around the original SmallVec scratch. The prepared
worker, original Vec API, operation loop, three warmups and nine samples remain;
each observation performs two original calls. Hooks never enter production
checkouts, the formal renderer or the running protocol-15 campaign.

The guard is declared before scratch, so scratch and its borrowed views drop
before the guard records the former numeric range. It records real capacity,
initialized length, inline/heap storage, private pointer stability, modulo
64/4096/native-page offsets, native residency and whole-process fault endpoints at
reservation, initialization, after verification and after owner drop. No reference
or slice to uninitialized/former storage is created. Inline owner drop does not
free the stack frame; mapped former ranges do not prove allocation/physical-page
identity. Snapshots include partial boundary pages and may include neighboring
bytes. Queries do not read, initialize or lock observed contents.

Every call/boundary and original healthy-input boolean is checked. Hooks retain
false results too. Interrupted/external-thread callbacks invalidate the ledger
without panicking in the destructor. Callbacks use fixed storage and existing
bounded fault parsers/mincore; the independent allocator probe checks zero
alloc/realloc/dealloc. Synchronization, stack touches and probes still perturb
execution and may change compiler/layout behavior. They cannot establish
uninstrumented geometry equivalence, precise first-fault instruction or candidate
throughput. Fault endpoints include observer work and must remain inside their
original outer observation counters.

Capture and read-only replay share one validator. Digests bind every raw sample,
numerical timing journal, scratch ledger, original source/lock, reversible source
change, native hook and rendered worker. All fourteen cases and every observation
are required on both sides. Backend/placement/preparation mismatches are refused;
observed scratch layouts may differ and are reported without pooling. Existing
output directories and unexpected source edits are preserved. CI retains complete
numerical JSON/journals on success or failure; it excludes source copies, full
addresses, private paths, build logs and exception text. Original full logs remain
with the execution workspace. The numerical capsule replays independently of those
private logs. Non-Linux runs explicitly lack native residency/counters and cannot
substitute for native Linux observations.

```sh
python3 scripts/verify_scratch_audit.py --check --output /path/to/new-check
python3 scripts/verify_scratch_audit.py --replay /path/to/completed-check
```

This covers only the declared ordinary-verify diagnostic visibility gap. The
separate stream-owner audit covers stream-layer verify/recovery byte storage.
Neither completes #2897 conditions 1/2/5, codec-internal decode scratch, remaining
non-stream dynamic paths, complete layout pairing or final performance/release
acceptance. The actual [4f2799e numeric artifact](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39394001627/artifacts/12250674932)
was independently replayed in full: both sources and all fourteen cases pass the
structural contracts, while thirteen cases retain diagnostic shape differences.
No observations were removed or pooled into a formal timing comparison.


## Complete remaining storage inventory

The existing fixture inventory now has one source-copy mode for the remaining
decoder scratch, non-stream output and smoke/hotspot source scope:

```sh
python3 scripts/diagnose_fixture_layout.py --check-dynamic \
  --output /path/to/new-private-dynamic-inventory
python3 scripts/diagnose_fixture_layout.py \
  --replay-dynamic /path/to/completed-private-dynamic-inventory
```

The catalog is fixed. Both actual source revisions execute all 126 original cases
in the original calibration and measurement modes, with three warmups and nine
observations, two original kernel iterations per observation. The calibration
pass is a bounded structural rehearsal; its actual target/complete flags remain,
but it does not qualify duration or trigger adaptive count retries. Six additional
decoder-owner cases cover uncached Classic inverses, tower GF16, and Leopard GF8
and GF16 at 32,768 bytes and a 65,600-byte multiblock tail. Across both sources this
requires 516 fresh workers, 6,192 observations and 12,384 kernel iterations.
Hotspot iterations contain multiple original API calls; iteration counts must
not be presented as counts of individual APIs.

| Storage | Observed boundaries |
| --- | --- |
| Smoke/hotspot sources | Actual generated/encoded Vec positions, ownership moves or clones, and original-scope destruction |
| Non-stream recovery outputs | Clone, actual missing/required presence, restored views and request release; missing ShardSlot storage is distinguished from absent Option storage |
| Classic decode matrices | Original initialized element storage, inversion work, fresh/cached decode views, inline/heap form and source-specific capacity |
| Leopard decoding | Cached/uncached work leases, active lane shape, locators, conversion/scaling scratch, and real internal decoded-output buffers before publication |

No production checkout is edited. Source-copy insertions are reversible and
preserve original constructors, allocations, resizes, copies, API calls and release
statements. The generic recovery publication/late-failure/selective semantics are
unchanged. An active/inactive observer oracle checks both actual sources in all
four field/family configurations. Candidate invalid-length rejection is checked;
the old Leopard implementation's known unchecked malformed raw-view path is not
executed as an oracle. Legacy selective behavior is compared with itself, not
silently replaced by the candidate's stronger contract.

Only the generated diagnostic executable installs a System-forwarding Rust
allocator observer. Its fixed state and acquire/release guard do not allocate;
independent heap and concurrent request tests check the callbacks. Every observed
alloc/realloc/free has a request generation; a successful realloc ends the old
generation even when the address is unchanged. Views bind the generation and a
relative byte offset. These are observed Rust/System request identities, **not
physical allocation identities, malloc usable sizes or a source-level allocation
count guarantee**. The optimizer may omit or combine allocations, as described in
the [GlobalAlloc contract](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html).
Missing mappings are refused rather than invented.

The bounded ledger keeps modulo-64/4096/native-page offsets, initialized lengths,
actual known capacities, anonymous threads, process fault endpoints and numerical
virtual-page queries. No full pointer is exported; uninitialized or former bytes
are never dereferenced. Unmapped former ranges and bounded-query limits stay
explicit. A mapped range does not establish continued ownership. Requests retained
by a thread-local cache after a call are not classified as leaks.

Before execution, a complete plan binds source/tree/lock, original harness,
reversible changes, generated worker/native observer and compiler/CPU/allocator/
kernel-memory-policy context. Every step records the same context before/after;
source edits, missing/reordered events, stale request generations, incorrect
presence, native bounds, raw-journal disagreement and incomplete catalogs refuse
the inventory. Failure logs, all completed records and the original plan remain.
Existing output is never replaced. Raw output must be outside the source repo,
in a new private directory; CI runs this check without uploading its raw files.

Raw workload records carry the explicit `dynamic-source-copy-v1` envelope, which
the formal workload validator rejects. Only a private validator copy unwraps it
to reuse the original scalar/seed/input contracts. Instrumentation changes code
generation, allocator concurrency, memory touches and service time; callbacks
include observer costs and may include unrelated process activity. Matching
instrumented geometry cannot establish uninstrumented equivalence. The result
always has `release_approval=false`, `candidate_attribution=null` and
`formal_performance_geometry_equivalence=false`. This completes the implemented
observation mechanism, not paired-layout/ABBA acceptance or a release waiver.
