# 摘要绑定复用 Snapshot 缓冲

延续[最终流式复核](snapshot-streaming-revalidation-2026-10-26.md)，提交
`bcc77fb` 将初次绑定中只使用摘要的读取也改为流式处理。源码、独立完整重放及
验证边界见[验证记录](../benchmarks/global-review/2026-10-26/snapshot-opaque-validation.json)。
本轮按既有选择只提交本地改进，正式计时和远程资源调整继续暂缓。

## 同一个所有者负责绑定与复核

此前 archive、binary、只核对摘要的日志和 worker 原件通过 binary_bound/raw_bound
返回完整 Vec，调用者随后立即丢弃。现在 verify_binary/verify_raw 返回单位值，
不为这些输入创建文件大小的 Vec；需要复制、解析 JSON 或检查 stdout 内容的路径
仍从同一次读取的原始字节完成绑定和解码。

Snapshot 按需创建一个 65,536 字节的堆缓冲，初次摘要绑定、之后的成员及每次
unchanged 共用它，随 Snapshot 释放。上一轮的最终复核使用每次调用的栈数组，
本轮改为由 Snapshot 持有，以便初始绑定与多次最终复核复用同一缓冲。空 Snapshot
不分配它，也没有增加线程共享缓存、锁或 unsafe。

digest_reader_counted 在有界摘要计算时返回实际读到的字节数。两类读取共用 account：
累计及 scoped 子预算仍按实际字节记账，错误 pin 或超预算不能登记为已绑定成员。
重复成员直接拒绝；普通原件必须非空，原始日志允许为空。逐组件路径、符号链接、
目录/叶子类型、32 MiB 成员上限、limit+1 超限探测和 Interrupted 重试继续生效。

最终仍逐份重新读取所有登记成员，任何缺失、超限或摘要变化都拒绝完成。流式摘要
没有替换需要解码的原始字节，也不缓存目录检查或减少复核次数。

## 完整目录的适用范围

独立读取八个假 worker 的完整 receipt 和冻结源目录，确认一次只读 collection
重放中的以下 8,380 个唯一成员使用摘要专用入口：

| 成员 | 数量 |
| --- | ---: |
| 两个冻结源的 archive 和 binary | 4 |
| worker binary | 8 |
| worker stdout/stderr | 16 |
| 已与 canonical 绑定的 worker 原件 | 8,352 |

其中 8 个 stderr 为空，8,372 个成员非空。这个统计表示入口覆盖范围，不能视为
减少了 8,380 次 heap allocation；空日志原先也不一定分配。canonical 数据需要
解析，继续使用原来的拥有字节读取。保护 plan 的初始绑定和 functional stderr
同样切换入口，但没有计入上表的一次 collection 重放。

PathBuf、摘要字符串、成员表、初始解码及报告仍会分配。64 KiB 仅为每个 Snapshot
的摘要 scratch 大小，既不是整个工具的内存上限，也不表示已测得 RSS 或总分配量。

## 验证

新增三组混合读取契约，验证多块摘要、按需分配及指针复用、空日志、重复成员、
错误 pin、累计/子预算和最终变更拒绝；Snapshot 定向契约共 16 组。

使用已提交源码中未修改的 digest_reader、digest_reader_counted 和 hex 编译私有
读取请求 wrapper，四份空、小、多块原件摘要均与独立 SHA-256 一致，最大读取请求
为 65,536 字节。没有计时或 allocator/RSS 监测。

新工具实际启动八个原生假 worker，完成两轮 348-case 合成目录：2,784 个观测、
8,352 份原件到 canonical 的绑定，独立读取器一致。collection、execution、statistics、
functional 四类既有产物只读重放通过，原件摘要前后未变。假 worker 和合成 baseline
不建立真实 native 源码身份或性能资格。

stable Rust 1.99.1 workspace all-features：1,037 项通过、43 项既有忽略；严格 Clippy、
格式、diff 与 Linux xtask 测试目标离线交叉编译通过。后者只证明编译，没有 Linux
执行或隔离资源资格。664 份 JSON、workspace 依赖继承、解释器入口及 package allowlist
审计结果与数量记录在同一验证 JSON；核心/WASM 的包成员仍为 71/6。

64 份 codec 源码及根 manifest、lock、build.rs 不变。capture_provenance_verified、
performance_accepted 和 release_qualified 保持 false。没有吞吐、延迟或全平台最优结论。

## 后续顺序

正式计时仍等待专用或明确授权的 Linux 保留资源，然后验证 native 同源构建、完整
未变 baseline、collector 实际监督开销和重复稳定性。完整 ABBA、A2 漂移及逐布局
门槛通过后，再按生产并发和峰值内存预算评估 verify 分块或 oversized GF16 cache。
本地合成结果不授予这些运行策略调整的性能归因。

## Collector 接收阶段的后续改进

本页保留提交 `bcc77fb` 的验证事实。接收阶段复用 Snapshot 已绑定摘要、去掉两份
记录各自的重复计算，详见[摘要复用报告](collector-digest-reuse-2026-10-27.md)。
