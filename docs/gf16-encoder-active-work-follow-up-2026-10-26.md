# Leopard GF16 encoder：移除闲置 scratch

接续[GF16 decode 共享 scratch](gf16-shared-scratch-follow-up-2026-10-26.md)，
本轮继续执行全局复审第 2、5 项的资源生命周期收敛。

实现提交：`dba382ddc1dbd0b28b34ffe4ba9fb7f640828fce`。
基线为上一轮交付 `5ab7cdf`；已包含远程 main
`07f02df5f78ada84588a6f4fc737b7fb484d33d1`，前后锁文件相同。

## 源码收敛

GF16 encoder 仍分配、初始化并通过 TLS 缓存一个 chunk scratch。
它沿 FFT/IFFT 帮助函数传入 dit4_at_16，但最终仅执行 `let _ = scratch`。
当前 u16 蝶形直接操作 work lanes；输入字节直接转换到这些 lanes，
输出也直接从 lanes 转为用户布局。完整调用链的所有分支均未使用 scratch。

现在移除这份 Vec、TLS 槽、初始化和三个帮助函数的无效透传参数。
实际工作区仍由 FlatWork16 持有，缓存准入及返回检查按其真实分配字节计算。
没有新增公共 API、unsafe、算术路径、调度开关或阈值。
GF8 encoder 的蝶形确实使用 scratch，GF16 decoder 的转换/缩放也仍需要 scratch，
两者保留各自必要的资源。

encoder 的 work 堆缓冲预算仍为 4 MiB，immutable FFT plan 独立受 1 MiB 预算约束。
去掉闲置 scratch 后，恰好 4 MiB 的 work 可缓存；超过预算的 work 使用本地资源，
保持原有小缓存及 plan。预算不包括调用者分片、其他家族缓存或进程整体驻留内存。

## 冷/热资源对照

[完整资源记录与源码绑定](../benchmarks/global-review/2026-10-26/gf16-encoder-active-work-resources.json)
包含每版本 15 条配对记录、30 个计数窗口。每个场景重复三次；使用完全相同的最终
外部测试覆盖旧源码，收集全部结果并核对输出后，旧实现的预算断言实际失败，候选通过。

每组在新 OS 线程上运行两次 encode_sep。全局 field tables 已在父线程初始化，
线程私有 plan/work 从冷状态开始。输入、codec、调用者 parity 容器和参考输出在窗口外
准备；第一次计入 FFT 计划、work、scratch 及 API 元数据，第二次允许沿用合法缓存。
每次计数结束后将所有 parity 字节与同版窗口外生成的参考输出比较。

计数覆盖调用线程的 alloc/alloc_zeroed/realloc 请求和累计申请字节。
现有 GF16 encode_sep 使用串行计算；没有把其他工作线程的申请遗漏为零。
这些数据不是存活字节、RSS、延迟或吞吐。

| 布局 / 分片长度 | 冷申请次数，前 → 后 | 冷申请字节，前 → 后 | 热申请字节，前 → 后 |
| --- | ---: | ---: | ---: |
| 10+4 / 32,768 B | 7 → 6 | 295,712 → 262,944 | 0 → 0 |
| 10+4 / 65,536 B | 7 → 6 | 590,624 → 525,088 | 0 → 0 |
| 10+4 / 131,136 B，64 B 尾部 | 7 → 6 | 590,624 → 525,088 | 0 → 0 |
| 4+32 / 65,536 B | 9 → 8 | 4,260,992 → 4,195,456 | 4,260,992 → 0 |
| 4+64 / 65,536 B | 10 → 9 | 8,458,848 → 8,393,312 | 8,458,848 → 8,393,312 |

10+4 的冷调用少申请一个 32～64 KiB chunk；其正常热调用原本已复用缓存，保持零申请。
4+32 的 work 为 `64 * 65536 = 4,194,304 B`，旧版加上闲置 scratch 后超限，
每次重新准备资源。候选可缓存这份恰好 4 MiB 的 work 和独立 plan，热调用由 9 次请求
降为零。它现在会保留这份 work；不能由申请减少推导所有负载的常驻内存都下降。

4+64 的 work 为 8 MiB，仍超过预算；冷/热调用均减少一份 64 KiB scratch，
热请求数由 10 降至 9，保留计划及宽视图元数据等申请，不能概括为整个 API 零分配。
原小缓存跨 oversized 调用的保留，由实际缓存指针和输出回归覆盖。

stdout 与 JSON 的 work_only_budget_eligible 是两版采用的同一个候选预算判据，
不是旧运行时缓存状态的采样。

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test leopard_encode_reuse_regressions gf16_encode_uses_only_active -- --nocapture
```

## 验证

stable Rust 1.99.1 / macOS ARM64：

- 全工作区全特性 928 项通过、43 项原有忽略；排除自启动子测试的重复计数。
- 独立 no_std+SIMD 库 273 项通过；严格全目标全特性 Clippy、格式及 diff 检查通过。
- 发布版 9.0.0 与路径 9.1.0 的 200 种非 fast 配置精确 parity、双向恢复及历史 tower 向量通过。
  其中包含 Leopard GF16；大宽度资源场景由本轮独立功能回归补充。
- 扩展多块对照：4+32、4+64 的整片 parity 与逐个独立 64 B wire block 比较，含短尾。
  现有布局交替、全零输入、非零脏输出、奇数起始地址和恢复回归继续通过。
- 缓存单测覆盖 work 预算等值、实际超限分配及 oversized 调用前后的小缓存/plan 身份。
- x86_64 Linux 全特性库和 WASM no_std+SIMD 库交叉编译检查通过，未执行 x86 实机基准。
- JSON 与分发包 allowlist 审计通过：核心包 71 个成员、WASM 包 6 个成员，工具与本轮证据不进入分发包。

实际 dynamic source-plan 在新旧冻结副本各生成 9 个成员、编译并通过 14 项功能回归。
本轮未改动这些 observer 成员，其严格摘要保持原值；encoder 文件随完整运行时副本一起
构建，但该 observer 没有 encoder 内部所有者探针。资源证据来自未注入版本的外部分配
计数，不把这些编译检查称为 encoder 生命周期采集或发布验收。

## 后续顺序

本轮完成 GF16 encoder 的闲置工作区及参数链清理。全局复审第 3 项普通大分片 verify
的分块 scratch，仍等待真实负载与稳定速度证据；第 5 项 GF16 decode 在 oversized
调用期间保留旧小缓存的替代策略，仍需线程数、大小交替频率及进程峰值预算。

历史 ABBA 漂移失败记录保持，本轮未重新计时，没有吞吐提升或全平台最优结论。
本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。

## 后续进展

Leopard GF8/GF16 encoder 的跨块 lane 引用表复用与短尾验证见
[lane 视图跟进报告](leopard-lane-view-follow-up-2026-10-26.md)。
以上保留本轮原始实现身份及资源结果。
