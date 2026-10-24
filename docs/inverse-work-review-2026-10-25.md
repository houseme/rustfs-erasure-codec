# Direct augmented inverse work

Runtime `4aa4ee0` is based on main `0814bbd`, including merged selected-survivor
caching, reduced missing-data equations and GFNI scalar-tail isolation. The
[complete result](../benchmarks/release-9.1.0/2026-10-25/matrix-inverse-work-review.json)
separates local, native-completed and resource-pending evidence.

Classic inversion previously allocated a square identity matrix and copied it
into a second augmented matrix. The new implementation creates the same zeroed
`[matrix | identity]` storage directly, copies input rows and writes unit diagonal
entries. Gaussian operations, pivot selection/order, full SIMD row widths and
inverse extraction remain unchanged. The shared path serves Vandermonde setup
and dense cold recovery; partial-data recovery retains its reduced solver. Public
APIs, format identity, coefficients, cache keys and destination publication are
unchanged. There are no new unsafe operations, thresholds or allocator settings.

## Resource regression and compatibility

The correct six-case regression fails on unchanged main and passes after the fix.
GF8 and GF16 33+33, 40+40 and 64+64 recovery use reversed-identity custom parity,
preallocated 65-element buffers and disabled inverse caching. No systematic data
survives, forcing dense inversion with pivot swaps. Every case records ten to
nine total allocation requests, and three to two requests at the square-matrix
size. The two retained squares are the selected-input matrix and returned inverse.
The removed identity contains k squared elements. These are caller-thread request
counts with backend setup excluded; they are not throughput or RSS measurements.
All recovered bytes and original parity remain correct.

A separate GF8/GF16 oracle compares directly against the original augmented
construction over empty, small, inline/spilled and vector-boundary sizes, with
normal/reversed rows and general tower-field coefficients. It preserves the input,
exact inverse, singular errors and nonsquare errors. Existing selective recovery,
late destination failure and warm cache regressions pass.

Local stable validation passes 395 library, 164 integration and ten doc parent
tests (569), six filtered child executions separately, 43 existing ignored and
266 codec-only no_std tests. Published 9.0.0 comparisons pass 200 non-fast normal
configurations, 27 custom configurations / 162 cross recoveries, 12 wide GF16
configurations and 270 selective configurations / 2,640 cross recoveries on the
local default backend. Current Rust tooling passes 278 unit and 39 CLI tests
(317), strict codec/tool Clippy, fmt, typos and diff checks.

The observer accepts exactly one recognized legacy or direct scratch initializer.
Unknown or ambiguous initializers still refuse. Both pre/post inverse observations
are removed in a complete-method comparison to require unchanged production code.
All nine generated files are independently checked: legacy output is byte-exact;
only the matrix input/output changes, and reversing only the initializer plus its
new test restores the complete old output. Formal Rust templates and benchmark
loops remain unchanged. The original resource-budget mistake, legacy-only observer
refusal and obsolete negative-test anchor failure are retained; the latter now
requires that its mutation actually changes the input.

## Native execution and remaining work

Eight commands on the exact committed source pass, including 412 library,
163 integration and ten doc parent tests (585), six filtered child executions,
the new allocation/matrix oracles, both GFNI row widths, the GFNI scalar-entry
contract, 978 cache-layout cases and 18 warmed zero-allocation cases. A zero-codec
independent replay verifies all eight raw summaries and 52 unchanged inputs.

The pipeline then stops at the no_std command when another task starts a foreign
heavy cargo process. This is a real resource-ownership refusal, not a codec failure
or a statistical rejection. Its original `failed-preserved` terminal, partial
records and refusal identity remain retained. The canonical guard stops only this
owner's commands, verifies source/CPU-set/original observer identity, archives all
records, independently verifies the downloaded archive, publishes a complete
atomic acknowledgement and closes the restored lease. Owner exit is independently
verified. The guard is not relaxed.

A separate immutable continuation contains exactly the interrupted no_std command
and remaining original suffix, 26 commands total. It has not launched because
shared resources remain busy. The completed eight are not selected for rerunning.
Native six-backend published-version compatibility is therefore pending; local
results and old native sources are not relabeled to fill that gap.

This change removes one redundant matrix allocation and copy. It provides zero
performance observations and does not explain prior same-pattern drift or page
faults. Final source/tool/lock freezing, accepted complete published-9.0.0 ABBA and
paired-layout data, and actual strict positive release preflight remain required.
All previous timing refusals remain intact. No release approval, tag or registry
upload is produced.
