# Retained caller fixtures for release workers

Protocol 14 retains the caller contract introduced in protocols 9 and 12: it prepares one codec and caller fixture for the complete three-warmup,
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

Ordinary verify scratch, internal verify/recovery scratch and dynamic buffers in
other profiles remain unobserved. Observed stream signatures may differ between
versions; a descriptive match does not establish physical-page equality or replace
full per-layout and baseline/candidate acceptance. Backlog #2897 remains open.
