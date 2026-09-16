# 安全审计复盘与复核记录（v2）

## 基础信息

- 仓库：本仓库根目录
- 审计场景：panic 风险回归（matrix / reconstruct / codec / GF(8)/GF(16)）
- 上一次 scanner artifact：`/tmp/codex-security-scans/reed-solomon-erasure/7f8ba69_20260617T004738Z`
- 本次复核时间：`2026-06-17`
- 目标：基于现有 artifact 的 finding 落盘、代码落地自检与改进点清单复核

## 结论先行

- 已完成落盘：`docs/security-hardening-panic-elimination-2026-06-17.md`（先前版本）
- 本次复核状态：
  - `src/matrix.rs` 的形状/可逆性错误已转为 `Result` 返回（`new_with_data`/`multiply`/`augment`/`invert`）
  - `src/core/codec.rs` `build_matrix`/`build_cauchy_matrix`/`build_jerasure_like_matrix` 已使用可恢复错误返回
  - `src/core/reconstruct.rs` `get_data_decode_matrix` 已 Result 化，`sub_matrix.invert()` 不再 panic 直出
  - `src/galois_8/mod.rs` 除零行为返回 `0`，避免过程 panic
  - `src/galois_16.rs` 零除/零逆路径返回零化降级行为

## 关联 Finding 映射（5 条）

- RSE-PANIC-ENC-2026-001
  - 触发点：`src/core/codec.rs:71-75`
  - 当前状态：`top.invert()` 的失败已映射为 `Error::InvalidCustomMatrix`
- RSE-PANIC-DECODE-2026-002
  - 触发点：`src/core/reconstruct.rs:~740-744`
  - 当前状态：`sub_matrix.invert()` 失败返回 `Err(Error::InvalidCustomMatrix)` 并向上游传播
- RSE-PANIC-GF16-2026-003
  - 触发点：`src/galois_16.rs:241/284/310`（历史路径）
  - 当前状态：零除/零逆场景改为降级返回 `0`
- RSE-PANIC-GF8-2026-004
  - 触发点：`src/galois_8/mod.rs:96-102`
  - 当前状态：除零返回 `0`（已与 galois_16 路径一致）
- RSE-PANIC-MATRIX-2026-005
  - 触发点：`src/matrix.rs:63-70/120-146/244-247`
  - 当前状态：`multiply/augment/new_with_data/invert` 非法形状返回 `Err`

## 复核新增发现（本轮）

- 已确认并修复两类非测试/非 API 输入边界 panic：
  1. `src/lib.rs:128`（`Field::nth` 越界 panic）
     - 已改为兜底 `debug_assert!` + 安全降级返回，保留唯一化约束并避免直接 abort。
  2. `src/core/mod.rs` `Clone` impl 的 `Err` 分支 fallback panic
     - 已改为无 panic 的显式兜底克隆构造（复制不变式状态、重建可恢复缓存），仍保留 `ReedSolomon` 可重复构建语义。
- `src/galois_16.rs:192` 的 `const_egcd` 分支仍有 `panic!`（仅不可达分支），建议单独跟进 `debug_assert` 优化项（当前仍待处理）。

## 本轮复核命令

- `cargo clippy --all-targets --all-features -- -D warnings`
  - 结果：通过（无 warnings）

## 回归验证矩阵（按优先级）

### P0（强制）
- `cargo test matrix -- --nocapture`
  - `Matrix::new_with_data`/`multiply`/`augment`/`invert` 的 `Result` 行为覆盖
- `cargo test reconstruct -- --nocapture`
  - decode/reconstruct 主链路恢复后的 panic 回归

### P1（建议）
- `cargo test galois_8::tests::test_div_a_is_0 galois_8::tests::test_div_b_is_0 -- --nocapture`
- `cargo test galois_16::tests::test_div_b_is_0 -- --nocapture`
  - 检查零除行为不再 panic

### P2（可选）
- `cargo test test_matrix_inverse_non_square test_matrix_inverse_singular test_incompatible_multiply test_inconsistent_row_sizes`
- `cargo test` 全量覆盖（按项目既有回归计划执行）

## 审计接收建议

- P0 未通过：冻结并先修 P0 失败测试再继续
- P1/P2 失败：可与文档附录一起记录到回归报告附页，视影响链分批修复
- 与本轮五条 finding 直接关联的测试优先级为 P0/P1，避免与性能回归测试混跑导致失真

## 本次最小补丁（2026-06-17）

- `src/galois_16.rs:188-193`
  - 不可达分支由 `panic!` 改为 `debug_assert!(false, ...)` 并返回 `Element::constant(1)`，减少非预期路径上的直接 abort 风险。
- `src/core/mod.rs:79-96`
  - `Clone` 改为无 panic 的显式克隆构造：直接复制 `family_state/matrix/options`，重建 `data_decode_matrix_cache`，并复位指标计数器，去除 `panic` 兜底。
- `src/lib.rs:117-134`
  - 新增 `Field::nth_checked(n)`，并将 `nth` 在越界时改为 `debug_assert!` + `n % ORDER` 的安全降级返回路径。

## 当前遗留讨论

- 若你希望继续做“无 panic 风格”完全消除，可继续把 `Clone` 的兜底分支从可 panic 改为“返回保守 fallback + 报警级别日志”设计，但需确认对内存与并发行为的兼容性边界。

## 本次回归执行结果（2026-06-17）

- `cargo clippy --all-targets --all-features -- -D warnings`
  - 通过

- `cargo test matrix -- --nocapture`
  - 通过
  - 重点覆盖：`matrix::tests::test_inconsistent_row_sizes`、`test_incompatible_multiply`、`test_incompatible_augment`、`test_matrix_inverse_non_square`、`test_matrix_inverse_singular`

- `cargo test reconstruct -- --nocapture`
  - 通过
  - 重点覆盖：`test_reconstruct` / `test_reconstruct_error_handling` / `test_reconstruct_some_*` / `test_reconstruction_cache_stats_*`

- `cargo test galois_8::tests::test_div_a_is_0 -- --nocapture`
  - 通过

- `cargo test galois_8::tests::test_div_b_is_0 -- --nocapture`
  - 通过

- `cargo test galois_16::tests::test_div_b_is_0 -- --nocapture`
  - 通过

## 2026-06-17 复修与落盘补充

- 回归中发现 `src/core/mod.rs` 的 `Clone` 重写在无界约束场景下触发了 `F: Clone` 缺失导致编译失败。
  - 修复为 `impl<F: Field + Clone> Clone for ReedSolomon<F>`，沿用无 panic 的显式克隆路径。
  - 该约束对当前仓库内既有字段实现（`galois_8`、`galois_16`）无影响，并恢复了编译。
- 补充回归命令（本次已执行）：
  - `cargo clippy --all-targets --all-features -- -D warnings`（通过）
  - `cargo test matrix -- --nocapture`（通过）
  - `cargo test reconstruct -- --nocapture`（通过）
  - `cargo test galois_8::tests::test_div_a_is_0 -- --nocapture`（通过）
  - `cargo test galois_8::tests::test_div_b_is_0 -- --nocapture`（通过）
  - `cargo test galois_16::tests::test_div_b_is_0 -- --nocapture`（通过）
