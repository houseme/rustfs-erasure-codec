# EC AArch64 Release Recheck 结论（2026-06-14，Round 2）

## 1. 本轮执行范围

- `./scripts/release-check.sh`
- `RSE_SMALL_FILE_PROFILE=fast ./scripts/run_small_file_benchmark_matrix.sh`
- `RSE_SMALL_FILE_PROFILE=extended ./scripts/run_small_file_benchmark_matrix.sh`
- `cargo xtask check-benchmark --baseline benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv --current target/benchmark-smoke/small-file-results.json --metric ns_per_iter ...`

## 2. 主链复核结果（release-check）

- `cargo check --tests`：通过
- `cargo test --test selftest`：2/2 通过
- `env RSE_SMOKE_PROFILE=quick cargo test --test benchmark_smoke ... -- --ignored --nocapture`：27/27 通过
- `cargo test --no-default-features`：194/194 通过（`target/debug/deps` 下全部通过，无失败）
- `cargo test --features std`：278/278 通过（含完整单测、ignore 基准测试、doctest）
- 结论：`./scripts/release-check.sh` 在本次快照（`VALIDATION_PROFILE=fast`）**全部通过**，并正确提示跳过 extended 门控。

## 3. 小文件性能复核

- 本次对比使用 `RSE_SMALL_FILE_PROFILE=extended`，与历史基线文件口径一致：
  - 历史基线：`benchmarks/small-file/2026-05-27-aarch64-apple-silicon-extended.csv`
  - 当前工件：`target/benchmark-smoke/small-file-results.json`
  - 判定规则：`ns_per_iter`，阈值 `encode/verify/verify_with_buffer=0.12`, `reconstruct/reconstruct_data=0.18`
- 回归检查结果：
  - 对比条目：64
  - 回归失败：0
  - 最大回归率（最坏点）：`5.073%`（`reconstruct:10:4:1048576`），未超 18% 门限
- 关键样本（当前 vs 基线）：
  - `4+2, 1KiB, encode`: `1283.4 ns` vs `3166.8 ns`（显著更快）
  - `4+2, 16KiB, reconstruct`: `16041.6 ns` vs `33516.6 ns`（显著更快）
  - `10+4, 64KiB, reconstruct_data`: `136908.4 ns` vs `180233.4 ns`（显著更快）
- `benchmarks/small-file/...` 基线不包含 `verify_with_buffer` 项，当前对比也未出现该项，说明该维度仍需在下一次基线刷新时补齐。

## 4. 对“是否需要小文件优化”的结论

1. 小文件路径（`1KiB / 4KiB / 16KiB / 64KiB / 128KiB / 256KiB / 512KiB`）在当前验证链路中是**明确覆盖**的，结论不变：需要持续监控。
2. 本轮可复现结论：未见稳定、持续性回退，更多是 baseline 与运行行为差异导致噪声，当前不建议新增 EC 内核级优化。
3. 优先动作是继续保持 release-check 与文档的口径一致和历史比对治理。

## 5. 文档-主链一致性修正

- 已将 `docs/benchmark-methodology.md` 的 `reconstruction` 热点验证示例补齐为：
  - `cargo test --release --features "std simd-accel" benchmark_reconstruction_hotspots -- --ignored --nocapture`
- 该项与 `scripts/release-check.sh` 的真实执行链保持一致，避免未来再次出现“脚本能跑，文档示例无法复现文档口径”的偏差。
