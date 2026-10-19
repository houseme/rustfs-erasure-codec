# Native residency and a uniform service-window contract

The previous [scratch review](stream-scratch-service-review-2026-10-19.md) left
physical residency unobserved and rejected the DWARF recordings. This follow-up
uses native virtual-page snapshots at explicit boundaries. No further DWARF
capture, production codec change, allocator override or buffer reuse is introduced.

## Native boundaries and limits

The existing transparent stream reader/writer diagnostic now queries Linux
[`mincore`](https://man7.org/linux/man-pages/man2/mincore.2.html) using a checked,
fixed stack bitmap and the actual native page size. It does not read, write,
prefault or lock the buffer memory. Ordinary views remain byte-for-byte I/O;
reservations, initialization, transfers, EOF and original drop scopes are retained.
The ledger requires every ordinary view, each before-read and reserved-writer
boundary, encode-return scratch range and post-drop writer range. Missing live
coverage, invalid/error results, overflow and reordered boundaries reject the
capture. Parallel reader callbacks may interleave; each shard must retain its
own before/after order. Non-Linux support is explicitly unavailable, never zero.

A snapshot counts resident **virtual pages**. Boundary pages may contain adjacent
objects; counts cannot be summed into unique RSS or interpreted as physical IDs.
Former ranges may have been reused by the allocator or another thread. ENOMEM
means `not_fully_mapped` and an unknown resident count, not that every page is
unmapped. Other errors are retained and rejected. No addresses or task IDs are
exported. The queries, stack bitmap, locking and existing procfs reads perturb the
process: these observations do not qualify formal throughput or exact faulting
instructions. Verify/recovery internal scratch and non-stream dynamic buffers
remain outside this diagnostic's coverage.

## Complete native capture

[Immutable audit data](https://github.com/houseme/rustfs-erasure-codec/tree/a39af5f74e1382865ebec58b2760eac0d8d4fbe5/diagnostics/protocol14-native-residency)
retains all 15 published-9.0.0 stream cases: 180 observations, 1,080 original API
calls and 88,728 residency records, including every empty and failed former-range
query. The observer is frozen at 47936912b9e846a2c4b0aa356d71bea51c7ca519; raw
numeric JSON and timing journals are unchanged. All records and complete
histograms replay offline. An initial launcher count-map rejection occurred
before any codec observation; its failure and restoration are also retained.
Native Linux heap/semantic checks passed all 20 tests; both owned CPU partitions
were restored. The audit branch is never accepted release evidence.

For `encode_stream:10:4:16777216:4194304`, with 4 KiB native pages:

| Actual boundary | Complete observed histogram |
| --- | --- |
| Read scratch before delegated read | 3,600 queries: all 1,025/1,025 pages resident |
| Independent writer after reserve | 288 queries: 4 with 1 page, 39 with 2 pages, 245 with 4,097 pages resident |
| Former read scratch after API return | 720 queries: 324 with 0 pages, 36 with 33, 350 with 1,025; 10 not fully mapped |
| Former parity view after API return | 288 queries: 36 with 0 pages, 35 with 1,025; 217 not fully mapped |
| Former writer after drop | 288 queries: 248 with 4,097 pages; 40 not fully mapped |

These are repeated queries, including EOF and reused buffers, not independent
allocations or unique-page totals. Read buffers are already resident at the
observable read boundary; this does not identify which earlier instruction first
touched each page or exclude initialization costs. Return/release snapshots now
confirm changing page states that virtual syscall ranges alone could not show.
They do not attribute all service-time variance or justify a runtime rewrite.

## Protocol 15 declared before calibration

[The public predeclaration](https://github.com/rustfs/backlog/issues/2897#issuecomment-6270709022)
sets one rule for all 126 cases, based on the observed allocator/page-state
transitions and the prior independent approximately two-second service control:

| Contract | Calibration target | Formal sample floor |
| --- | ---: | ---: |
| Wall interval | 2 s | 1 s |
| Original measured interval | 100 ms | 40 ms |

The worker, controller and offline verifier use the same constants in
`release-sampling/service_window.rs`. Paired-layout protocol 2 shares the sample
floors and continues to use its single frozen count for every cell and phase.
There is no 64-call floor, per-case exception, selected subset, deleted observation
or weakened spread/drift threshold. Calibration still starts at one original
iteration and retains every complete rehearsal/count proposal. All allocation,
copy, release and RNG work, three warmups, nine samples, 10% central spread,
5% same-version repeat drift, full case/layout catalogs and regression gates stay
in force. Protocol-14 observations are preserved and cannot enter protocol 15.

The approximately two-second point control motivates this uniform hypothesis; it
does not establish full-campaign stability. Fresh complete 9.0.0 qualification is
required before candidate attribution. The controller stops after complete A1 if
any distribution fails and stops after A2 if repeat drift fails, retaining all
raw results. Passing qualification still requires final candidate pairing,
complete paired layouts and strict positive release preflight before a tag.

No VerifyWorkspace call-chain change or Leopard owned-output optimization follows
from this capture. Generic ReconstructShard decode-before-publish, late failure
and selective recovery semantics remain unchanged. This is measurement-contract
and diagnostic delivery, not 9.1.0 release approval.
