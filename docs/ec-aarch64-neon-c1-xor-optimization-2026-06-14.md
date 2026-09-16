# EC AArch64 NEON `c == 1` 优化复测结论（2026-06-14）

## 1. 变更落点

- 文件：`src/galois_8/aarch64/neon.rs`
  - `rust_neon_mul_slice_xor` 的 `c == 1` 分支改为 NEON 向量化路径：
    - `64B` 每次处理（4x unroll）
    - `16B` 尾部处理
    - 处理尾长度（<16）仍走纯标量 fallback
- 文件：`src/galois_8/tests.rs`
  - 新增 `test_rust_neon_mul_slice_xor_c1_vectorized_fastpath`
  - 覆盖随机长度 + 与 `mul_slice_xor_scalar_for_test(1, ..)` 对齐校验

## 2. 复测命令与基线

- 小文件基线：
  - `benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv`
- 执行命令：
  - `cargo test --features "std simd-accel" test_rust_neon_`
  - `RSE_SMALL_FILE_PROFILE=extended bash scripts/run_small_file_benchmark_matrix.sh`
  - `python3 scripts/check_benchmark_regression.py --baseline benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv --current target/benchmark-smoke/small-file-results.json --metric ns_per_iter --threshold encode=0.12 --threshold verify=0.12 --threshold verify_with_buffer=0.12 --threshold reconstruct=0.18 --threshold reconstruct_data=0.18 --require-case encode:4:2:1024 --require-case verify_with_buffer:4:2:4096 --require-case reconstruct:4:2:16384 --require-case reconstruct_data:10:4:65536`
  - `VALIDATION_PROFILE=extended RUN_SIMD_ACCEL_TESTS=1 RUN_SMALL_FILE_GATE=1 RSE_SMALL_FILE_PROFILE=extended RSE_SMALL_FILE_BASELINE=benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv ./scripts/release-check.sh`

## 3. 复测结果（核心）

- 测试与主链结论：
  - `test_rust_neon_mul_slice_xor_c1_vectorized_fastpath` 通过
  - 小文件 `ns_per_iter` 比对通过：`failures: []`
  - 本轮比较条目数：`64`
  - 最大回归率：`0.0703`（`reconstruct:10:4:1024`），全部低于阈值
  - `release-check` 在当前快照下完成并未报回归门控异常
- 性能特征：
  - 多数小/中小文件点出现明显提速（如 `reconstruct:4:2:16384`, `encode:10:4:1048576` 等）
  - 无证据显示 `c == 1` 路径引入功能回退

## 4. 结论

1. `aarch64` 小文件路径（含 `1KiB/4KiB/16KiB/64KiB/128KiB/256KiB/512KiB`）仍已覆盖，并建议保持该覆盖作为默认复测口径；
2. 当前优化属于行为一致、低风险增益：修复了 `c == 1` 的纯标量 XOR 迭代热点；
3. 该项可视为一轮可落地优化收口，后续可继续跟踪在更高负载和更长迭代下的稳定性，但不要求继续扩展大范围结构改造。
