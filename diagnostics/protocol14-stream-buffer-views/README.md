# Actual stream buffer views — structural diagnosis only

Frozen capture tool `3cd46696d0922edd88496962ae497d87dfbf639c`, protocol 14 with
explicit transparent diagnostic adapters. Baseline is published 9.0.0; candidate
production code is unchanged from merged main 6182941. This is not a replacement
for original Vec performance cases, accepted evidence or a release manifest.

The existing inventory collected all 15 stream cases in A1/B1/B2/A2: 720 complete
observations, each with two original API calls (1,440 total). Every EOF, transferred
byte, writer append/reservation, cursor clone/missing/recovery state and original
scope drop completion validates. All 720 completed samples match their numeric
journals. Native Linux heap executable passes 19 tests including zero
alloc/realloc/dealloc for observer callbacks plus reserved Vec I/O; generated
sampler Clippy passes. Shared test counts are not summed. The eight-CPU partition,
controller and original CPU ranges were restored after 80 isolation checks.

## Recorded differences

All 15 retained-source geometries match. Both same-version comparisons have equal
sets of view shapes/offsets (30 checks). Every cross-version comparison differs
(60 checks across four pairings of the same 15 cases). A set match does not prove
per-observation frequency/order, full-address identity, physical-page equivalence
or stable performance. Raw ordered events are retained without filtering.

| Case | Roles with different shape/offset sets in A1/B1 |
| --- | --- |
| `encode_stream:10:4:1048576:1048576` | `read_scratch_view`, `parity_write_view`, `writer_before`, `writer_after` |
| `encode_stream:10:4:16777216:4194304` | `writer_before`, `writer_after` |
| `encode_stream:10:4:65536:65536` | `read_scratch_view`, `parity_write_view`, `writer_before`, `writer_after` |
| `encode_stream:4:2:1048576:262144` | `read_scratch_view`, `parity_write_view`, `writer_before`, `writer_after` |
| `encode_stream:4:2:65536:65536` | `read_scratch_view`, `parity_write_view`, `writer_before`, `writer_after` |
| `reconstruct_stream:10:4:1048576:1048576` | `cursor_before_missing`, `cursor_missing`, `cursor_after_recovery` |
| `reconstruct_stream:10:4:16777216:4194304` | `cursor_before_missing`, `cursor_missing`, `cursor_after_recovery` |
| `reconstruct_stream:10:4:65536:65536` | `cursor_before_missing`, `cursor_missing`, `cursor_after_recovery` |
| `reconstruct_stream:4:2:1048576:262144` | `cursor_before_missing`, `cursor_missing`, `cursor_after_recovery` |
| `reconstruct_stream:4:2:65536:65536` | `cursor_before_missing`, `cursor_missing`, `cursor_after_recovery` |
| `verify_stream:10:4:1048576:1048576` | `read_scratch_view` |
| `verify_stream:10:4:16777216:4194304` | `read_scratch_view` |
| `verify_stream:10:4:65536:65536` | `read_scratch_view` |
| `verify_stream:4:2:1048576:262144` | `read_scratch_view` |
| `verify_stream:4:2:65536:65536` | `read_scratch_view` |

For the previously widest `encode_stream:10:4:16777216:4194304` case, retained
source, observed read scratch and parity write-view sets match, while writer
before/after storage sets differ. This identifies a remaining caller-output
comparability boundary; it does not prove writer placement uniquely causes the
old 59.103368% timing distribution or quantify a 9.1.0 regression.

Borrowed read/parity views expose actual slices but not backing capacity; their
capacity is null. Owned writer/cursor observations include real length/capacity.
Private full-pointer equality is checked for reserved appends, while only modulo
64/4096/OS-page positions and prior-view equality flags leave the process.

## Strict replay and source boundaries

Review reproduced acceptance of mismatched backend/I/O/constructor budgets in the
initial structural comparison. The shared validator now requires formal
backend/features/I/O and prepared-workload/placement comparability in both
inventory modes. Offline replay validates all 720 original observations under
those stricter checks, retaining the same 60 structural mismatches. It records
validator source separately; no measurement or numerical data was repeated or
rewritten. Instrumentation and formal-sampler digests still match the capture.

The first CI also exposed a stream-only test dependency embedded in the generic
allocation probe reused by CPU placement. The dedicated test is now an explicit
attachment; the generic counter is restored to its main version. CPU-placement
and standalone heap checks compile and pass. This test-composition fix does not
change the frozen worker instrumentation or invalidate these captured views.

Ordinary verify scratch, hidden verify/reconstruction scratch, dynamic buffers in
other profiles, complete controlled-layout acceptance and final full candidate
ABBA remain outside this evidence. Callback synchronization, memory touches and
explicit adapter types perturb observation; zero callback heap allocation is not
zero overhead or proof of original formal geometry. No source/algorithm/format,
production API, original warmup/sample count, duration or regression threshold was
changed. release_approval is false and candidate_attribution is null throughout.

The snapshot retains every complete sample/view, timing journal, source/context,
frozen count and restoration outcome. Numeric logs contain only original timing
and view lines; ancillary build/launcher logs with machine paths remain private.
Replay with the bound validator module by loading the four sample/view files and
calling `analyze_stream_views` using the recorded sources, CPU mask, page size and
iteration count. The diagnostic renderer remains separately fingerprinted from
the formal sampler. Never merge this audit branch into main or configure it as
RSE_RELEASE_EVIDENCE_REF.
