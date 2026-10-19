# Post-construction CPU placement diagnosis

This baseline-only diagnostic follows the protocol-10 controls whose same-version
repeat drift failed even with identical CPU endpoints. It never approves release
performance or attributes a candidate speedup. It preserves the original ordinary
`encode:10:4:524288` kernel at a fixed 384 iterations; each epoch has three warmups
and nine samples. It is a scoped causal diagnostic, not a replacement for all 126
release cases or the separate 16-layout acceptance.

`diagnose_cpu_placement.py` generates an external overlay on unchanged published
9.0.0. It reuses `PreparedSmallFile` and the original extracted execution loop.
The codec is constructed with the entire audited reservation available, before any
pin. Construction availability is recorded on both sides. The cached decision for
an explicit parallel API is checked before/after each pin; this hypothetical
policy is **not** evidence that ordinary encode uses Rayon or proof of actual
internal dispatch. No production or formal sampler source is modified.

At each READY boundary the parent verifies the measuring TID belongs to its newly
created worker session, verifies its expected previous affinity, applies a singleton
CPU mask, reads it back, and acknowledges one byte. At END it checks actual affinity
again and requires every observation. The worker independently checks the actual
CPU in bounded procfs snapshots and the pinned available parallelism. Raw READY
logs contain the private TID; public gate records omit it. No unrelated thread or
service is stopped or repinned.

The declared worker plan covers every reserved CPU:

1. One retained source/codec across forward then reverse CPU order.
2. A separate retained source/codec across reverse then forward order.
3. One fresh process per CPU in two complete forward/reverse waves.

Private full-pointer identity and exported caller geometry are checked throughout
each worker; all parity is updated in place and the final codeword is verified.
Source allocation/first-touch CPU before pinning is not controlled. Separate workers
do not share physical pages, and identical offsets do not prove identical physical
or cache placement. All source/scratch layout, page-fault and scheduler observations
are retained; missing, reordered, escaped or changed-fixture records are rejected.
Short/wide distributions and repeat drift remain visible, never filtered to a
stable subset. The basic scheduling counters bracket the wall timer and are not
proof of whole-process or host isolation.

Run only inside the owned audited Linux CPU partition, from a clean committed tool
and baseline checkout:

```sh
python3 tool/scripts/diagnose_cpu_placement.py \
  --tool /path/to/tool --repository /path/to/published-9.0.0 \
  --output /path/to/new-capture --isolation-cgroup /sys/fs/cgroup/owned-partition \
  --iterations 384
```

Allocator environment is explicit in provenance. The controller may independently
run the predeclared default and retained-arena contexts; their results do not
establish a cross-policy speedup. Every completed worker and interrupted numeric
journal is preserved. Cleanup removes only byte-identical generated inputs and
retains unexpected files for inspection.

`python3 scripts/diagnose_cpu_placement.py --check` compiles/tests the real overlay.
Only this check overlay links the test allocation counter; normal diagnosis and
formal sampling do not. Linux tests count zero allocation/reallocation/free for
sixteen real fault-counter reads. This does not claim the entire diagnostic or
stdout observer is cost-free. Shared cleanup and the handshake have refusal tests.
