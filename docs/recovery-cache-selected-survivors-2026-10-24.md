# Recovery inverse reuse across unselected parity erasures

Runtime `b0bd342` fixes duplicate Classic inverse construction, based on
`cdc4a05`. Integrated candidate `d138103` includes main `1be185d` and its native
Rust controllers. The [source-bound review](../benchmarks/release-9.1.0/2026-10-24/recovery-cache-selected-survivors-review.json)
separates each executed source, deterministic resource result and pending native
validation. The earlier `544bb06` native proof does not qualify this candidate.

## Cache identity and compatibility

For a fixed codec with k data shards, reconstruction selects the first k
surviving inputs in shard order. The old inverse cache used every missing shard
as its key. Erasing an additional parity shard after the last selected input
therefore rebuilt the same inverse and stored another entry.

The new key borrows only the sorted erasure prefix before the last selected
survivor. That prefix uniquely determines the same first k inputs. Both lookup
and insertion use it. A pattern without trailing erasures keeps its original
borrowed slice. Full erasure layouts still validate quorum and shard lengths,
match reusable workspaces, select requested outputs and control publication.
Changed selected input rows use a different key. The existing inverse builder,
mutex, immutable Arc, LRU capacity and disabled-cache behavior remain in place.
Cache statistics intentionally recognize hits across these equivalent inputs.

GF8/GF16 tests compare every result against full matrix elimination for all 163
small layouts in each of three matrix modes (978 layouts). Complete selected-input
keys independently check pointer reuse and separation. A warmed nonsingular
custom entry must not conceal a singular replacement; failed matrices are not
cached. Explicitly disabled caches continue to construct separate inverses.

The original focused regressions fail against `cdc4a05`: GF8 and GF16 do not
share the inverse, and preallocated GF8 32+16 recovery requests three allocations
when only unused trailing parity changes. After the fix that case requests zero.
The final broader test also verifies zero allocations for 18 combinations of two
fields, three matrix modes and 4+4 / 10+4 / 32+16 shapes with 65-element shards.
Missing unrequested parity buffers are poisoned and must remain untouched and
unpublished. These are warmed caller-thread allocation counts, not throughput,
RSS, page-residency or exclusive API-cycle measurements.

## Validation and diagnostic compatibility

Executed local source `5007f4c` passes 392 library, 163 integration and ten doc
parent tests (565 total, 43 existing ignored), with six filtered child executions
recorded separately, plus 264 no_std tests. Published 9.0.0 comparisons pass 200
normal configurations, 27 custom configurations / 162 cross-recoveries, 12 wide
GF16 configurations and 270 selective configurations / 2,640 cross-recoveries on
the local default backend. Codec/test/build/compatibility bytes are identical in
integrated candidate `d138103`.

The integrated native Rust tool suite passes 271 unit and 39 CLI tests (310).
Strict codec/tool Clippy, x86 all-target/all-feature compile-only Clippy,
formatting, typos and whitespace checks pass. The pre-migration `9f104da` tool
source also passed all 252 Python tests; that historical result is not presented
as the current Rust controller suite.

The read-only dynamic observer recognizes exactly one of the legacy complete-key
or new prefix-key lookup anchors. Unknown and ambiguous bodies refuse. Removing
only injected observation lines restores every production statement. Independent
comparison checks all nine generated files: legacy inputs retain their original
bytes; only the current production cache file changes, and reversing only that
production change restores the complete old output. The two current-source
snapshots are updated after that comparison. Formal Rust sampling loops, counts
and benchmark source files remain unchanged.

## Native and release status

The 31-command native plan binds `d138103`. Resource admission has returned
waiting without launching a codec workload because another task holds CPU
partitions. It must acquire the existing shared nonblocking lease and maintain
continuous resource observation; existing owners are not interrupted. The earlier
prepared `9f104da` plan, its zero-start wait and all frozen inputs remain retained
as superseded before any workload. Final acknowledgement uses a complete
validated temporary upload followed by atomic rename.

This narrower cache relation does not attribute earlier same-pattern timing
drift or process-wide page faults; those investigations retain their own scope.
Current package inventories are 71 core / six WASM members. This change provides
a verified reduction in duplicate inverse work and allocations. Accepted
end-to-end throughput, final source/tool/lock freezing, complete accepted
published-9.0.0 ABBA/paired-layout evidence and an actual strict positive release
preflight remain outstanding. All historical failed campaigns and the complete
72-case repeatability refusal retain their sources and outcomes. No tag,
registry upload or release approval is produced.
