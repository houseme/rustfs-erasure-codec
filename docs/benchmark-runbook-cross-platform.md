# Cross-Platform Benchmark Runbook

本手册记录如何在 x86_64 和 aarch64 平台上执行完整的 EC 性能压测流程。

## 1. 前置条件

| 依赖 | 最低版本 | 说明 |
|---|---|---|
| Rust | 1.85+ | 需要 `edition = 2024` 支持 |
| cargo | 匹配 rustc | |
| bc | 任意 | 脚本中比值计算依赖 |
| git | 2.x | 用于 revision 记录 |

aarch64 额外要求：
- NEON 支持（所有 Apple Silicon / ARMv8+ 均满足）
- 如需 SVE 后端测试，需 ARMv9 硬件

## 2. 快速开始

### 2.1 一键完整压测（推荐）

```bash
bash scripts/run-full-benchmark.sh
```

默认行为：
- extended profile（4x2 + 10x4，shard 1K→1M）
- 5 iterations per case
- 15 秒冷却间隔
- 自动检测架构、收集硬件信息
- 输出到 `benchmarks/<arch>/`

### 2.2 仅小文件

```bash
bash scripts/run-full-benchmark.sh --phase small
```

### 2.3 仅大文件（隔离测试）

```bash
bash scripts/run-full-benchmark.sh --phase large
```

### 2.4 自定义参数

```bash
bash scripts/run-full-benchmark.sh \
  --profile fast \
  --iterations 10 \
  --cooldown 30 \
  --features "std simd-accel"
```

## 3. 平台特定说明

### 3.1 x86_64 (Linux)

自动检测的 SIMD 后端：
- `rust-gfni-avx512` — 需要 GFNI + AVX-512（Intel Ice Lake+ / AMD Zen 4+）
- `rust-avx2` — 需要 AVX2（Haswell+）
- `rust-ssse3` — 需要 SSSE3（Core 2+）
- `scalar-rust` — 无 SIMD 时 fallback

强制指定后端：

```bash
RSE_BACKEND_OVERRIDE=rust-avx2 bash scripts/run-full-benchmark.sh
```

### 3.2 aarch64 (Linux / macOS)

自动检测的 SIMD 后端：
- `rust-neon` — 需要 NEON（所有 ARMv8+）
- `scalar-rust` — fallback

Apple Silicon 注意事项：

```bash
# macOS 上使用 gtime 替代 system time
brew install gnu-time

# 确保使用 release build
bash scripts/run-full-benchmark.sh --features "std simd-accel"
```

aarch64 Linux（如 Graviton、鲲鹏）：

```bash
# 确认 NEON 可用
grep -i neon /proc/cpuinfo

# 运行
bash scripts/run-full-benchmark.sh
```

### 3.3 后端一致性验证

运行后端一致性扫描（确认所有可用后端行为一致）：

```bash
# x86_64
bash scripts/run_x86_backend_smoke_matrix.sh

# aarch64
bash scripts/run_aarch64_backend_smoke_matrix.sh
```

## 4. 输出结构

```
benchmarks/
├── x86_64-linux/
│   ├── 2026-06-17-x86_64-linux-extended-hwinfo.txt
│   ├── 2026-06-17-x86_64-linux-extended.csv
│   ├── 2026-06-17-x86_64-linux-extended.json
│   └── 2026-06-17-x86_64-linux-extended-large-isolated.csv
├── aarch64-linux/
│   ├── 2026-06-18-aarch64-linux-extended-hwinfo.txt
│   ├── 2026-06-18-aarch64-linux-extended.csv
│   └── 2026-06-18-aarch64-linux-extended.json
└── aarch64-darwin/
    ├── 2026-06-18-aarch64-darwin-extended-hwinfo.txt
    ├── 2026-06-18-aarch64-darwin-extended.csv
    └── 2026-06-18-aarch64-darwin-extended.json
```

### hwinfo.txt 内容

自动采集以下信息：

- CPU 型号、频率、核心数
- L1/L2/L3 缓存大小
- SIMD 指令集标志
- 内存总量
- 内核版本
- OS 版本
- Rust/cargo 版本
- C 编译器版本
- 系统负载
- 压测配置参数

## 5. 冷却策略

脚本内置冷却机制：

| 阶段 | 默认冷却 | 说明 |
|---|---|---|
| 开始前 | 15s | 确保 CPU 退出 boost 热状态 |
| 小文件→大文件 | 15s | 阶段间冷却 |
| `--cooldown` | 自定义 | 高负载环境建议 30-60s |

判断冷却是否充分：

```bash
# 检查 CPU 频率是否回到基础频率附近
grep 'cpu MHz' /proc/cpuinfo | head -1

# aarch64 Linux
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq

# macOS
sysctl -n hw.cpufrequency
```

## 6. 结果对比

### 6.1 同平台 A/B 对比

```bash
# 旧基线
OLD=benchmarks/x86_64-linux/2026-06-17-x86_64-linux-extended.csv

# 新结果
NEW=benchmarks/x86_64-linux/2026-06-18-x86_64-linux-extended.csv

# 使用 check_benchmark_regression.py
cargo xtask check-benchmark \
  --baseline "$OLD" \
  --current "$NEW" \
  --metric ns_per_iter \
  --threshold reconstruct_opt=0.10
```

### 6.2 跨平台对比

跨平台对比需注意：
- 绝对数值不可直接比较（不同 CPU 性能差异大）
- 重点比较 **比值**（opt/plain ratio）和 **趋势**
- 相同操作在不同平台的性能比例应稳定

### 6.3 关键指标

| 指标 | 用途 | 优先级 |
|---|---|---|
| `ns_per_iter` | 小文件延迟对比 | 高 |
| `throughput_mb_s` | 大文件吞吐对比 | 高 |
| opt/plain ratio | 优化效果验证 | 高 |
| encode/verify ratio | 回归检测基准 | 中 |

## 7. 常见问题

### Q: aarch64 上没有 `simd-accel` feature 怎么办？

aarch64 的 NEON 支持通过 `std` feature 自动启用，不需要额外 feature flag：

```bash
bash scripts/run-full-benchmark.sh --features std
```

### Q: 如何确认实际使用的后端？

查看 CSV 输出中的 `backend` 和 `backend_id` 列，或：

```bash
RSE_BACKEND_OVERRIDE=auto cargo test --release --features "std simd-accel" \
  --test benchmark_smoke -- --ignored --nocapture 2>&1 | grep -i backend
```

### Q: 大文件测试 OOM 怎么办？

10x4_1M case 需要 ~80MB 内存（14 shards × 1MB）。如遇 OOM：

```bash
# 仅测 4x2 大文件
RSE_BENCH_CASE_FILTER='4x2_512k,4x2_1m' bash scripts/run-full-benchmark.sh --phase large
```

### Q: CI 环境中如何使用？

CI 环境建议用 fast profile + 较少 iterations：

```bash
bash scripts/run-full-benchmark.sh --profile fast --iterations 3 --cooldown 5
```

## 8. 文档模板

每次跨平台压测完成后，建议按以下模板记录：

```markdown
## <平台> 压测记录 — <日期>

### 硬件
- CPU:
- 核心:
- 频率:
- 内存:
- SIMD:

### 软件
- OS:
- Kernel:
- Rust:
- Features:

### 结果摘要
| Case | reconstruct | reconstruct_opt | ratio |
|---|---|---|---|
| 4x2_1K | | | |
| 4x2_1M | | | |
| 10x4_1K | | | |
| 10x4_1M | | | |

### 结论
- opt/plain ratio 是否稳定:
- 是否有回退:
- 备注:
```
