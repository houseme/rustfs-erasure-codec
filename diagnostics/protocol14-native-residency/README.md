# Native stream residency — diagnostic only

All 15 published-9.0.0 stream cases, six original calls per observation, three
warmups and nine samples are retained without numeric changes: 180 observations,
1,080 API calls and 88,728 address-free residency records. `records.zip` contains
the original JSON members, full journals, source/runtime/protocol bindings and all
former-range query failures. The frozen observer is 47936912b9e846a2c4b0aa356d71bea51c7ca519;
these are diagnostic protocol-14 observations, never protocol-15 formal samples.

`mincore` queries virtual-page snapshots without touching/prefaulting the buffers.
Mapped results are not RSS, physical ownership or allocation IDs. Partial boundary
pages can include adjacent objects. ENOMEM is `not_fully_mapped`, with unknown
resident count; it does not mean every page is unmapped. Former ranges may already
be reused. The adapter/queries/locking perturb the measured process; no timing,
throughput, exact first-fault instruction or full-campaign equivalence follows.

`summary.json` retains complete histograms. `checks-and-restoration.json` records
both the rejected pre-observation count-map launch and the repaired capture. All
native heap checks passed and both owned partitions were restored. Private raw
launcher/build materials remain local. No binary hashes, paths, addresses or IDs
are exported. Source/tool/lock/compiler/CPU bindings and every scalar observation
are public; release approval and candidate attribution remain false/absent.

Replay read-only with the frozen observer checkout (or an unchanged native-audit
implementation): `python3 replay.py --tool /path/to/codec-checkout`. The replay
checks checksums, all 15 cases, every observation/journal/boundary and complete
histograms. Do not merge this data branch or configure it as accepted release
performance evidence.
