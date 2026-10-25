# 调用者资源生命周期优化跟进

本轮落实[模块复审](global-module-review-2026-10-26.md)执行顺序中的第 2 项。
实现与测试提交为 `89194e49f9941f063fbd89921113826d618a6c9d`，基于该分支
已提交的流式收敛重构及诊断修正。远程 main 基线仍为 `b8d7a91`。

## 改进的使用路径

原通用 `verify_par` 在每次调用中创建 parity scratch。已有通用
`verify_with_workspace` 只提供串行入口；GF8 另有 `verify_with_workspace_opt`。
tower GF16 等泛型调用者需要自行管理 `Vec<Vec<F::Elem>>` 才能使用并行策略和复用存储。

新增 `verify_with_workspace_par`，在所有支持的泛型 Field 上提供同一复用路径：

1. 检查全部输入的计数和长度，非法输入不调整 workspace。
2. 按当前 codec 和 shard length 准备现有 `VerifyWorkspace`。
3. 委托现有 `verify_with_buffer_par`，沿用原有 family、kernel 和并行策略选择。

GF8 的 workspace-opt 入口也委托此实现，删除其单独的 family/resize 适配逻辑。
串行 Field API 没有增加 Send/Sync 要求。新增入口仅在 `std` 下存在，要求与现有并行
buffer API 相同的元素/输入线程约束。Leopard 仍沿用现有 family 分流。
每个并发操作持有自己的可变 workspace，不增加共享全局缓存。

普通 `verify`/`verify_par` 的调用行为不变；收益来自调用者明确选择复用 API。
workspace 仍持有与 parity 大小相应的 scratch，不能把每次申请减少写成峰值 RSS 下降。

## 优化构建的资源结果

[完整资源记录](../benchmarks/global-review/2026-10-26/parallel-workspace-resources.json)
绑定上述实现提交、源文件、测试、manifest、lockfile 和 build script。
同一组 codec/数据分别调用普通并行校验、caller-buffer 并行校验、新 workspace 并行校验。
构造、输入和两种可复用 scratch 均在计数窗口外；每种 API 先成功预热一次，再计数三次。
计数覆盖调用线程的 alloc/alloc_zeroed/realloc 请求，`max_parallel_jobs=1` 保证资源窗口
不把其他线程的分配遗漏为零。独立正确性场景另外使用两线程池。

| 有限域 / 布局 / 每片元素数 | 普通 verify_par 每次申请 | caller buffer | 新 workspace |
| --- | ---: | ---: | ---: |
| GF8 / 4+2 / 32,768 | 1 次 / 65,536 B | 0 | 0 |
| tower GF16 / 4+2 / 32,768 | 1 次 / 131,072 B | 0 | 0 |
| GF8 / 2+33 / 65 | 1 次 / 1,024 B | 0 | 0 |
| tower GF16 / 2+33 / 65 | 1 次 / 1,024 B | 0 | 0 |
| GF8 / 10+4 / 1,048,576 | 1 次 / 4 MiB | 0 | 0 |
| tower GF16 / 10+4 / 1,048,576 | 1 次 / 8 MiB | 0 | 0 |

全部 18 条记录一致。2+33 场景的普通 parity payload 位于内联存储中，申请的是宽引用表；
这里的 1,024 B 来自实际请求，不能按逻辑 payload 字节数估算。
资源测试没有测量 latency 或吞吐。上一轮漂移不通过的 ABBA 保留原始身份和结论，
不替换为本轮新提交的性能证据。

复现：

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test parallel_verify_workspace -- --nocapture
```

## 执行顺序的当前状态

| 项目 | 状态及依据 |
| --- | --- |
| 1. 稳定测量环境 | 仍未获得稳定速度基线；本轮没有重复失败环境中的速度采集，也没有调整并行阈值或算术内核 |
| 2. 调用者资源生命周期 | 已补齐通用并行 workspace 入口、收拢 GF8 适配并验证重复调用的分配收益；已有 codec/恢复 workspace/ShardSlot 路径保持 |
| 3. 普通 verify 有界 scratch | 已量化示例中的 4/8 MiB 每次申请；复用 API 能先消除重复申请，实际业务瓶颈和稳定速度对照仍不足以支持重写普通 verify |
| 4. 宽布局流式视图 | 已有系数表收敛提交；进一步的逐块输入/输出视图改变执行或借用模型，待实际宽布局占比与稳定测量支持 |
| 5. Leopard 缓存策略 | 仍需线程数、大小交替频率及峰值内存数据评估保留缓存的取舍，本轮没有引入新缓存策略 |
| 6. 文档入口 | 已补充中英文 API 指引、changelog 与当前跟进入口；历史记录不改标签 |

第 3～5 项的前置证据尚未满足，不将资源测试成功或新增 API 当作这些项已完成。

## 正确性与工具验证

新增回归覆盖 Classic 的 Vandermonde/Cauchy/JerasureLike/Custom，fast-one-parity，
GF8/tower GF16，长度 1/65/4097，workspace 多次缩放、首/尾损坏、非法计数/尺寸、
Leopard GF8/GF16。独立大分片测试在本机两个有限域均记录 `parallel=true, jobs=2`，
并检查正确数据与尾部损坏；这条记录是调度决策见证，不是线程利用率或加速比测量。

- 工作区全特性：917 项通过，43 项原有忽略，按顶层进程最终汇总排除子测试重复计数。
- 独立 no_std+SIMD 库：273 项通过。
- 发布版 9.0.0 与路径 9.1.0 的 200 个非 fast 配置：精确 parity、双向恢复及历史向量通过。
- 全目标全特性 Clippy（warnings denied）、Rust 格式与 diff 检查通过。
- JSON 与包 allowlist 审计通过；核心分发包 71 个成员、WASM 分发包 6 个成员，本轮工具/资源证据不进入包。
- 三种实际 native source-plan 输出及独立 dynamic renderer 重新绑定精确摘要，校验仍严格。
- 临时源码副本实际注入 4 个 stream-core 探针成员并编译运行新增 4 项 API/资源回归，通过。

上述记录来自 stable Rust 1.99.1/macOS ARM64。没有补充其他架构实机吞吐、长期 fuzz
或正式发布性能验收。本轮执行 commit；未 push、创建 PR、打 tag 或发布 crate。
