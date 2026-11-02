# Reusable verify workspace length-check investigation

At commit `6cf17e0`, `VerifyWorkspace::prepare` first checks the number of
parity buffers, then checks the first buffer length, then scans all buffer
lengths. The first length is therefore observed twice for an already-sized,
nonempty workspace. Public codec constructors reject a zero parity count. The
[candidate](candidate.patch) removes the separate first-length test while
retaining the count check and complete scan. It is an isolated prototype, not
production code or evidence of a performance gain.

The complete scan matters even when the first buffer has the right length.
`VerifyWorkspace::resize` can unwind after changing an earlier buffer, for
example if a user-defined `Field::zero` panics. The regression in
`tests/verify_partial_resize.rs` injects this interruption and checks that a
later verification repairs the remaining buffers, succeeds for valid parity,
and rejects corrupted parity. It passes on the original implementation and
on the prototype with and without the codec's `std` feature. The prototype
also passed the existing standard-library workspace, parallel verify and
buffer-reuse tests; the corresponding no-default-feature test targets contain
zero tests, so only the dedicated no-std regression provides execution evidence
for that configuration.

A non-isolated AArch64 diagnostic used the retained `paired-probe.rs`, locked
dependencies, GF8, 4+2 and 10+4 at 64/256 elements, 2+33 at 64 elements, and
a 4+2/4096 control. Both codecs re-encode and verify the same inputs before
sampling, with workspace setup outside the timed loop. Each phase used a short
warmup and a single aggregate 300 ms sample. This is an exploratory screen,
not the repository's native acceptance harness.

The same-source [baseline](local-screen.txt) exceeded the 5% phase-drift limit
on several cases (including +7.54%, +17.29% and -20.78% A drift). The candidate
was nevertheless measured afterward; those candidate numbers are invalid for
attribution because the baseline gate had already failed. Neither the partial
screen nor the reduced source-level operation count establishes a speedup or
regression. The candidate stays out of production until an isolated paired
preflight, complete A/B/B/A, compatibility tests and wider-field controls pass.

To reproduce the diagnostic, archive `6cf17e0` into sibling `base` and `same`
directories. Change only `same`'s package version to `9.1.0+diagnostic` so
Cargo can link both copies. Copy `paired-probe.rs`, `Cargo.toml.example` and
`Cargo.lock` into a sibling `probe` directory as `src/main.rs`, `Cargo.toml`
and `Cargo.lock`. Build the same-source baseline with locked dependencies;
apply `candidate.patch` only to `same` for the candidate. Use this source only
for exploratory diagnosis, not performance acceptance.
