# ShardSlot calibration diagnosis

The rejected protocol-6 full baseline campaign recorded two execution regimes at
4,096 iterations of `reconstruct_shard_slot:4:2:65536`: the first calibration
warmup took about 49 ms, subsequent calibration observations about 430 ms, and
all observations in a fresh measurement worker about 48 ms. Exported backend,
features, seed and count matched. Low measuring-thread scheduling wait does not
identify the cause or exclude host effects.

`scripts/diagnose_shard_slot.py` runs the published 9.0.0 baseline at the same
fixed count in four fresh processes: calibrate, sample, sample, calibrate. It
first uses the unmodified shared sampler, then repeats with a separate checked
instrumentation adapter. The adapter retains the original Vec source creation
per observation, six per-iteration clones, missing slots 0/4, and reconstruction.
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
python3 scripts/diagnose_shard_slot.py \
  --tool /path/to/clean/tool-checkout \
  --repository /path/to/clean/published-9.0.0-checkout \
  --output /path/to/new-diagnostic-directory \
  --isolation-cgroup /sys/fs/cgroup/owned-partition \
  --iterations 4096
```

`python3 scripts/diagnose_shard_slot.py --check` runs the real generated Rust
adapter tests without reserving resources or collecting performance evidence.
The formal sampler protocol and workload remain unchanged by this diagnostic.
The pending #2897 completion conditions stay unchecked until their whole scope
has actual acceptance evidence.
