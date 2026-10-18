# Complete ShardSlot observer-state diagnosis

These four within-version diagnostic stages preserve all 288 observations (216
measured observations plus all warmups) of the published 9.0.0
`reconstruct_shard_slot:4:2:65536` case at a fixed 4,096 iterations. They are not
adaptive calibration, candidate comparisons, the 126-case release campaign, or
accepted evidence. Every dataset has `release_approval=false` and null candidate
attribution. No successful subset was selected for release.

- `layout`: eight fresh workers, plain then traced, each in calibrate/sample/
  sample/calibrate order. Both modes reproduce a fast first warmup and a much
  slower subsequent state. All 196,608 traced clone iterations are counted with
  zero histogram overflow. Layout is stable within each observation and differs
  between the first observation and subsequent observations.
- `phase-intervals`: all eight workers and another 196,608 traced clone iterations.
  Median diagnostic clone/drop intervals total roughly 397–406 ms per observation,
  versus about 22 ms in reconstruction. The first warmup's clone/drop intervals
  total roughly 14–20 ms. Instrumentation affects timing and allocator history;
  these numbers cannot be used as candidate speedup estimates.
- `syscall-counts`: four complete fresh plain workers under strace. Every worker
  records 45,073 madvise calls. Counts include startup/export/cleanup and cannot
  alone identify all callers or establish memory-path performance.
- `bounded-stacks`: four complete workers with a predeclared limit of the first
  64 madvise calls and at most eight frames per event. All 256 captured events
  are retained in explicitly derived, address-free JSON. They locate observer
  `read_to_string`/realloc and benchmark free paths calling MADV_DONTNEED. Some
  static libc routines resolve only to the nearest exported symbol. The bounded
  trace is not a full-call inventory, and timings are perturbed by tracing.

All copied numerical sampler/provenance JSON, syscall summaries and cleanup
results remain byte-identical to their originals. Derived analysis and stack
exports are named and labeled as derived. Stack export removes full addresses,
process IDs, object paths and instruction offsets, normalizes the sampler object
name, and preserves every captured call's order, length, advice, result and
symbolized frames. Raw address-bearing traces, build/launcher logs and private
resource-management files remain local. No codeword data or credentials appear.

The matched controllers are retained in `controllers`; source bindings identify
be43a48 / 13f30d5 and shared sampling protocol 7. The later protocol-8 fixed-stack
observer correction requires entirely new baseline controls and release evidence.
All four stages restored their owned CPU partition and parent affinities.
