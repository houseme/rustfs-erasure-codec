# ShardSlot calibration diagnosis

The rejected protocol-6 full baseline campaign recorded two execution regimes at
4,096 iterations of `reconstruct_shard_slot:4:2:65536`: the first calibration
warmup took about 49 ms, subsequent calibration observations about 430 ms, and
all observations in a fresh measurement worker about 48 ms. Exported backend,
features, seed and count matched. Low measuring-thread scheduling wait does not
identify the cause or exclude host effects.

`scripts/diagnose_shard_slot.py` runs the published 9.0.0 baseline at the same
fixed count in four fresh processes: calibrate, sample, sample, calibrate. It
first uses the current shared sampler, then repeats with a separate checked
instrumentation adapter. Under protocol 9 and later the source and codec live for the worker;
the adapter retains six per-iteration clones, missing slots 0/4, and reconstruction.
It does not replace this workload with preallocated/reused slots.

The trace records source and clone lengths, capacities and offsets modulo 64,
4096 and the actual OS base page size. A fixed 128-entry stack histogram counts
clone layouts and transitions without allocating observer storage in the loop.
Overflow is reported explicitly; it cannot appear as a complete inventory.
Every reconstruction checks that existing slot storage retains its complete
pointer identity and becomes present. Full codeword bytes are checked on the
first and last iteration of every observation. Full addresses are never exported. Separate aggregate intervals cover clone/mark-missing, reconstruction, and dropping the cloned slots; the observer checks remain outside these subintervals but inside the enclosing diagnostic timer. They help distinguish adapter allocation costs from reconstruction and cannot establish an allocator or kernel root cause on their own.

Instrumentation and its output perturb execution and allocation history. Its
numbers must not be pooled with the plain arm or used for throughput claims.
Neither arm is adaptive calibration, full release ABBA, proof of actual internal
kernel dispatch, or accepted release evidence. The diagnostic always records
`release_approval=false` and `candidate_attribution=null`. It retains every
warmup/sample, bounded histogram and interrupted numerical journal, and binds the
published source, tool commit, shared renderer, instrumentation and CPU/runtime
context. An incomplete run has no completed diagnosis.

Run only inside the already reserved and audited Linux cpuset partition:

```sh
cargo xtask diagnose-shard-slot \
  --tool /path/to/clean/tool-checkout \
  --repository /path/to/clean/published-9.0.0-checkout \
  --output /path/to/new-diagnostic-directory \
  --isolation-cgroup /sys/fs/cgroup/owned-partition \
  --iterations 4096
```

`cargo xtask diagnose-shard-slot --check` runs the real generated Rust
adapter tests without reserving resources or collecting performance evidence.
The diagnostic adapter itself does not change the formal workload. A subsequent ordinary-telemetry correction is explicitly bound to sampling protocol 8; earlier protocol-7 observations remain historical diagnostics and cannot validate the new observer.
The pending #2897 completion conditions stay unchecked until their whole scope
has actual acceptance evidence.

## Ordinary observer correction

The protocol-7 investigation found Rust `read_to_string` growth in the procfs
observer reaching glibc `realloc` and `madvise(MADV_DONTNEED)`. Independent
four-worker syscall counts found 45,073 `madvise` calls per worker; bounded
call stacks also locate repeated page returns under the benchmark's `free`.
Instrumented subintervals place most of the additional time in cloning and
freeing buffers, rather than reconstruction. These findings do not uniquely
attribute every historical regression or establish a candidate speedup.

Protocol 8 replaces ordinary stat/schedstat reads with bounded stack storage,
retries interrupted reads and refuses truncation. A separate check executable
counts alloc/realloc/dealloc on the observing Linux thread; its test allocator
is never linked into formal capture. Optional extended process diagnostics keep
their distinct observer contract. No allocator tunable, production algorithm,
codeword format, timing threshold or original per-iteration clone policy changes.
Fresh baseline controls and the complete release campaign are still required.

Protocol 9 extends retained caller fixtures to all small-file/stream release cases.
The current tracer follows its worker-lived source; historical observation-lived
traces remain bound to their earlier schemas and archived controllers. See
[caller fixture contracts](release-caller-fixtures.md) for the exact scope and
untracked dynamic destinations. None of these diagnostic schemas approves release.
