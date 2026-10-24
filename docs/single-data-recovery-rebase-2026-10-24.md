# Single-data recovery rebase and observer compatibility

PR #86 is rebased from `ae32d2688bbde11b077375fee20031095275f3c6` onto
main `801f4996da8585d17c7068b6ede9ef60892bec51`, including the GF16 tower/layout
integration and native tooling changes from #84/#85. The only textual conflict
was in CHANGELOG; both entries are retained. Rebased codec source `4044e4d`
keeps `src/core/reconstruct.rs` and its oracle tests byte-identical to the original
PR. The observer fix below changes repository tooling only.

## Diagnostic integration fix

The prior [tooling CI job](https://github.com/houseme/rustfs-erasure-codec/actions/runs/40061899336/job/120777022096)
failed two source-snapshot assertions after the recovery function changed.
The old observer also inserted a `sub_matrix` reference into the new function,
which no longer owns that variable. Merely updating the snapshot would leave
the generated source invalid.

The observer now recognizes the separate decode-matrix builder. It records the
selected submatrix immediately before full inversion in the fallback branch;
the single-data branch has no selected submatrix/inversion scratch to record.
Actual matrix construction, final decode and cache-hit observations remain.
The older recovery layout keeps its existing observer bytes. Generated code
retains every original reconstruction statement, and changed fallback anchors
are rejected. Current-production source snapshots are updated explicitly;
all frozen sampler-output snapshots and archived benchmark files are unchanged.

## Fresh validation

Stable 1.99.1 on ARM64 passes:

- 384 all-feature library, 162 integration and 10 documentation tests; 43
  existing tests remain ignored. Codec-only no_std passes 257 library tests.
- The five full-inverse oracle tests cover GF8/GF16 survivor combinations,
  larger erasure blocks, singular custom matrices and general tower coefficients.
- Actual published 9.0.0: 200 normal-format parity/bidirectional recovery cases,
  27 custom tower configurations / 162 cross recoveries and 12 wide GF16 cases.
- All 255 native-tool tests (226 unit / 29 CLI), including both former failures;
  five Python dynamic-controller tests also pass.
- Generated observers compile against both actual source copies (current codec
  and published baseline). On each copy the active-observer test preserves normal,
  selective, invalid-input and late-failure API results/publication/bytes.
- Strict all-target/all-feature codec, compatibility and native-tool Clippy;
  x86_64 Linux library Clippy; formatting, typos and diff checks.
- Release/workspace/package audits pass: core/WASM lists are 68/6 files, and
  the JSON audit checks 614 files with zero errors.

The x86 check is compilation only; native Linux execution on the new head is
left to CI. No timing campaign, performance acceptance or release approval is
inferred from these functional checks. Historical validation remains bound to
its original source.
