# EC x86_64 Validation Review (2026-06-16)

## 1. Scope

This pass re-validated the current `main` branch on the live `x86_64` host and
reviewed whether the current branch had drifted into patch-on-patch EC changes
that should be refactored again before further optimization work.

Referenced materials:

- `docs/ec-small-file-benchmark-playbook.md`
- `docs/benchmark-methodology.md`
- `benchmarks/small-file/2026-05-27-x86_64-linux-extended.csv`
- `benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv`

## 2. Environment

- host arch: `x86_64`
- CPU: `AMD EPYC 9V45 96-Core Processor`
- ISA confirmed by `lscpu`: `ssse3 / avx2 / avx512f / avx512bw / gfni`
- branch: `main`
- git pull status: already up to date on `2026-06-16`

## 3. Commands Run

```bash
git pull --rebase

lscpu

cargo test --features "std simd-accel" test_select_x86_backend_priority -- --nocapture
cargo test --features "std simd-accel" test_active_backend_metadata -- --nocapture
cargo test --features "std simd-accel" test_x86_cross_backend_conformance_matrix -- --nocapture

bash scripts/run_x86_backend_smoke_matrix.sh

RSE_SMALL_FILE_PROFILE=extended \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture

cargo xtask check-benchmark \
  --baseline benchmarks/small-file/2026-05-27-x86_64-linux-extended.csv \
  --current target/benchmark-smoke/small-file-results.json \
  --metric ns_per_iter \
  --threshold encode=0.12 \
  --threshold verify=0.12 \
  --threshold verify_with_buffer=0.12 \
  --threshold reconstruct=0.18 \
  --threshold reconstruct_data=0.18 \
  --require-case encode:4:2:1024 \
  --require-case verify_with_buffer:4:2:4096 \
  --require-case reconstruct:4:2:16384 \
  --require-case reconstruct_data:10:4:65536

RSE_SMALL_FILE_PROFILE=extended \
RSE_SMALL_FILE_CASE_FILTER=4x2_1k,4x2_4k,4x2_16k,4x2_64k,10x4_64k,10x4_256k \
RSE_SMALL_FILE_ITERATIONS=40 \
cargo test --release --features "std simd-accel" \
  --test benchmark_small_files \
  benchmark_small_file_matrix_runs_and_exports_results -- --ignored --nocapture

cargo test --release --features "std simd-accel" \
  benchmark_reconstruction_hotspots -- --ignored --nocapture
```

## 4. Validation Findings

### 4.1 Small-file coverage is required

The answer remains yes: EC validation on this branch must explicitly consider
small-file sizes such as `1 KiB / 4 KiB / 16 KiB / 64 KiB / 128 KiB / 256 KiB / 512 KiB`.

Those sizes are already part of the supported benchmark matrix. The real risk is
not missing coverage, but misreading a noisy run and optimizing the wrong path.

### 4.2 x86 backend selection is healthy

The following checks passed:

- `test_select_x86_backend_priority`
- `test_active_backend_metadata`
- `test_x86_cross_backend_conformance_matrix`

On this host, default `auto` selection still resolves to `rust-avx2`.

### 4.3 x86 smoke collection still works end-to-end

`bash scripts/run_x86_backend_smoke_matrix.sh` completed and wrote:

- `benchmarks/x86_64-simd/2026-06-16-amd-epyc-9v45-96-core-processor.json`

The generated machine summary still shows the same policy split as earlier x86
review work:

- benchmark-driven recommendation ranks `rust-gfni-avx512` first
- policy-eligible default ranking still keeps `rust-avx512` ahead of `rust-avx2`
- current runtime priority remains conservative and still starts with `rust-avx2`

That means there is still policy drift worth documenting, but not enough new
evidence here to justify changing runtime dispatch in this pass.

## 5. Small-file Interpretation

### 5.1 One-shot `extended` comparison is noisy on this host

A full one-shot `extended` run compared against
`benchmarks/small-file/2026-05-27-x86_64-linux-extended.csv`
flagged many apparent regressions.

That result is not safe to use directly as a code-change trigger because the
same session became materially different once the suspicious cases were rerun in
isolation with higher iteration counts.

### 5.2 Targeted reruns changed the story

Focused reruns with `RSE_SMALL_FILE_CASE_FILTER` plus `RSE_SMALL_FILE_ITERATIONS=40`
showed the previously suspicious cases were actually strong on this host:

- `4x2_1k verify_with_buffer`: `288.93 ns`
- `4x2_4k verify_with_buffer`: `838.00 ns`
- `4x2_16k reconstruct_data`: `4582.10 ns`
- `4x2_64k encode`: `15152.62 ns`
- `10x4_64k reconstruct_data`: `56643.12 ns`
- `10x4_256k reconstruct_data`: `214755.60 ns`

Compared with the archived `2026-05-27` x86 baseline, those filtered reruns are
not regression evidence. They are generally much faster than the archived
baseline and much faster than the misleading one-shot `extended` pass.

### 5.3 What is still suspicious

The only notable remaining weak point in the filtered rerun set was:

- `10x4_64k reconstruct`: `337348.28 ns`

That case exceeded the configured latency threshold versus the archived x86
baseline, while `10x4_64k reconstruct_data` stayed strong at `56643.12 ns`.

This means:

1. the dedicated data-only recovery path is still valuable
2. the evidence does not support deleting the current `reconstruct_data`
   specialization as “patch-on-patch waste”
3. if another optimization pass is needed later, the better target is full
   `reconstruct` mixed recovery behavior around `10x4 / 64 KiB`, not the current
   `reconstruct_data` data-only specialization

## 6. Patch-on-Patch Review Judgment

### 6.1 What still looks layered

The branch still has layered execution structure in `reconstruct_data`:

- shared chunked encode path
- one/two-output reconstruction-specialized parallel path
- `data_only` stage policy split

That structure is not aesthetically minimal.

### 6.2 Why it should stay for now

This pass did **not** find evidence that the current specialized `reconstruct_data`
paths should be flattened back into the generic path.

`benchmark_reconstruction_hotspots` still shows:

- `reconstruct_data_missing_1_data`: `1.3258x` vs baseline `reconstruct`
- `reconstruct_data_missing_data_plus_parity`: `1.7260x`
- `reconstruct_data_32x16_missing_2_data`: `1.1317x`

`reconstruct_data_missing_2_data` was only `0.9600x`, which is near parity and
worth tracking, but it is not enough on its own to justify deleting the current
specialized branch when the other hotspot scenarios remain clearly positive.

### 6.3 Final code decision

For this pass, the best executable high-performance choice is:

1. keep the current runtime structure unchanged
2. keep the validated `reconstruct_data` specialization
3. avoid a speculative refactor that would trade measured hotspot wins for a
   cleaner but less-proven implementation

## 7. Final Conclusion

1. Small-file speed absolutely needs to stay in the EC validation surface.
2. On this `x86_64` EPYC host, a one-shot full `extended` matrix is too noisy to
   be treated as direct regression proof.
3. Suspicious small-file points must be rerun with `RSE_SMALL_FILE_CASE_FILTER`
   and higher iterations before changing code.
4. Current branch review does **not** justify another broad runtime refactor.
5. The only optimization-worthy follow-up signal from this pass is to study
   full `reconstruct` around `10x4 / 64 KiB`, not to undo the current
   `reconstruct_data` specialization.
