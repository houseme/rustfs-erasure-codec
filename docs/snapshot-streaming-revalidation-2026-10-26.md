# 原始成员最终复核的流式读取

延续[目录扫描开销改进](collector-streaming-scan-follow-up-2026-10-26.md)，提交
`5879017` 将共享 Snapshot 的最终摘要复核改为固定缓冲读取，并去掉路径检查的
临时组件 Vec。[验证记录](../benchmarks/global-review/2026-10-26/snapshot-stream-validation.json)
绑定源码与完整兼容重放，本轮仍只进行本地验证。

## 只流式处理摘要重读

原 unchanged 为每个已绑定成员重新读取一份完整 Vec，然后比较 SHA-256。最终
复核不解码这些字节，完整缓冲没有继续参与统计、JSON 或源身份校验。

新实现每次 unchanged 复用一个 65,536 字节 scratch，流式计算每个成员的摘要。
初次读取原始字节、固定摘要、解析 JSON/文本的流程不变，不把流式摘要结果当作
可供解码的另一份输入，也不绕过原字节绑定。

打开文件时继续执行路径、符号链接、目录/叶子类型及 metadata 大小检查；重读
最多观察 MEMBER_LIMIT+1 字节，超过原 32 MiB 边界立即拒绝。Interrupted 重试，
其他 I/O 错误直接拒绝，不返回部分摘要。

原始空日志可计算空摘要；原本非空成员变空仍因摘要变化而拒绝。此前记录的所有
成员最终都要重新检查，任何缺失、替换、超限或摘要变化都不能完成 Snapshot。
Scoped 与累计原始字节预算保持原语义，未改协议及测量门槛。

## 路径组件直接迭代

member 原先先收集 components 到 Vec，再根据索引判断最后一个组件。现在使用
peekable 迭代器判定叶子：中间组件必须是普通目录，叶子必须是普通文件。

空路径、当前/父目录、绝对路径、符号链接和文件作为中间目录等仍拒绝。已有合法
路径解析语义和原始成员名称保持不变，未增加目录缓存或跳过逐组件检查。

此改动仅移除组件 Vec 及最终重读的文件大小 Vec。PathBuf、摘要字符串、初次
解析缓冲和报告仍会分配，不能称为零分配，也不能把 64 KiB 当作整个工具的内存上限。

## 验证与证据边界

新增四组契约覆盖零/单字节、块边界、多块摘要一致性，Interrupted、I/O 错误、
超限读取、成员变空/变大，以及路径与叶子类型。Snapshot 定向契约共 13 组。

私有 wrapper 使用已提交源码中未修改的 digest_reader 与 hex 实现，并用固定
scratch 连续核对四份空、小和多块原件。每份摘要与独立 SHA-256 一致，最大读取
请求为 65,536 字节。wrapper 只验证摘要与读取请求边界，没有计时或监测 allocator/RSS。

新工具实际启动八个原生假 worker，完成两轮 348-case 合成目录：2,784 个观测、
8,352 份原件绑定，独立读取器一致。collection、execution、statistics、functional
四类已有产物也只读重放通过，原件摘要前后未变。

stable Rust 1.99.1 完整 workspace all-features 为 1,034 项通过、43 项既有忽略；
严格 Clippy、格式、diff 和 Linux xtask 测试目标的离线交叉编译检查通过。交叉编译
没有在 Linux 执行 collector，也不建立资源隔离或真实 baseline 资格。

本轮没有测量总分配次数、RSS、吞吐或延迟。capture_provenance_verified 与
performance_accepted 保持 false；64 份 codec 源码及根 manifest、lock、build.rs
不变，未改编解码运行路径、并行阈值或缓存策略。
663 份 JSON、workspace 依赖继承、解释器入口与 package allowlist 审计通过，
核心 71、WASM 6 个成员不变。PR #116 的新 head CI 以实际状态为准。

## 后续顺序

正式计时与远程资源调整继续暂缓。下一步仍需确认可用的专用 Linux 保留资源，
在相同 native 构建与完整未变 baseline 下验证 collector 的实际监督开销和重复稳定性。

完整 ABBA、A2 漂移与逐布局门槛通过后，再按生产对象尺寸、并发度和峰值预算
评估 verify 分块及 oversized GF16 cache 策略；本地合成数据和固定缓冲大小不授予
候选性能归因。本轮未合并、打 tag 或发布。

## 初次摘要绑定的后续改进

本页保留提交 `5879017` 的实现和验证事实。之后初次摘要绑定与最终复核共用
Snapshot 持有的按需堆缓冲；具体入口、完整目录范围及独立验证见
[摘要绑定报告](snapshot-opaque-verification-2026-10-26.md)。
