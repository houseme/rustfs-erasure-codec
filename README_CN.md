# rustfs-erasure-codec

[![CI](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml/badge.svg)](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml)
[![Crates](https://img.shields.io/crates/v/rustfs-erasure-codec.svg)](https://crates.io/crates/rustfs-erasure-codec)
[![Documentation](https://docs.rs/rustfs-erasure-codec/badge.svg)](https://docs.rs/rustfs-erasure-codec)
[![dependency status](https://deps.rs/repo/github/houseme/rustfs-erasure-codec/status.svg)](https://deps.rs/repo/github/houseme/rustfs-erasure-codec)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)
[![Crates.io License](https://img.shields.io/crates/l/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)

[English](README.md) | 中文

`rustfs-erasure-codec` 是一个 Rust 2024 Reed-Solomon 纠删码库，覆盖内存分片、定向恢复、渐进式恢复以及按块流式处理场景。

当前 `9.1.0` 主线提供：

- Classic `GF(2^8)` 与 `GF(2^16)` Reed-Solomon
- Leopard GF8 与 Leopard GF16 编解码器族
- 面向 `galois_8` 的运行时 SIMD 后端分发
- 可复用的验证与恢复 workspace
- 定向恢复与渐进式恢复 API
- 按块流式 encode、verify、reconstruct API
- `no_std` 支持与 WASM 子 crate

WASM 绑定见 [wasm/README.md](wasm/README.md)。

## 安装

默认 `std` 构建：

```toml
[dependencies]
rustfs-erasure-codec = "9.1.0"
```

启用全部支持的 SIMD 后端：

```toml
[dependencies]
rustfs-erasure-codec = { version = "9.1.0", features = ["simd-accel"] }
```

也可以只启用部署平台需要的后端：

```toml
[dependencies]
rustfs-erasure-codec = { version = "9.1.0", features = ["simd-neon"] }   # aarch64
# rustfs-erasure-codec = { version = "9.1.0", features = ["simd-ssse3"] } # x86_64
# rustfs-erasure-codec = { version = "9.1.0", features = ["simd-avx2"] }  # x86_64
# rustfs-erasure-codec = { version = "9.1.0", features = ["simd-avx512"] }# x86_64
# rustfs-erasure-codec = { version = "9.1.0", features = ["simd-gfni"] }  # x86_64
# rustfs-erasure-codec = { version = "9.1.0", features = ["simd-vsx"] }   # powerpc64
```

运行时后端分发带有保护；目标 CPU 不支持的 ISA 会安全回退到标量路径。

## 快速开始

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::VerifyWorkspace;

fn main() {
    let rs = ReedSolomon::new(3, 2).unwrap();

    let mut shards = vec![
        vec![0, 1, 2, 3],
        vec![4, 5, 6, 7],
        vec![8, 9, 10, 11],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 0],
    ];

    rs.encode(&mut shards).unwrap();
    let original = shards.clone();

    let mut missing: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
    missing[0] = None;
    missing[4] = None;

    rs.reconstruct(&mut missing).unwrap();

    let rebuilt: Vec<Vec<u8>> = missing.into_iter().map(|shard| shard.unwrap()).collect();
    let mut workspace = VerifyWorkspace::new(&rs, rebuilt[0].len());

    assert!(rs.verify_with_workspace(&rebuilt, &mut workspace).unwrap());
    assert_eq!(rebuilt, original);
}
```

如果需要高频校验，优先使用 `verify_with_workspace(...)` 或 `verify_with_buffer(...)` 复用临时缓冲区。
启用 `std` 后，可用 `verify_with_workspace_par(...)` 在所有支持的有限域（包括 tower GF16）
上复用 scratch 并沿用现有并行策略。每个并发操作使用自己的 workspace；分片布局变化时会自动调整。

对于缺失模式稳定的重复 `Option<Vec<u8>>` 恢复，可以先准备 workspace：

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;

let rs = ReedSolomon::new(10, 4).unwrap();
let mut shards = vec![vec![0u8; 1024]; 14];
rs.encode(&mut shards).unwrap();

let mut missing: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
missing[0] = None;
missing[10] = None;

let workspace = rs.prepare_reconstruct_opt_workspace(&missing).unwrap();
rs.reconstruct_opt_with_workspace(&mut missing, &workspace).unwrap();
```

泛型 Classic 恢复可通过 `prepare_reconstruct_workspace(...)` 准备完整计划，
通过 `prepare_reconstruct_data_workspace(...)` 准备数据恢复计划，或通过
`prepare_reconstruct_some_workspace(...)` 固定所需输出。统一使用
`reconstruct_with_workspace(...)` 复用格式、缺失布局、逆矩阵及组合 parity 行，
支持 tower GF16、`ShardSlot` 和 no_std。计划拥有所需目标，后续修改 required mask
不会改变计划；可通过 `data_targets()` / `parity_targets()` 查询排序后的输出。每次执行前仍检查
分片数量、有效矩阵、缺失位置和长度；布局变化后需重新准备。计划不持有调用者
payload，目标存储可由 `ShardSlot` 单独复用。GF8 原有
`OptionVecReconstructWorkspace` 名称作为别名保留，旧入口继续使用批量执行器。
详见 [英文使用示例](README.md)。

## 主要 API

| 领域 | API |
|---|---|
| Classic 编码 | `galois_8::ReedSolomon`, `galois_16::ReedSolomon` |
| 编解码器选择 | `CodecOptions`, `CodecFamily`, `MatrixMode`, `LeopardMode` |
| 校验复用 | `VerifyWorkspace`, `verify_with_workspace`, `verify_with_workspace_par`, `verify_with_buffer` |
| 恢复复用 | `ReconstructWorkspace<F>`, `prepare_reconstruct_workspace`, `reconstruct_with_workspace`, `OptionVecReconstructWorkspace`, `ShardSlot<T>` |
| 定向恢复 | `reconstruct_some`, `reconstruct_some_opt` |
| 渐进式恢复 | `decode_idx` |
| 增量编码 | `ShardByShard` |
| 流式处理 | `stream::encode_stream`, `stream::verify_stream`, `stream::reconstruct_stream`, `reconstruct_stream_from` |

## 编解码器族

`CodecOptions::codec_family` 用于选择算法族。

| Family | 状态 | 说明 |
|---|---|---|
| `Classic` | 完整支持 | 默认族。支持矩阵模式、`update`、`encode_single*`、`decode_idx` 与 `reconstruct_some`。 |
| `LeopardGF8` | 支持 `galois_8` | 基于 FFT 的 GF(2^8) 路径。要求分片长度是 64 字节整数倍，最多 256 个总分片。Classic-only API 会被拒绝。 |
| `LeopardGF16` | 支持高分片数 | 基于 FFT 的 GF(2^16) 路径，面向更高总分片数。Classic-only API 会被拒绝。 |

示例：

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions};

let rs = ReedSolomon::with_options(
    32,
    16,
    CodecOptions::builder()
        .codec_family(CodecFamily::LeopardGF8)
        .build(),
)
.unwrap();
```

Leopard family 限制：

- 分片长度必须是 64 字节整数倍
- 所有分片缓冲区长度必须一致
- `decode_idx(...)`、`update(...)`、`encode_single*` 仍然只适用于 Classic

## 矩阵模式

`CodecOptions::matrix_mode` 只对 `CodecFamily::Classic` 生效：

- `Vandermonde`
- `Cauchy`
- `JerasureLike`
- `Custom`

如需兼容既有 classic 载荷布局，保持 `MatrixMode::Vandermonde`。

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::CodecOptions;

let custom_rows = vec![vec![1u8, 1, 1], vec![1u8, 2, 4]];
let rs = ReedSolomon::with_custom_matrix(3, 2, &custom_rows, CodecOptions::default()).unwrap();
```

## 内存复用

`ShardSlot<T>` 允许重复恢复流程保留缺失分片的底层缓冲区：

```rust
use rustfs_erasure_codec::galois_8::{mark_missing_slots, shards_to_slots, ReedSolomon};

let rs = ReedSolomon::new(4, 2).unwrap();
let mut shards = vec![
    vec![0, 1, 2, 3],
    vec![4, 5, 6, 7],
    vec![8, 9, 10, 11],
    vec![12, 13, 14, 15],
    vec![0, 0, 0, 0],
    vec![0, 0, 0, 0],
];
rs.encode(&mut shards).unwrap();

let mut slots = shards_to_slots(&shards);
mark_missing_slots(&mut slots, &[1, 5]);
rs.reconstruct(&mut slots).unwrap();

assert!(slots[1].is_present());
assert!(slots[5].is_present());
```

对于 `galois_8` 的 SIMD 敏感负载，可以使用对齐分片辅助接口：

- `rustfs_erasure_codec::galois_8::alloc_aligned_shards(...)`
- `galois_8::ReedSolomon::alloc_aligned(...)`

## 编码格式身份

`rs.encoding_format()` 统一描述已解析的算法族、分片数和有效矩阵，供编码器相等性及恢复 workspace 兼容判断使用。
调用 `rs.encoding_format().stable_id_bytes()` 可按需生成完整、版本化的标识字节，作为独立元数据保存并在使用持久化分片前比较；它不插入分片内容，也不用于自动创建 codec。
内置 GF8 和 tower GF16 提供可移植身份；自定义 Field 未声明稳定字段合同时返回 `None`。缓存、后端和线程配置不改变编码身份，序列化不进入编码及恢复热路径。

## 流式处理

流式接口位于 `rustfs_erasure_codec::stream`，默认 `std` 特性下可用。

主要入口：

- `encode_stream(...)`
- `verify_stream(...)`
- `reconstruct_stream_from(...)`：独立 `Option<Read>` / `Option<Write>` 输入输出
- `reconstruct_stream(...)`：兼容原有 `Cursor<Vec<u8>>` 调用

当前范围：

- 实现在 Classic `galois_8` 路径上
- 通过 `StreamOptions` 做按块处理
- I/O 前校验端点布局和可恢复性，每块写入前校验输入长度一致性
- Leopard-family 编解码器会返回 `UnsupportedCodecFamily`

对文件及其他非 Seek 外部流，使用 `reconstruct_stream_from` 做有界内存恢复。
两个数组各有 total-shards 个槽：`Some(reader)` 明确表示存在且对应 writer 必须为 `None`；缺失位置用 `None` reader 和 `Some(writer)`。
即使为空数据集，也必须有至少 data-shards 个存在的 reader。接口从当前读写位置推进，处理短读写及中断；失败后可能保留已写前缀，不自动 flush。
旧 Cursor 接口仍以空 Vec 推断缺失，输出仍驻留内存。详见[本轮核查及示例](docs/erasure-codec-completion-audit-2026-10-17.md)。

## 运行时后端控制

环境变量：

- `RSE_BACKEND_OVERRIDE`
- `RSE_STRICT_BACKEND_OVERRIDE=1`

未设置或设置为 `auto` 的 `RSE_BACKEND_OVERRIDE` 会在平台支持时允许 generated SIMD encode code。任何已识别的显式 override（包括 `scalar`）都会使用所选 generic backend，并绕过 generated SIMD codegen。
已识别的显式后端名包括 `scalar`、`rust-neon`、`rust-ssse3`、`rust-avx2`、`rust-avx512`、`rust-gfni-avx2`、`rust-gfni-avx512` 和 `rust-vsx`。

检查当前后端：

- `galois_8::active_backend_name()`
- `galois_8::active_backend_kind()`
- `galois_8::active_backend_id()`

## 调优与剖析

常用 `CodecOptions` 参数：

- `fast_one_parity`
- `inversion_cache`
- `inversion_cache_capacity`
- `max_parallel_jobs`

并行策略环境变量：

- `RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES`
- `RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB`
- `RS_PARALLEL_POLICY_MAX_JOBS`
- `RS_PARALLEL_POLICY_L2_CACHE_BYTES`
- `RS_PARALLEL_POLICY_DEBUG`

可选指标：

- `benchmark-metrics` feature
- `leopard_gf8_profile_stats()`
- `reset_leopard_gf8_profile_stats()`

## 校验与基准测试

常见工作流：

```bash
cargo test --workspace
cargo test --workspace --features "simd-accel benchmark-metrics"
cargo clippy --workspace --all-targets --features "simd-accel benchmark-metrics" -- -D warnings
cargo bench --bench galois_backend --features "std simd-accel"
```

后端敏感的性能工作应使用 ABBA drift gate：

```bash
RSE_ABBA_BACKEND=auto \
RSE_ABBA_SAMPLE_SIZE=20 \
RSE_ABBA_WARMUP_TIME=2 \
RSE_ABBA_MEASUREMENT_TIME=2 \
bash scripts/run_galois_backend_abba.sh <baseline-ref> <candidate-ref>
```

如果 A1/A2 baseline drift gate 失败，不应发布性能提升或退化结论。

推荐参考：

- [docs/benchmark-methodology.md](docs/benchmark-methodology.md)
- [docs/README-performance-index.md](docs/README-performance-index.md)
- [docs/ec-klauspost-feature-comparison.md](docs/ec-klauspost-feature-comparison.md)
- [scripts/README.md](scripts/README.md)

## 项目来源

版本 `0.9.0` 到 `6.0.0` 最初由
[Darren Ldl](https://github.com/darrenldl) 创建，并由
[rust-rse](https://github.com/rust-rse) 社区继续维护。

当前 `7.0.0` 主线由
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
维护，包含 Rust 2024 重构、运行时 SIMD 架构、Leopard 编解码器族和 RustFS 兼容性加固。

## 贡献

欢迎贡献。对于后端敏感、基准敏感或编解码器族相关改动，建议附带聚焦验证结果。

## 许可证

本项目采用 MIT License，详见 [LICENSE](LICENSE)。
