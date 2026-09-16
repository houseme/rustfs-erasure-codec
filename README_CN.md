# rustfs-erasure-codec

[![CI](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml/badge.svg)](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml)
[![Crates](https://img.shields.io/crates/v/rustfs-erasure-codec.svg)](https://crates.io/crates/rustfs-erasure-codec)
[![Documentation](https://docs.rs/rustfs-erasure-codec/badge.svg)](https://docs.rs/rustfs-erasure-codec)
[![dependency status](https://deps.rs/repo/github/houseme/rustfs-erasure-codec/status.svg)](https://deps.rs/repo/github/houseme/rustfs-erasure-codec)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)
[![Crates.io License](https://img.shields.io/crates/l/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)

[英文](README.md) | 中文

`rustfs-erasure-codec` 是一个现代 Rust Reed-Solomon 纠删码库，覆盖内存内编码、渐进式恢复、定向恢复以及按块流式处理场景。

当前 `8.0.1` 主线已经提供：

- Classic `GF(2^8)` 与 `GF(2^16)` Reed-Solomon
- 面向 `galois_8` 的运行时 SIMD 后端分发
- Leopard GF8 与 Leopard GF16 编解码器族
- 渐进式恢复与定向恢复 API
- 可复用的验证/恢复缓冲区
- 按块流式 encode / verify / reconstruct API
- `no_std` 支持与 WASM 子 crate

WASM 绑定见 [wasm/README.md](wasm/README.md)。

## 亮点

- `galois_8::ReedSolomon` 是当前最主要、优化最完整的执行路径。
- `galois_16::ReedSolomon` 仍适用于经典 `GF(2^16)` 场景。
- `CodecOptions` 可统一控制编解码器族、矩阵模式、反转矩阵缓存与并行策略。
- `VerifyWorkspace`、`ShardSlot<T>` 和对齐分片辅助接口可降低热点路径分配成本。
- `decode_idx(...)`、`reconstruct_some(...)`、`ShardByShard` 覆盖渐进式与增量型工作流。
- `stream::StreamOptions` 提供按块流式处理入口。

## 安装

添加 crate：

```toml
[dependencies]
rustfs-erasure-codec = "8.0.1"
```

如果关注吞吐，建议开启 SIMD：

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.1", features = ["simd-accel"] }
```

也可以只启用目标平台需要的后端：

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.1", features = ["simd-neon"] }   # aarch64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-ssse3"] } # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-avx2"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-avx512"] }# x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-gfni"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-vsx"] }   # powerpc64
```

说明：

- 默认启用 `std`
- `simd-accel` 是启用全部 SIMD 后端的总开关
- 运行时会自动探测 ISA，不支持时安全回退到标量路径

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

如果 `verify(...)` 需要高频调用，优先使用 `verify_with_workspace(...)`
或 `verify_with_buffer(...)` 来复用校验分片临时缓冲区。

## 内存复用辅助接口

对于重复恢复场景，`ShardSlot<T>` 允许保留缺失分片的底层缓冲区所有权，避免重复分配：

```rust
use rustfs_erasure_codec::galois_8::{ReedSolomon, mark_missing_slots, shards_to_slots};

fn main() {
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
}
```

对于 `galois_8` 的 SIMD 敏感负载，还可以使用对齐分片辅助接口：

- `rustfs_erasure_codec::galois_8::alloc_aligned_shards(...)`
- `galois_8::ReedSolomon::alloc_aligned(...)`

## 编解码器族

`CodecOptions::codec_family` 用于选择算法族：

| 编解码器族         | 状态                | 说明                                                                                                                |
|---------------|-------------------|-------------------------------------------------------------------------------------------------------------------|
| `Classic`     | 完整支持              | 默认族。支持 `update`、`encode_single*`、`decode_idx`、`reconstruct_some` 与矩阵模式切换。                                         |
| `LeopardGF8`  | 适用于 `galois_8` 路径 | 基于 FFT 的 `GF(2^8)` 路径。要求分片长度为 64 字节整数倍，总分片数不超过 256。`update`、`encode_single*`、`decode_idx` 等 Classic-only API 不支持。 |
| `LeopardGF16` | 适用于更高总分片数场景       | 基于 FFT 的 `GF(2^16)` 路径。`update`、`encode_single*`、`decode_idx` 等 Classic-only API 不支持。                             |

示例：

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions};

let rs = ReedSolomon::with_options(
32,
16,
CodecOptions {
codec_family: CodecFamily::LeopardGF8,
..CodecOptions::default ()
},
)
.unwrap();
```

Leopard family 的重要限制：

- 分片长度必须是 64 字节整数倍
- 所有分片缓冲区长度必须一致
- `decode_idx(...)`、`update(...)`、`encode_single*` 仍然只适用于 Classic

## 矩阵模式

`CodecOptions::matrix_mode` 只对 `CodecFamily::Classic` 生效：

- `Vandermonde`
- `Cauchy`
- `JerasureLike`
- `Custom`

如果需要兼容既有经典载荷布局，建议继续使用 `MatrixMode::Vandermonde`。

最小自定义矩阵示例：

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::CodecOptions;

let custom_rows = vec![vec![1u8, 1, 1], vec![1u8, 2, 4]];
let rs = ReedSolomon::with_custom_matrix(3, 2, & custom_rows, CodecOptions::default ()).unwrap();
```

## 渐进式与定向 API

### 渐进式恢复

`decode_idx(...)` 适用于经典 `galois_8::ReedSolomon`，适合输入分片分批到达的恢复场景。

### 定向恢复

`reconstruct_some(...)` 只恢复你标记为必需的分片。

### 逐分片增量编码

`ShardByShard` 提供带状态跟踪的渐进式编码器，适合数据分片逐步到达的场景。

## 流式 API

流式接口位于 `rustfs_erasure_codec::stream`，默认 `std` 特性下可用。

主要入口：

- `encode_stream(...)`
- `verify_stream(...)`
- `reconstruct_stream(...)`

当前适用范围与限制：

- 实现在 classic `galois_8` 路径上
- 通过 `StreamOptions` 做按块处理
- `reconstruct_stream(...)` 当前使用 `Cursor<Vec<u8>>`，present cursor 从位置 `0` 开始读取
- 入口会校验输入（分片数量、present 分片等长、块大小），非法输入返回 `StreamError` 而非产出错误或空结果
- Leopard family 的流式 encode / verify / reconstruct 返回 `UnsupportedCodecFamily`

当数据不适合整组分片常驻内存时，优先考虑这个路径。

## 运行时后端控制

`galois_8` 主路径支持运行时后端查看与强制覆盖。

环境变量：

- `RSE_BACKEND_OVERRIDE`
- `RSE_STRICT_BACKEND_OVERRIDE=1`
- `RUST_REED_SOLOMON_ERASURE_ARCH`

未设置或设置为 `auto` 的 `RSE_BACKEND_OVERRIDE` 会在平台支持时允许 generated SIMD encode code。任何已识别的显式 override（包括 `scalar`）都会让 encode 使用所选的 generic backend，并绕过 generated SIMD codegen。因此 `RSE_BACKEND_OVERRIDE=scalar` 可以可靠地避免执行 generated SIMD。

公开辅助函数：

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

可选剖析/指标接口：

- `benchmark-metrics` feature
- `leopard_gf8_profile_stats()`
- `reset_leopard_gf8_profile_stats()`

## 校验与基准测试

常见工作流：

```bash
# 运行测试
cargo test --workspace

# 运行基准
cargo bench --features simd-accel

# 执行发布校验
bash scripts/release-check.sh

# 执行扩展校验
VALIDATION_PROFILE=extended bash scripts/release-check.sh

# 采集 x86_64 SIMD 基准产物
bash scripts/collect_x86_simd_benchmarks.sh
```

推荐同时参考：

- [docs/benchmark-methodology.md](docs/benchmark-methodology.md)
- [docs/README-performance-index.md](docs/README-performance-index.md)
- [docs/README.md](docs/README.md)
- [scripts/README.md](scripts/README.md)

## 项目来源

版本 `0.9.0` 到 `6.0.0` 最初由
[Darren Ldl](https://github.com/darrenldl) 创建，并由
[rust-rse](https://github.com/rust-rse) 社区继续维护。

当前仓库中的 `8.0.1` 主线由
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
维护，代表了 Rust 2024 重构、运行时 SIMD 架构与 Leopard 相关工作的最新状态。

## 贡献

欢迎贡献。对于后端敏感、基准敏感或编解码器族相关改动，建议附带聚焦的验证结果。

## 许可证

本项目采用 MIT License，详见 [LICENSE](LICENSE)。

仓库内打包的 `simd_c` 源码派生自
[Nicolas Trangez 的 Haskell 实现](https://github.com/NicolasT/reedsolomon)，同样遵循 MIT License。
