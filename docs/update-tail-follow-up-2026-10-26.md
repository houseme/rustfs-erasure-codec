# 增量更新短尾 scratch 与资源测试预热

接续 [GF16 缓存生命周期](gf16-cache-lifecycle-follow-up-2026-10-26.md)，本轮沿用
全局复审第 2 项的资源生命周期原则，收拢已有 update delta buffer 的容量，并修正
完整回归新暴露的并发资源测试预热缺口。

实现提交：`2204b591c7143a6a8272da13ae2c153dc747e69f`。
测量预热修复：`9036a8d3e1d0c564abdaf8f35a5732ce99bf68c6`。
对照基线为 `27cab0b`，远程 main 仍为 `07f02df`。
[完整资源记录和源码绑定](../benchmarks/global-review/2026-10-26/update-tail-resources.json)
保留全部有利与不利对照、首次完整回归失败及修复后的缓存记录。

## delta 容量跟随实际变化块

普通矩阵 update 已按既有串行 chunk 分块，并跳过内容相同的块。
原实现第一次遇到变化时 reserve_exact(chunk_len)，即使变化仅发生在末尾不足一块的
范围，也预留完整块的容量。实际 delta 元素从始至终只计算 old.len() 个；本轮改变
容量预留为 old.len()，算术循环和计算量保持。

同一次调用继续使用原有 Vec，clear 后复用容量，后续需要更大范围时再增长。
所有输入计数/长度校验、Leopard 不支持 update 的错误、fast parity 的无 scratch XOR、
矩阵系数、有限域内核和既有分块策略保持；没有新增缓存、unsafe、API 或调参开关。

4+2 Classic/Vandermonde、仅第 3 个 data shard 的末元素发生变化时，每次申请如下：

| 每片元素数 / 短尾元素数 | GF8：前 → 后 | tower GF16：前 → 后 |
| --- | ---: | ---: |
| 16,385 / 1 | 16,384 → 1 B | 32,768 → 2 B |
| 65,537 / 1 | 65,536 → 1 B | 131,072 → 2 B |
| 65,601 / 65 | 65,536 → 65 B | 131,072 → 130 B |
| 1,052,673 / 4,097 | 65,536 → 4,097 B | 131,072 → 8,194 B |
| 4,194,369 / 65 | 262,144 → 65 B | 524,288 → 130 B |

元素单位保持原策略：GF8 为 1 B，tower GF16 为 2 B；最后一行 GF16 每片实际为
8 MiB + 130 B。每项正向更新及反向恢复各计数三次，申请次数均为 1。
两版本共 120 个短尾窗口，所有 parity 都与窗口外独立 encode_sep 的结果逐字节比较。

另有 24 个先短后长窗口：第 0 个 data shard 先贡献 65 元素的变化尾部，第 3 个 shard
随后贡献完整 65,536 元素块及尾部。旧实现一次申请完整块；新实现先申请短尾，再增长
一次，后续短尾复用已有容量：

| 字段 | 旧申请量 / 次数 | 新申请量 / 次数 |
| --- | ---: | ---: |
| GF8 | 65,536 B / 1 | 65,601 B / 2 |
| tower GF16 | 131,072 B / 1 | 131,202 B / 2 |

这是实际取舍：仅短尾调用显著减少预留，先短后长的输入顺序多一次容量增长。
完整块先出现的原有回归仍要求一次申请；无变化和显式提供相同内容仍要求零申请。
不将本轮改动描述为所有输入顺序的最少分配或吞吐提升。

[原有 45 场景资源报告](../benchmarks/global-review/2026-10-26/update-tail-legacy45.json)
继续按原 schema 执行；本轮 144 个窗口使用最终同一外部探针对照两份库源码。
旧实现收集全部记录、完成 parity 检查后，只因新增短尾申请预算断言失败；新实现通过。
计数仅覆盖调用线程的 alloc/alloc_zeroed/realloc 请求及完整目标字节，update 沿用串行
执行。构造、输入和参考结果均在窗口外，未测量存活峰值、RSS 或稳定耗时。

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test incremental_update_regressions -- --nocapture
```

## 缓存资源实验的预热边界修正

本轮首次完整回归在上一轮新增的 cache lifecycle 测试失败。两个 worker 的某次首次
超限阶段记录了 18 次 / 8,659,504 B，其他重复为 14 次 / 8,657,392 B；新增申请为
2,112 B，结束存活增量另多 1,056 B，使原有申请预算超出 560 B。
失败前的全部 35 行记录保留，未作为正式分支最终通过结果。

锁文件对应的 parking_lot_core 0.10.0 在首次阻塞时初始化 ThreadData，并可能创建或
增长进程 parking hash table。一次成功但未发生锁竞争的 decoder 预热不能保证这个
状态已初始化。其 park 路径先取得线程状态，随后才执行 before_sleep；parking_lot
0.13.0 的 Condvar 正是在 before_sleep 中解锁 mutex。

现在每个 worker 预热 decoder 后，必须进入同一个 Condvar 的等待队列。
父线程只有在所有 worker 注册并释放 mutex 后才能取得它，再检查 notify_all 返回值
等于 worker 数。这把线程 parking 状态和 hash table 初始化确定地放在计数前。
原有申请/存活预算、start/done/release 测量边界、输出检查和 decoder 缓存策略保持。
没有增大容差或依赖 sleep、重复重跑来等待有利调度。

修复后 63 个 release 窗口及完整工作区通过。旧 `efc33ce`/`27cab0b` 的源码绑定记录
保持原样；本报告补充新暴露的预热边界。新窗口的进程基线包含已准备的 parking 状态，
不能直接与旧窗口的绝对 live_before 相减来宣布内存下降。

## 验证和后续顺序

stable Rust 1.99.1 / macOS ARM64，edition 2024、manifest 未声明 MSRV：

- 最终全工作区全特性 939 项通过、43 项原有忽略，排除自启动子测试重复汇总。
- 独立 no_std+SIMD 库 273 项、同模式增量更新集成测试 10 项通过。
- 增量更新原有功能回归覆盖两字段、四种矩阵、fast parity、跨块及尾部、非法输入
  和 no-op；新增目录另覆盖仅短尾和先短后长的双向 parity。
- 严格全目标全特性 Clippy、格式、diff 检查通过。
- 发布版 9.0.0 与当前 9.1.0 的 200 个非 fast 配置精确 parity、双向恢复及历史向量通过；
  原有 45 场景、270 个计数窗口实际执行通过。
- 全工作区中的严格 source-plan 摘要检查保持通过；encode.rs 不属于重写成员，
  本轮未修改诊断摘要或放宽校验。
- JSON 和分发包 allowlist 审计通过：核心包 71 个成员、WASM 包 6 个成员，测试、
  工具及本轮证据不进入分发包。

第 3 项普通 verify 分块仍需真实瓶颈与稳定速度证据；第 5 项缓存策略继续等待生产
线程数、大小交替频率及峰值预算。历史 ABBA 漂移失败记录保留，本轮没有重新计时、
跨平台实机吞吐或发布性能验收结论。

本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。
