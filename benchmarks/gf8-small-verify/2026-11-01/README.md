# Small-shard Classic verification investigation

Sizes below are **per-shard elements**, not logical object bytes. The focused
release test in `tests/parallel_verify_workspace.rs` observes allocations on
the calling thread after warmup, checks valid parity and corruption in both
parity rows at the first and last element, and covers Vandermonde, Cauchy,
Jerasure-like and custom matrices. It covers GF8, GF16 and a custom three-byte
field. A retained caller buffer is checked separately; no timing is inferred
from allocator request counts.

On the local Apple Silicon build, ordinary 4+2 GF8 verification made no heap
request at 2,047 or 2,048 elements per shard, then made one 4,098-byte request
at 2,049 and one 8,192-byte request at 4,096. GF16 showed the same element
boundary, with requests of 8,196 and 16,384 bytes respectively. The warmed
caller-buffer path made no calling-thread requests in the observed cases.
These observations do not by themselves establish that allocation dominates
latency or that the same request sizes apply to all builds or allocators.

`candidate.patch` is an **unapplied experiment** that checks parity rows one at
a time using one inline scratch row for lengths within the inline limit. In a
local disposable source copy at `5f93c16`, the focused GF8/GF16 allocation and
corruption checks removed the measured request. It can change generated-kernel
selection, repeat input work and short-circuit after a mismatching row; resource
savings therefore do not prove a throughput gain or full compatibility.
`candidate-single-scratch.patch` is a later local variant that keeps one scratch
object for both branches to avoid introducing a second inline scratch object.
Its local release integration suite passed all eight tests, including the
expanded GF8/GF16/custom-field and matrix checks, and no-default-features build
passed. Neither patch is active in production or accepted for performance.

An additional disposable build applied `candidate-single-scratch.patch` and
`candidate-interop-counter.patch`, together with the expanded release
compatibility probe. On Apple Silicon, 32 GF8/GF16 small verification cases
matched published 9.0.0 parity and rejected corruption in both directions.
The counter observed 208 candidate-path entries during the new version's
initial small-case pass; it is test-only and makes no performance claim. The
counter patch assumes the expanded compatibility probe is already present.

`tests/small_verify_nostd.rs` additionally checks GF8 and GF16 with no
default features across four matrix modes and 2,048/2,049/4,096/4,097-element
boundaries, including corruption and late length errors. Both current code and
the instrumented single-scratch candidate passed locally on Apple Silicon.
The test also cross-compiled for x86 Linux with AVX2 and AVX-512 feature
selections; cross-compilation does not establish native execution.

For native x86, `paired_probe.rs` uses the automatic SIMD backend, one visible
CPU and one Rayon thread. It shares valid and corrupted input buffers, checks
both codec copies, and tests 4+2 and 10+4 at 1,024, 2,048, 2,049, 4,096 and
4,097 elements. `runner-small-cases.patch` applies after the previously
validated cross-arm gate to the archived streaming runner; it changes cases and
single-CPU policy, and monitors fourteen off-target CPUs at the same half-core
aggregate threshold. The candidate tested by this setup would have been the
first patch, not the later single-scratch variant.

The `results/` preflight used committed baseline
`dbb6e670ab9604d4011bd18b6732ed1356cd8d7f`; the two copies differed only by a
temporary package version. It stopped at 4+2 / 2,049 elements because the
same-source A/B level bias was -5.80%, outside the 5% limit. The active backend
was GFNI AVX-512; 112 monitor observations retained CPU set 0–15, no named
competitor and no off-target busy streak. No candidate ran. Layout, timing and
other sources of this bias were not distinguished; do not derive a candidate
performance conclusion.

`single_copy_probe.rs` is a materially different baseline-only diagnostic: A
and B call the same non-inlined function and codec instance. The native binary
contained one matching function symbol. `single-results/` stopped on a
resource-monitor rejection after one completed 4+2 / 1,024-element case;
65 monitor observations recorded an active named competitor in one sample.
The earlier 2,049-element bias case was not reached, and no candidate ran.
This does not show whether the bias is resolved. Before considering adoption,
use a stable, provenance-matched comparison and validate both ordinary and
parallel verification, custom fields, stack usage, feature combinations and
small-size regressions. The original same-source gate must not be weakened.

## Refreshed same-function control

A later baseline-only attempt rebuilt both copies from committed source
`a76dd2ef66bd2392847c8ff0fc449f7df50e2ae7` and retained the same non-inlined
common function. Its native binary again contained one matching symbol. The
first resource-monitor observation reported CPUs 0–15 but an active named
competitor, so the runner refused to start. `continuation-a76/results/`
contains the one observation and failure summary; no baseline or candidate
case was recorded. The 2,049-element bias therefore remains unexplained.

## Same-function baseline on refreshed source

A later baseline-only attempt rebuilt the same non-inlined-function probe from
committed source `616f5001a28873076a3eb91323b9775f0794cd8b`. The first round
completed all ten cases. For 4+2 / 2,049 elements, its A/B level difference
was approximately +0.01%, but the second round did not reach that case: its
first 4+2 / 1,024-element case showed -16.28% A-level cross-round drift and the
runner stopped. A post-hoc diagnostic of the recorded case also found -16.38%
B-level drift. The whole two-round preflight is rejected and no candidate ran.

`single-616f500/results-616f500/` preserves the original samples and monitor.
Across 403 observations, CPUs remained 0–15, no named competitor was seen and
the maximum off-target busy streak was one. This cannot exclude short
interference or unobservable host effects. The within-round result and the
prior dual-copy bias came from different builds and call structures; they do
not establish the cause of the earlier bias or a stable setup. No performance
conclusion or production activation follows.

## Address-randomization diagnostic interruption

A later baseline-only diagnostic rebuilt the same-function probe from committed
source `738fe71163eaab8f4df7b37a74242a386bb7232c` and launched it with
`setarch x86_64 -R` for the probe process. This changes the execution
configuration and cannot serve as production performance acceptance. The
resource monitor rejected the run before the first case completed: among 40
observations the effective CPU set changed from 0–15 to 0–7 and a named
competitor was seen in one observation. `no-aslr-738fe71/` retains the empty
baseline output and original monitor/summary. No candidate ran and the attempt
provides no evidence about whether address randomization caused earlier drift.

## Refreshed address-randomization control

A baseline-only address-randomization diagnostic was rebuilt from committed
source `bdd9a0df43a64fde35571d30158efd03903de45a`, including the newer GF16
runtime changes. The same non-inlined verification function was used for A and
B; the probe ran with `setarch x86_64 -R`, so this is not a production-default
performance acceptance run.

The runner completed six cases in the first round, then stopped when its
resource monitor detected an active named competing process. Across 230
observations the effective CPU set remained 0–15, the named-process check was
active once and the maximum off-target busy streak was one. The monitor did not
record the competitor's identity. `no-aslr-bdd9a0d/` retains the original
partial output, monitoring and failure summary. The remaining first-round cases,
the entire second round and all candidate timing were not completed. Therefore
this attempt cannot establish cross-round stability, identify the cause of the
prior drift or support a performance conclusion.

## Exact candidate dispatch boundaries

`candidate-dispatch-test.rs` is a test artifact for the unapplied
single-scratch candidate, not a production integration test. In a disposable
copy of committed source `734530705d4665cc481d986d6d6d49805611fe8e`, it
checks the test-only atomic counter separately for every call. The 216 cases
cover GF8, GF16 and a three-byte custom field, four matrix modes, 4+2 around
2,048 elements, 10+4 around 1,024 elements, 4+1 with and without fast parity,
and the 4,096/4,097 per-row boundary. It verifies that only the intended
ordinary `verify` calls enter the experiment, caller-buffer verification does
not, corruption of every shard is detected, and a later invalid shard length
is rejected before entering the candidate path or changing inputs.

On Apple Silicon, release tests passed with default features and separately
with `--no-default-features`, selecting `-p rustfs-erasure-codec` explicitly to
avoid enabling `std` through other workspace packages. The counter is
instrumentation, so these runs provide no timing evidence. Native x86 execution
and stable performance acceptance remain outstanding.

To reproduce on a clean disposable copy, apply `candidate-single-scratch.patch`,
then apply only the `src/lib.rs` and `src/core/verify.rs` parts of
`candidate-interop-counter.patch` with `git apply --include`; copy
`candidate-dispatch-test.rs` to `tests/candidate_dispatch.rs` and run
`cargo test --release -p rustfs-erasure-codec --test candidate_dispatch`,
repeating with `--no-default-features`. Do not ship the counter.

## Scratch allocation and stack code generation

A source-level assumption that a single `SmallVec` would reduce stack usage did
not hold in the tested compiler output. `stack-screen/` contains the isolated
probe, lockfile, compiler version, assembly prologues and allocation
observations. The exact source base was `e009ee74f4c0f35568bc38ac2b0d1338573c75c7`;
all variants used the same source path, Rust 1.99.1 release optimization and
no explicit SIMD features. The Apple target was compiled locally and the Linux
x86 target was cross-compiled. Default features and the codec's
`--no-default-features` produced identical figures for the four variants shown below.

The table is **only the emitted wrapper's static stack frame**, including saved
registers and excluding the x86 return address. The wrapper monomorphizes
ordinary `verify` for a `Vec`-of-`Vec` caller. It does not include its callers,
outlined callees, dynamic stack, other monomorphizations or runtime peak use.
Sizes in bytes, GF8 / tower GF16 / custom 3-byte field:

| Variant | AArch64 Darwin | x86-64 Linux cross-compiled |
| --- | --- | --- |
| Current baseline | 8,976 / 17,168 / 25,328 | 8,952 / 17,160 / 25,304 |
| Two syntactic scratch values (`candidate.patch`) | 4,864 / 8,960 / 13,024 | 4,840 / 8,936 / 13,000 |
| Single `with_capacity` (`candidate-single-scratch.patch`) | 8,464 / 16,640 / 24,800 | 8,440 / 16,632 / 24,792 |
| Single `new` plus exact reservation (`candidate-single-exact.patch`) | 4,880 / 8,960 / 13,024 | 4,856 / 8,952 / 13,016 |

The two-scratch wrapper's smaller figure is not necessarily a smaller call
chain: for example, its outlined x86 `SmallVec::with_capacity` functions have
additional 4,120 / 8,216 / 12,312-byte frames. Other calls may also add stack.
Do not infer peak stack or latency from this table; compiler versions, features,
link-time optimization and callers can change code generation.

A naive single-`SmallVec::new()` prototype is retained as
`candidate-single-new.patch` for reproducibility, but **rejected**: with 4+1
fast parity at 4,097 elements it requested 8,192 bytes for GF8 and 16,384 for
GF16, versus 4,097 and 8,194 for the existing parallel path. The
`fast_single_parity_verification_reuses_the_classic_inline_scratch_budget`
resource test failed as intended. Three repeated allocation observations are in
`stack-screen/resource-observations.txt`; request sizes are not latency or RSS.

The exact-reservation experiment reserves only when the selected capacity
exceeds the inline limit. On Apple Silicon its same resource test passed with
4,097/8,194-byte requests, all nine `parallel_verify_workspace` tests passed,
and instrumented dispatch tests passed in default and no-default-features
builds. The uninstrumented variant also passed the no-default-features small
verification test and published 9.0.0 compatibility probe (200 general, 36
large GF8, 12 6+2 and 32 small-verification cases). The instrumentation is not
included in either candidate patch. These are functional and resource checks;
the exact-reservation variant is **unapplied and not performance accepted**.
Native x86 stack high-water measurement, end-to-end timing and stable
baseline A/B/B/A remain outstanding.

To reproduce code generation, create sibling `codec` and `probe` directories,
check out or archive the stated source into `codec`, copy `stack-screen/probe.rs`
to `probe/src/lib.rs`, and copy `Cargo.toml.example` and `Cargo.lock` into
`probe` as `Cargo.toml` and `Cargo.lock`. For each variant use an independent
target directory, applying at most one patch to the same `codec` source path.
Run `cargo +stable rustc --offline --locked --release --manifest-path
probe/Cargo.toml --target <target> --lib -- --emit=asm`; repeat with
`--no-default-features` before `--lib`. The probe deliberately leaves the codec's
SIMD features disabled. Results depend on the installed toolchain and target.

### Native x86 candidate dispatch

On the reserved x86 Linux VM, the instrumented exact-reservation candidate
passed the 216-case dispatch test with `std simd-avx2` and `std simd-avx512`;
the test asserted the active backend names `rust-avx2` and `rust-avx512`. It
also passed separately with `--no-default-features --features simd-avx2` and
`--no-default-features --features simd-avx512`, compiled respectively with
`-C target-feature=+avx2` and `-C target-feature=+avx512f,+avx512bw`. In both
no-std cases the ordinary GF8 backend is `scalar-rust`, as the current
`auto_select_backend` specifies. These cases do not establish that a generated
SIMD encode kernel executed. The initial no-std AVX2 attempt incorrectly
expected `rust-avx2` and failed that test assertion before running the cases;
the rerun corrected the expectation to match the existing dispatch policy.

The native test used Rust 1.99.1, one pinned vCPU and the same source base; the
resource check stopped the run if the effective CPU set changed or a named
competing benchmark appeared. No timing was collected. The concise test
output is in `stack-screen/native-dispatch-results.txt`. To reproduce, apply
`candidate-single-exact.patch` and then `candidate-exact-counter.patch` to a
disposable source copy, install `candidate-dispatch-test.rs` as
`tests/candidate_dispatch.rs`, and run the feature configurations above with
`-p rustfs-erasure-codec --test candidate_dispatch`. Set
`RSE_TEST_EXPECT_BACKEND` to the stated backend to assert selection. The
counter patch is test-only.
