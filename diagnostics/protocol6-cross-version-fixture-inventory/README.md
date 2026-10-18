# Complete cross-version structural inventories: protocol 5 and protocol 6

These are one-iteration layout inventories, not performance measurements or release evidence. Each inventory retains all eleven buffered-verification Vec cases in A1/B1/B2/A2 order, three warmups and nine observations per case/phase: 528 raw observations. Neither timing estimates nor candidate speedups are approved. `release_approval` is always false.

`before` uses published 9.0.0 and main `bc3a3dcb6e7c1a7a2a8a3a612dbef2d742ed7b09`, sampling protocol 5. Same-version repetitions have matching layouts, but six of eleven cases differ across the versions: 24 of the 66 complete phase-pair checks mismatch. The original standalone controller is retained with its exact provenance hash.

`after` uses published 9.0.0 and candidate `d3cb05d4299ae2604325af38be2b29e1b51ac0cd` (PR #17), protocol 6. Caller buffers are allocated before codec setup. The runtime and every retained Vec fixture bind the actual 4096-byte OS base page size, along with fixed 64/4096 offsets and actual base-page offsets. All 66 layout checks match. The committed controller and complete timing/sample journals are retained without numerical edits. No allocator is replaced and no address forced.

`initial-setup-stop` preserves restoration outcomes from an earlier source-bundle import failure. The sampling modules did not load; no workload observations were produced. The successful before capture was restarted in a fresh directory. This is an initialization failure, not a codec or performance result.

Every copied JSON/TSV member and controller is byte-for-byte unchanged. All phase records, including warmups, partial snapshots and before mismatches, are present. Ancillary build/console logs and resource-management scripts contain internal paths and remain private; there are no credentials, full virtual addresses or real user objects in this snapshot. Both completed inventories restored their exclusive guest CPU partitions and controller state.

This only establishes recorded structural comparability for the eleven known Vec cases. Other dynamic allocation paths, complete per-layout performance ABBAs, the full original 126-case release campaign and positive strict preflight remain outstanding in backlog #2897. The earlier unstable layout-grid distribution remains in the previous snapshot and is not invalidated or promoted by these results.
