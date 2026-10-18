# Retained caller fixtures for release workers

Protocol 9 prepares one codec and caller fixture for the complete three-warmup,
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

The original smoke and hotspot preparation/timing contracts remain intact. In
particular, hotspot inputs are still generated/encoded every iteration. No codec
algorithm, matrix, encoded format, production API, threshold or allocator tunable
changes. The formal catalog remains 126 cases; this is not a selected stable subset.

`diagnose_fixture_layout.py` inventories all 103 retained caller cases in all four
phases at one iteration, producing 4,944 observations and 618 pairwise checks. It
preserves mismatches and interruptions and always reports no performance approval.
It does not replace full minimum-duration baseline/candidate ABBA, the separate
16-layout 1/4 KiB acceptance, or remaining #2897 dynamic-layout work. Conditions
1/2/3/5 stay partially complete until their whole scope is actually validated.
