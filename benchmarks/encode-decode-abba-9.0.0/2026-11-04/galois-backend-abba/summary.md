# Galois Backend ABBA Summary

- Drift threshold: 5.00%
- Input gate: PASS
- Drift gate: PASS
- Candidate attribution: PERMITTED
- Commits: A=9891dbe / B=8252010

| Criterion group | Operation | Length | A1 mean ns | A2 mean ns | A drift | B1 mean ns | B2 mean ns | B drift | B vs A mean | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| galois_mul_slice_rust-neon_RustNeon_RustSimd_override_auto | mul_slice | 65536 | 1208.30 | 1206.15 | -0.18% | 1205.47 | 1212.01 | 0.54% | 0.13% | PASS |
| galois_mul_slice_rust-neon_RustNeon_RustSimd_override_auto | mul_slice | 1048576 | 19236.94 | 19350.47 | 0.59% | 19287.19 | 19882.81 | 3.09% | 1.51% | PASS |
| galois_mul_slice_rust-neon_RustNeon_RustSimd_override_auto | mul_slice | 4194304 | 76719.78 | 77117.37 | 0.52% | 77500.78 | 78919.57 | 1.83% | 1.68% | PASS |
| galois_mul_slice_xor_rust-neon_RustNeon_RustSimd_override_auto | mul_slice_xor | 65536 | 1212.00 | 1217.05 | 0.42% | 1218.34 | 1224.05 | 0.47% | 0.55% | PASS |
| galois_mul_slice_xor_rust-neon_RustNeon_RustSimd_override_auto | mul_slice_xor | 1048576 | 20201.21 | 20256.81 | 0.28% | 20290.95 | 20400.72 | 0.54% | 0.58% | PASS |
| galois_mul_slice_xor_rust-neon_RustNeon_RustSimd_override_auto | mul_slice_xor | 4194304 | 81849.15 | 82281.41 | 0.53% | 82378.06 | 82087.32 | -0.35% | 0.20% | PASS |
