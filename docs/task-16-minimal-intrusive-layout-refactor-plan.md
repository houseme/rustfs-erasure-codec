# Task 16: 最小侵入目录重排方案（仅结构与导出路径）

## 1. 目标

在**不改变行为**、不改变公开 API 语义的前提下，对当前代码目录做一轮最小侵入重排，降低后续维护成本，重点解决以下两个“巨石点”：

1. `src/galois_8/mod.rs` 体量过大，且混合了主逻辑与大量测试实现。
2. 原单文件 `src/core/leopard_gf8.rs` 职责过多；当前实现已拆分到 `src/core/leopard_gf8/`。

## 2. 约束与非目标

### 约束

1. 不修改算法行为、不修改性能策略阈值、不引入新 feature。
2. 对外可见导出路径保持兼容（含 `rustfs_erasure_codec::galois_8::*` 与 `core` 公开导出）。
3. 每个子步骤都可单独回滚，且能独立通过验证。

### 非目标

1. 不做算法优化（例如新的 SIMD 路径、新的并行策略）。
2. 不做 API 设计变更（新增/删除 public 方法）。
3. 不在本轮引入大规模命名重写。

## 3. 目标目录形态（重排后）

```text
src/
  core/
    leopard_gf8/
      mod.rs              # 原 leopard_gf8.rs 入口与 re-export
      tables.rs           # LUT / skew / 初始化
      driver.rs           # encode driver 与参数推导
      work.rs             # FlatWork 及 lane 视图管理
      encode.rs           # encode_skeleton / encode_with_tables 及流程函数
  galois_8/
    mod.rs                # 保留字段类型、导出、薄入口
    tests.rs              # 从 mod.rs 抽离出的测试模块
```

说明：该形态是“最小侵入”版本，不触及 `backend.rs / policy.rs / profile.rs` 的逻辑边界。

## 4. 分阶段执行计划（PR 级）

## 阶段 A：`galois_8` 测试代码外置（低风险，先做）

### 变更

1. 新增 `src/galois_8/tests.rs`，承接当前 `mod.rs` 内的 `#[cfg(test)] mod tests { ... }` 内容。
2. `src/galois_8/mod.rs` 改为 `#[cfg(test)] mod tests;` 薄声明。

### 兼容性

1. 仅测试编译单元重排，不影响库导出与运行时路径。

### 风险

1. 测试内对私有函数/常量可见性需要保持原作用域（通过同模块层级保障）。

## 阶段 B：`leopard_gf8` 按职责分文件（中风险，仍属结构改动）

### 变更

1. 新建目录 `src/core/leopard_gf8/`。
2. 将原单文件内容按职责拆入：
   - `tables.rs`：`LeopardGf8Tables`、LUT 初始化相关函数。
   - `driver.rs`：`LeopardGf8EncodeDriver`、`build_*driver`。
   - `work.rs`：`FlatWork` 与 lane 视图辅助。
   - `encode.rs`：`encode_skeleton`、`encode_with_tables`、编码流程辅助函数。
3. 新建 `src/core/leopard_gf8/mod.rs` 作为聚合入口，对 `super` 保持当前可见性约定（`pub(crate)`）。
4. `src/core/mod.rs` 仍保持 `pub(crate) mod leopard_gf8;`，外部调用路径不变。

### 兼容性

1. 所有已有调用仍走 `super::leopard_gf8::*` 路径，不改调用方签名。
2. 若存在测试/基准直接引用内部符号，保留原 `pub(crate)` 暴露级别。

### 风险

1. 内部 helper 拆分后的循环依赖（通过 `mod.rs` 聚合与 `use super::*` 约束规避）。
2. 文件移动导致未使用导入告警（按 clippy/fmt 一次性清理）。

## 阶段 C：导出路径和文档一致性清扫（低风险）

### 变更

1. 确认 `src/lib.rs`、`src/core/mod.rs`、`src/galois_8/mod.rs` 导出路径不变。
2. 若注释中引用旧文件路径，仅更新路径注释，不改语义。

## 5. 验证清单

每阶段完成后都执行：

1. `cargo fmt --all`
2. `cargo check --workspace`
3. `cargo test --workspace --all-targets`

建议最终再执行：

1. `cargo clippy --workspace --all-targets --all-features -- -D warnings`

说明：如果时间成本考虑，可先做 `check + targeted tests`，最后一次跑全量 gate。

## 6. 提交策略（建议）

建议拆为 2~3 个提交，便于 review 与回滚：

1. `refactor(galois_8): move inline tests into dedicated tests module`
2. `refactor(core): split leopard_gf8 into focused internal modules`
3. `chore: keep export paths stable after layout refactor`（如需要）

## 7. 回滚策略

1. 阶段 A/B/C 任一阶段可独立回滚，不影响其它阶段。
2. 若阶段 B 出现风险，可保留阶段 A 合并，阶段 B 单独撤回。

## 8. 预期收益

1. 降低单文件心智负担，提高定位速度。
2. 为后续性能优化与后端扩展提供更清晰边界。
3. 在不触碰行为的前提下提升代码评审可读性。
