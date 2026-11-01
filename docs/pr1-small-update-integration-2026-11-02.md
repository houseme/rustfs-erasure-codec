# Opt-in PR 1 kernels and small incremental update

The requested functionality is integrated behind two **default-off** Cargo
features. This is functional integration, not performance acceptance or a
change to default feature selection.

- `experimental-small-update` uses an isolated 256-element inline delta buffer
  for Classic incremental updates when the complete shard has at most 256
  elements and each element is at most three bytes. Other sizes, no-op updates,
  single-parity fast mode and other codec families retain their existing paths.
- `experimental-pr1-kernels` adds 6+2 generated encoders for AVX2, AVX-512 and
  NEON, plus the 6+3, 8+3 and 8+4 AVX-512 generated shapes. The experimental
  6+2 selector retains a 4,096-byte minimum, and AVX-512-only builds retain the
  existing 32 KiB generated-encoding threshold.
- The PR's AVX2/AVX-512 split-table row kernels are used only for **serial
  Classic reconstruction after stable input and output slices are obtained**.
  Ordinary encoding and the existing parallel path do not enter them. Backend
  and CPU feature checks remain mandatory; GFNI and scalar selections keep
  their existing fallbacks. This deliberately narrows the original PR's broader
  generic coding dispatch to avoid altering repeated `AsRef`/`AsMut` behavior.
  Custom fields must opt into GF8 byte views to use the table kernels.
- Successful table-row recovery records the existing serial work metrics once.
  The test-only counter and backend child tests verify actual table selection,
  scalar fallback, custom-field opt-out and metrics on native x86.

Source attribution: [boylee1111/rustfs-erasure-codec PR #1](https://github.com/boylee1111/rustfs-erasure-codec/pull/1).

## Functional validation

Tests used an isolated source snapshot based on committed `5aad891` plus these
integration changes. Default-feature AArch64 and x86 AVX2/AVX-512 generated
encoding source matched the committed baseline byte-for-byte. On local Apple
Silicon, release tests passed:

- all-feature library: 424 passed and five ignored;
- no-default-features library with both opt-ins and NEON: 290 passed;
- focused 6+2, small-update, incremental-update, parallel-codegen and encoding
  dispatch integration tests, including the measured 4,095/4,096/4,097
  boundary and custom matrices;
- published 9.0.0 compatibility: 200 general cases, 36 large GF8 cases, 12
  6+2 cases and 32 small-verification cases; update allocation reporting was
  also run with the new feature enabled;
- WASM, x86-64 Windows MSVC and i686 Linux `no_std` cross-checks; AVX2-only and
  AVX-512-only x86 test builds and Clippy checks with warnings denied.

On the reserved native x86 Linux VM, cross-compiled release test executables
were run directly on the CPU after an idle check. AVX2 and AVX-512 table-row
scalar comparisons, unaligned tails, rejected malformed views, actual recovery
selection and custom-field fallback passed. Separate generated-kernel tests,
6+2 integration tests and small-update tests passed for AVX2 and AVX-512;
`no_std` 6+2 and small-update tests also passed when built with the respective
compile-time target features. [Native results](../benchmarks/pr1-integration/2026-11-02/native-results.txt)
record the test output.
These are functional checks, not throughput results.

After the main branch advanced to `9f529f5`, an isolated snapshot of that
revision plus the integration changes passed the full release workspace test
suite with all features (excluding the fuzz package), the full workspace
all-target/all-feature Clippy gate with warnings denied, and both diagnostic
source-plan snapshot checks. The latest-head x86 all-feature release test
executables were then run on the VM: table-row selection and guards, generated
AVX2 and AVX-512 kernels, 6+2 parity, and small-update cases all passed. The
additional output is in
[the latest-head native results](../benchmarks/pr1-integration/2026-11-02/native-final-head-results.txt).
Cross-compiled binaries executed on the native CPU; they were not compiled on
the VM. After this run, only the table module's introductory documentation
comment was corrected; runtime code was unchanged. No claim is made for
architectures or hardware not tested.

## Performance and remaining work

The small-update feature's first same-source preflight on native x86 stopped
at 4+2 / 4,096 elements because A/B level bias was +6.13%, outside the 5%
gate. Both phase drift checks passed for that case; the resource monitor saw
no named competing process or sustained off-target load in 149 observations.
No second baseline round or candidate timing ran. Raw case samples, a redacted
per-observation resource record and the failure summary are in
`benchmarks/small-update/2026-11-02/integrated-feature-run/`. A normalized
instruction comparison found equal-sized update functions with no normalized
instruction differences, but different entry alignment. It does not compare
all external callees and does not establish the cause of the bias.

A follow-up used one common update function for both baseline arms, while each
arm retained its own parity buffers. It stopped at 10+4 / 256 elements with
+7.42% same-source bias; 257 resource observations showed no named competitor
and no sustained off-target busy streak. A further same-function, shared-buffer
diagnostic stopped when the resource monitor detected a competing process,
after five 4+2 cases and before the earlier 10+4 failure case. These controls
change call structure and binary layout. They do not establish the cause of
the original bias, validate the candidate or relax the original gate. The
samples and redacted monitors are in `single-function-run/` and
`shared-buffer-run/` under the small-update benchmark directory.

Complete stable baseline preflights, A/B/B/A, threshold and large-shard
controls, and additional CPU coverage are required before enabling either
feature by default or claiming a speedup. The separate wide-field constructor stack investigation
and rejected outlining experiment are documented in the small-update benchmark
README; no matrix-storage change is included here.
