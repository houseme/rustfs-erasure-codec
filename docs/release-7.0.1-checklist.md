# reed-solomon-erasure 7.0.1 发布清单

本清单用于 7.0.1 patch release 的发布收口。7.0.1 聚焦 stream 路径性能优化、benchmark artifact 归档、回归准入和文档同步。

## 1. 版本冻结

- `Cargo.toml` package version: `7.0.1`
- workspace dependency `rustfs-erasure-codec`: `7.0.1`
- `Cargo.lock` root package version: `7.0.1`
- README / README_CN 安装示例版本: `7.0.1`
- `CHANGELOG.md` 已新增 `7.0.1 (2026-06-28)` 条目。
- release 之前确认工作树无未提交变更（`git status --short` 为空）。

## 2. 7.0.1 发布重点

- 新增 stream path 专项 benchmark：
  - `tests/benchmark_stream_paths.rs`
  - `target/benchmark-smoke/stream-path-results.json`
  - `target/benchmark-smoke/stream-path-results.csv`
- 新增 `StreamIoMode::{Auto, Serial, Parallel}` 与 `StreamOptions::with_io_mode(...)`。
- 优化 stream block 读取和 padding 固定成本。
- 优化 `reconstruct_stream` metadata / container / buffer 复用。
- 收敛 `reconstruct_stream` 热路径，避免每 block 构造 `indexed` 临时向量。
- 更新 release gate：
  - `RUN_STREAM_PATH_GATE`
  - `RSE_STREAM_PATH_BASELINE`
  - `ns_per_block`
  - `encode_stream` / `verify_stream` / `reconstruct_stream` 阈值。
- 归档 stream benchmark artifact：
  - `benchmarks/stream-path/2026-06-28-cooldown/`

## 3. 推荐发布前验证

```bash
cargo test --lib --features std -- stream
cargo clippy --all-targets --all-features -- -D warnings
```

stream fast profile 建议保留冷却时间后执行：

```bash
sleep 20
RSE_STREAM_PROFILE=fast \
RSE_STREAM_ITERATIONS=10 \
RSE_STREAM_IO_MODE=auto \
cargo test --release --features std --test benchmark_stream_paths -- --ignored --nocapture
```

stream gate 示例：

```bash
cargo xtask check-benchmark \
  --baseline benchmarks/stream-path/2026-06-28-cooldown/phase4-baseline-f1ad373-fast-auto-selected-iter10.csv \
  --current target/benchmark-smoke/stream-path-results.csv \
  --metric ns_per_block \
  --threshold encode_stream=0.15 \
  --threshold verify_stream=0.15 \
  --threshold reconstruct_stream=0.18
```

预期结果：

- `cargo test --lib --features std -- stream`: pass
- `cargo clippy --all-targets --all-features -- -D warnings`: pass
- stream benchmark: pass
- stream gate: `failures: []`

## 4. Release-preflight 环境变量

如果以 `VALIDATION_PROFILE=release` 执行 `scripts/release-check.sh`，请准备以下 baseline：

- `RSE_SMOKE_BASELINE`
- `RSE_SMALL_FILE_BASELINE`
- `RSE_RECONSTRUCTION_HOTSPOT_BASELINE`
- `RSE_STREAM_PATH_BASELINE`

7.0.1 推荐版本化路径示例：

- `RSE_SMOKE_BASELINE=artifacts/benchmarks/7.0.1/smoke-results.json`
- `RSE_SMALL_FILE_BASELINE=artifacts/benchmarks/7.0.1/small-file-results.json`
- `RSE_RECONSTRUCTION_HOTSPOT_BASELINE=artifacts/benchmarks/7.0.1/reconstruction-hotspot-results.json`
- `RSE_STREAM_PATH_BASELINE=artifacts/benchmarks/7.0.1/stream-path-results.json`

## 5. Tag 到发布

```bash
git tag -a v7.0.1 -m "release: v7.0.1"
git push origin v7.0.1
```

Release note 模板：

```text
## reed-solomon-erasure 7.0.1

Patch release focused on stream path performance, benchmark governance, and release gate coverage.

### Highlights
- Added stream path benchmark artifacts and cooldown validation workflow.
- Added `StreamIoMode::{Auto, Serial, Parallel}` for stream I/O scheduling.
- Reduced stream block read padding and allocation overhead.
- Reused reconstruct stream metadata/container/buffers across blocks.
- Added stream path release gate support with `ns_per_block` thresholds.
- Updated README, benchmark methodology, release checklist, and changelog.

### Validation
- `cargo test --lib --features std -- stream`: PASS
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS
- stream fast profile benchmark: PASS
- stream regression gate: PASS (`failures: []`)
```
