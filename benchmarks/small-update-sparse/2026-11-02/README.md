# Sparse small-update investigation

This is an isolated prototype against commit `3a415cde881d13d1afe8c04a4fd096574902b8ad`.
It is not integrated in the codec. With `experimental-small-update` enabled,
the existing helper first compares a chunk and then builds a delta and updates
parity over its entire length. The prototype narrows arithmetic to the span
between the first and last different element. It still checks each mutable
output against the original complete range before applying the narrowed delta.

`work-probe.rs` uses an instrumented GF8-compatible custom field and verifies
all parity shards against a full encode. With 4+2, one changed shard and 256
elements, both first/middle/last single-element changes required 256 field
adds and 512 multiplied elements in the baseline, versus 1 and 2 in the
prototype. Changes at both ends and a dense change required 256 and 512 in
both; identical data required zero and zero. Counts are operations on this
fixture, not timings, allocation counts, or a general performance result.

The prototype changes how many times custom `Field::add`, `PartialEq`, and
`Field::mul_slice_add` are called and what slice lengths the latter receives.
These methods are user-implemented and can have observable side effects. The
current generic API does not permit assuming that this change is compatible.
The `shrinking-view-probe.rs` demonstrates another boundary: `AsRef` can return
the validated full output while the subsequent `AsMut` returns a shorter slice.
An earlier prototype incorrectly succeeded and partially wrote output; the
revised prototype and baseline both panic before the first parity write for
that fixture. This is evidence for one case, not proof of general equivalence.

Local AArch64 release checks used stable Rust with `experimental-small-update`,
both with and without default features. Existing `incremental_update_regressions`
(10 cases) and `small_update_inline` passed for the revised prototype. The work
probe passed on both versions and checked full-encode parity. Existing
`view-probe.rs` gave identical observations for 18 fixtures against the revised
prototype; those observations do not cover every view. A paired
baseline/candidate check-only run on the revised prototype passed the existing
GF8 catalog; no timing comparison was performed. There is no native acceptance
evidence for this proposal.

The expanded `paired-probe.rs` and `runner-sparse.patch` add dense and
explicitly identical updates at 64, 256 and 257 elements for both 4+2 and 10+4. Their
24-case check-only run passed locally. The runner retains the independent
same-source preflight, resource monitoring and drift gates; a check-only pass
does not indicate that any timing gate has passed.

Reconstruct the measured runner from commit `70898c1` as described in the
[small-update investigation](../../small-update/2026-11-02/README.md), applying
`cross-arm-gate.patch`, `runner-small-cases.patch` and `runner-update.patch` in
that order. Then apply `runner-sparse.patch` with `git apply --unidiff-zero`.
The reconstructed file was verified byte-for-byte against the runner used for
the native attempt; it remains outside the repository's executable sources.

## Native x86 check and incomplete performance preflight

Both paired binaries passed all 24 check-only cases on x86 Linux, reporting the
`rust-gfni-avx512` backend. One attempted preflight stopped before any samples
because the CPU reservation changed when another task started. After a fresh
idle check, a second preflight completed six cases; the seventh, 10+4 with a
single change at 256 elements, failed because its baseline A2/A1 drift was
-10.54%, exceeding the preset 5% limit. The resource monitor reported no error
for that second attempt. The cause of the drift was not established.

The [partial samples](native-baseline-incomplete.jsonl) and
[failure summary](native-preflight-summary.json) are retained for review.
No second baseline round or candidate timing occurred. These data cannot
establish either a speedup or regression, including for cases preceding the
failed gate. Do not use them to select a threshold or enable the prototype.

To reproduce, start with two clean copies of the named commit. Apply
`candidate.patch` only in the candidate copy. Copy either diagnostic source to
a `tests/` file in each copy and run:

```sh
RUSTUP_TOOLCHAIN=stable cargo test --offline --locked --release -p rustfs-erasure-codec --no-default-features --features experimental-small-update --test TEST_NAME -- --nocapture --test-threads=1
```

Do not enable this prototype for general custom fields without a compatible
opt-in contract and tests. Evaluate complete timing, including dense and
identical updates and the 256/257-element boundary, with stable independent
baselines before drawing a performance conclusion.
