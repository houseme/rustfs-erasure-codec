# Same-process parallel recovery follow-up

The user indicated that only the previously authorized Azure VM is available.
The runtime source is commit `fc6795c` plus the worktree snapshot in
[`../2026-10-31-followup/baseline-worktree.patch`](../2026-10-31-followup/baseline-worktree.patch).
The later `268e092` commit changes only GF16 experiment artifacts. The candidate
is [`../2026-10-31-parallel-followup/parallel-row-candidate.patch`](../2026-10-31-parallel-followup/parallel-row-candidate.patch),
with test-only instrumentation excluded automatically from release builds.

The host is an AMD EPYC 9V45 Azure VM, physically reserved as confirmed by the
user. All measured work used stable rustc 1.99.1, `std simd-avx2`, five guest
vCPUs, a maximum of four codec jobs and four Rayon workers. The input is
1 MiB per shard. Both probes restore missing `Option<Vec<u8>>` data using
`reconstruct_data_opt`; allocation is part of the measured API workload.
The monitors sampled guest cpuset, named competing processes and vCPU counters
every 200 ms. This cannot exclude shorter interruptions or unobservable host
frequency changes.

## Baseline-only diagnostics

`baseline_probe.rs` executes four phases of the same implementation in one
process, with 20 warmup operations per phase and three 300 ms samples. Two
separate baseline-only runs (`sameproc-aa-{1,2}.jsonl`) passed 5% phase drift
and same-code bias gates for 10+4 with one/two missing data shards and 4+2 with
one missing data shard.

`paired_probe.rs` uses two same-source dependency aliases so its code layout
more closely matches a baseline/candidate executable. The second package has a
temporary version change for linking. Its first A/A preflight
(`paired-row-aa-1.jsonl`) failed: the 10+4/two-missing A1/A2 drift was -7.65%
and the same-source difference -5.66%. The full failed run remains invalid;
no candidate attribution came from it.

The first A1 samples fell from roughly 160 to 156 microseconds while later
phases were roughly 144-146 microseconds. Insufficient 20-operation warmup is a
plausible contributor, not a demonstrated sole cause. `paired_probe_warm.rs`
uses a one-second warmup for every phase, three 300 ms samples, and verifies
the complete shard set after each phase. Two new A/A preflights
(`paired-row-warm-aa-{1,2}.jsonl`) passed both 5% gates. The earlier
`unchecked-paired-row-warm-aa-*` files used the same timed warmup but checked
shards only at the end; they are retained as separate diagnostics.

## Parallel row candidate

The independent `paired-row-warm-candidate-1.jsonl` A/B/B/A run passed the
phase drift gate after those two preflights. In the measured four-job 10+4,
1 MiB, one-missing case, baseline phase medians were 75.92 and 75.79 µs; the
candidate phase medians were 80.41 and 80.47 µs, a +6.05% latency change.
Two-missing and 4+2 controls changed -0.72% and +0.31%. Those controls cannot
select the candidate kernel. The one-missing case was the only candidate
activation target. This is a single-host, warmed microbenchmark, not a claim
about other CPUs or storage-level performance. It is sufficient reason not to
enable this implementation for the tested workload.

The A/A and candidate monitor files recorded cpuset 0-15 throughout, no named
competing benchmark, and no increase in steal counters on vCPUs 8-12. This
observation does not rule out sub-sample interruptions or unobservable host
frequency variation. The benchmark compares different linked crate copies;
same-source bias checks bound, but do not eliminate, code-layout effects.

Separate short `perf record -e cpu-clock` profiles captured about one thousand
samples each (`row-baseline-profile.jsonl` and `row-candidate-profile.jsonl`
are the probe outputs, not the perf samples). Baseline samples showed about
58% in AVX2 multiply/multiply-add symbols; candidate samples showed about 58%
in the fused row symbol. Scheduler-related samples were also substantial.
Profiles were collected in separate runs and are sampling evidence only, not
causal proof of the measured regression. Static inspection shows the existing
coefficient-streaming kernel broadcasts tables once per coefficient/chunk,
whereas the 128-byte fused row loop broadcasts tables again per input per
128-byte tile; this is a hypothesis for the overhead tradeoff.

## 256-byte tile experiment

`row-256-byte-experiment.patch` applies on top of the instrumented parallel
candidate. It uses eight AVX2 accumulators per 256-byte tile to reduce table
broadcast frequency. Native AVX2 scalar-row comparison, one/four-job dispatch
and custom-matrix recovery tests passed. Disassembly of the tested release
binary showed 13 stack-relative vector instructions, all stores associated with
term-array initialization; no vector register load from the stack appeared in
the function. This does not prove the absence of other bottlenecks.

The fresh baseline preflight (`paired-row8-warm-aa-1.jsonl`) failed: the 4+2
control A1/A2 drift was +6.66%, so the preflight stopped and no 256-byte
candidate timing was run. The monitor observed cpuset 0-15 and no named
competitor; the cause was not established. Do not infer an improvement or
repeat a candidate comparison until the baseline variability is controlled.

The 256-byte variant's all-features library suite passed 448 tests with five
ignored. An initial all-target Clippy run found three identity operations in
the experimental unroll; those were simplified, then the direct AVX2 scalar
comparison, all-features/all-targets Clippy with warnings denied, and AVX2-only
no_std `cargo check` passed. The stored patch includes that cleanup. No
candidate performance comparison was performed after the failed preflight.

The separate `row-256-byte-with-unaligned-test.patch` retains the same 256-byte
experiment and adds a native AVX2 chunk test for start offsets 0/17/31,
lengths spanning vector and 128/256/512-byte tails, zero/one/mixed coefficients,
non-aligned input and output slices, and output guard bytes. The focused scalar
comparison and AVX2-only all-target Clippy passed. The original
`row-256-byte-experiment.patch` is unchanged to preserve the checksum of earlier
records. This additional correctness test is not performance evidence.
