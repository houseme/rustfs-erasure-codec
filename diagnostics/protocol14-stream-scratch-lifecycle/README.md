# Stream scratch page return and service-window audit

Audit data only. Never merge this data branch into main, configure it as
`RSE_RELEASE_EVIDENCE_REF`, or treat a controller's capture-completion status as
release/profile approval. Published 9.0.0 is the only measured codec. The capture
tool is frozen main `aaebcb3835d6767fce44e4fbdbd3389664729713`; offline auditors
are bound separately to code commit `968a423` and their exact file hashes.

All **84 original observations** and timing journals are retained without numerical
edits: 24 FP profiles, 24 trace/small-DWARF observations, 12 rejected full-DWARF
observations and 24 longer-window control observations. At six calls per profiled
observation and 64 per control observation, these contain 1,896 original API calls.
They are separate diagnostic methods/contexts, not one timing pool or a candidate
performance comparison.

## What is established

The stream workload retains original independent 16 MiB caller writers and the
library's ten read/four parity buffers at 4 MiB blocks. Allocation, initialization,
copying and frees stay in their original scopes. No production codec, format,
VerifyWorkspace, Leopard output or generic reconstruction semantics changed.

The address-free syscall ledger binds the one task writing every actual sampler
marker. It records 416 VM syscalls, including 72 anonymous 128 MiB and 36 anonymous
64 MiB mappings initially requesting PROT_NONE, plus 36 successful 43.875 MiB
MADV_DONTNEED requests. The measured task has no brk growth. Partial munmaps and
MAP_FIXED replacement are tracked using opaque mapping IDs and offsets; addresses
and task IDs are absent. These are virtual-range operations, not physical RSS or
proof of residency/individual Vec allocation ownership. mprotect is not observed.

The protocol emits twelve immediate timing journals and only then twelve deferred
samples. Ledger intervals run between timing writes; boundary telemetry/journal
costs remain, and the first interval includes startup. They are not exact API
entry/exit intervals. Every marker is checked against the original numeric JSON.

The complete CPU profile contains 2,730 samples, 873 unresolved leaves and zero
lost/throttle records. Positive context assigns 299 read-path, 217 write-path and
106 encode-cleanup samples, the latter including kernel unmap/zap/free-pages
frames. All other samples and every actual period remain. These are whole-worker
samples, including setup/warmups/export, not pure kernel-loop CPU percentages or
expected throughput savings.

## What is rejected or still incomplete

- FP fault capture contains 857,298 events without loss/throttle. Immediate libc
  callers can be skipped. It supports path context, not exact read/parity zeroing
  attribution. Some non-core project source names from the earlier decoder remain
  basenames; ambiguous names are not used for a positive source-role assignment.
- The 512-byte DWARF capture has 857,293 events without loss/throttle, but 855,247
  lack an expandable callchain. They remain explicit unknowns, not filtered data.
- Compressed full DWARF exits zero despite EFAULT writes and compressed-data
  corruption. `full_dwarf_rejected/quality.json` rejects it. Its twelve numeric
  observations and capture metadata remain; no usable stack-event count is
  invented and no further unchanged profiler retry follows.
- The VM accepted requested fault periods but emitted actual period 1. Aggregates
  use recorded periods, never the requested 1,009 as a multiplier.
- Raw perf maps/data, syscall addresses, binary bindings and private launcher/build
  logs stay private. All original scalar sampler observations/journals and complete
  address-free event/stack accounting are here. The absent raw address data cannot
  be reconstructed from this public snapshot.

`capture-result.original.json` files are retained as originally written. In
particular, `capture_complete` or a controller's `collected-diagnostic-only` status
means that the worker/capture command finished, not that the perf data are usable.
The separate explicit quality result takes precedence. Original raw record-count
summaries also omitted MMAP2 due to a text-regex boundary; complete native aggregate
stats/counts are supplied separately and their totals conserve every record type.
No scalar benchmark observation was changed by that correction.

## Separate longer-window control

The unprofiled `service64` campaign predeclared exactly 64 calls per observation,
three warmups/nine samples, default build flags and default allocator. Both workers
are fresh processes; no perf/strace/view adapter or allocator override is active.
Caller fixture/workload/placement/backend contracts match. Widths are 3.391681%
and 3.461520%; same-version median drift is 1.621819%, within the existing 10%/5%
point-control limits. All 24 observations / 1,536 calls remain.

This tests a longer original service window, not a speedup relative to six calls.
The original formal calibration/protocol/count is unchanged. Adjacent point
controls do not establish full-campaign epoch stability, all 126 cases, candidate
regression acceptance, all dynamic layouts or the complete 16-layout supplement.
It supports investigating finite-window mixing of allocator page states before
runtime edits; it does not by itself prove the unique cause of every failure.

## Reproduction and restoration

Use the bound analyzer checkout, with published baseline source object available:

```sh
python3 replay.py --tool /path/to/codec-checkout
```

Replay is read-only: it verifies all 84 records/journals, source-bound role counts,
actual periods, unknowns, native record statistics, VM group accounting and the
scoped 64-call repeat control. Decode of raw addresses remains a private operation.

`checks-and-restoration.json` records all four campaigns independently: 188, 8, 5
and 65 isolation checks respectively. All owned partitions were removed and the
controllers/original CPU ranges restored. VM partition ownership does not prove
physical-host exclusivity. Backlog #2897 remains open; 1/2/3/5 partial, 4 only in its
normal-format scope, 6 unaccepted. No accepted manifest, tag or upload follows.
