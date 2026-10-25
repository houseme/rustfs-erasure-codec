# 校验 scratch 与稀疏 parity 输出跟进

本轮接续[调用者生命周期优化](caller-workspace-follow-up-2026-10-26.md)，
在原执行顺序的第 3、4 项中分别落实一个资源改进。没有新增公共 API、unsafe、
调参阈值或共享缓存。远程 main 基线仍为 `b8d7a91`。

## 普通 fast-one-parity 校验共享内联 scratch

实现提交：`90d82ae`；对照的运行时代码为 `19c8383`。

原普通 `verify` 在 fast-one-parity 分支始终申请一个整片 Vec，而其他 Classic 校验
以及 `verify_par` 已使用内联容量为 4096 个元素的 SmallVec。
现在普通 fast 校验也使用这份现有 scratch，通过共享比较函数选择原来的 XOR encoder。
`verify_with_buffer` 的 fast 分支也收回同一函数，不重复维护编码与比较逻辑。

没有把 XOR 改成逐元素比较或分块重编码；矩阵身份、验证顺序和损坏检测保持。
新回归覆盖 GF8/tower GF16、首尾损坏，以及 1/65/4096/4097 元素边界。
对照先在原实现上实际失败，再在改动后通过；每个场景保留三条记录。

| 每片元素数 | GF8 每次申请，前 → 后 | tower GF16 每次申请，前 → 后 |
| --- | ---: | ---: |
| 1 | 1 次 / 1 B → 0 | 1 次 / 2 B → 0 |
| 65 | 1 次 / 65 B → 0 | 1 次 / 130 B → 0 |
| 4096 | 1 次 / 4096 B → 0 | 1 次 / 8192 B → 0 |
| 4097 | 1 次 / 4097 B → 相同 | 1 次 / 8194 B → 相同 |

内联预算以元素计数；GF8 为 4 KiB，tower GF16 为 8 KiB。
这些存储仍然存在，不能把零 heap 请求解释为无需 scratch 或峰值 RSS 下降。
普通大分片 verify 的整片 scratch 没有改成有界分块，第 3 项尚未全部完成。

## 流式 parity 输出直接遍历已验证的目标

实现提交：`e3f7ed4`；对照的运行时代码为 `90d82ae`。

流式恢复的 parity 阶段原来每块构造一张 `SmallVec<[&mut [u8]; 32]>`；
目标数越过内联容量后，每块再申请引用表。
现在连续输出与稀疏输出通过私有、静态分发的适配器共用原有分块算术循环。
稀疏适配器使用已验证、递增且不重复的目标索引，通过安全的 `IterMut::nth`
重新借用分片所有者。没有缓存跨块可变引用、重排 payload 所有者或增加第二套算术循环。

连续输出仍直接进入原 GFNI 检查函数。稀疏输出满足原 GFNI 条件时，使用最多四个
内联引用适配原生入口；更宽输出沿用共享循环。因此没有为删除引用表而绕过已有
GFNI 选择，也没有增加经验性的尺寸阈值。系数、输入、分块和输出处理顺序保持。

| 布局，缺失 33 个 parity | 2 块恢复的申请次数，前 → 后 | 6 块恢复的申请次数，前 → 后 |
| --- | ---: | ---: |
| 2+33，全部 parity 缺失 | 40 → 38 | 44 → 38 |
| 4+40，含间隔的 parity 目标 | 49 → 47 | 53 → 47 |

块大小为 1024 B，长度分别为 1061/5157 B，均包含 37 B 短尾。
Reader 数据及 Writer 容量在计数前准备；计数窗口保留整个恢复 API 的实际 scratch 生命周期。
新回归先在原实现上失败，再验证申请次数不随块数增长，且全部恢复输出逐字节正确。

功能回归还覆盖 1/2/3/4/5/32/33/40 个 parity 目标、非连续目标、缺失数据后的 parity
恢复，以及 Auto/Serial/Parallel 三种 I/O 模式。旧的截断、部分输出、非法布局、奇异
矩阵、空流和全部可恢复小布局回归一并通过。

这消除了 parity 阶段的逐块输出表。数据恢复阶段在宽布局下的输入/输出引用表仍保留，
因此第 4 项也不能标记为所有路径均已完成。

## 证据与验证

[全部资源记录及各实现提交的源码绑定](../benchmarks/global-review/2026-10-26/scratch-sparse-resources.json)
保留 fast 校验每版本 24 条记录，以及稀疏恢复每版本两个布局、各两种长度的计数。
基线运行时叠加了对应的新回归；资源断言按预期失败，正确性检查仍执行。
这些是分配请求记录，没有计时或 RSS 测量。先前未通过稳定性检查的速度记录继续保留，
没有重新标记为本轮吞吐结果或发布性能验收。

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test parallel_verify_workspace fast_single_parity -- --nocapture
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test stream_reuse_regressions wide_parity_recovery -- --nocapture
```

本轮 stable Rust 1.99.1/macOS ARM64 的最终验证：

- 工作区全特性：920 项通过、43 项原有忽略，按顶层测试进程汇总。
- 独立 no_std+SIMD 库：273 项通过。
- 发布版 9.0.0 与路径 9.1.0：200 种非 fast 配置的精确 parity、双向恢复及历史向量通过。
- 全工作区、全目标、全特性 Clippy（warnings denied）、Rust 格式及 diff 检查通过。
- JSON 与包 allowlist 审计通过；核心包 71 个成员、WASM 包 6 个成员，工具及资源证据不进入分发包。
- 实际注入四个探针源码成员的临时副本编译成功，40 项相关回归通过；不是正式捕获结果。
- x86_64 Linux 库及 x86_64 Darwin 全目标全特性交叉检查通过，包含 GFNI 分支的类型检查。
  Linux 全目标检查最初因缺少 `x86_64-linux-gnu-gcc` 无法构建开发依赖，未宣称该项通过。
  本轮没有执行 x86 GFNI 实机测试。

Leopard 缓存策略仍等待工作负载与峰值内存数据；本轮没有改变它。
新资源报告和当前入口独立提交，历史报告的源码身份与结果保持原样。
本轮只 commit，未 push、创建 PR、打 tag 或发布 crate。
