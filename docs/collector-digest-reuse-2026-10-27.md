# Collector 复用已绑定原件摘要

延续[Snapshot 摘要绑定](snapshot-opaque-verification-2026-10-26.md)，提交
`29122c7` 去掉 collector 接收原件后为两份记录重复计算的 SHA-256。
[验证记录](../benchmarks/global-review/2026-10-27/collector-digest-validation.json)
保留源码身份、完整目录对照和本地验证边界。正式计时与远程资源调整继续暂缓。

## 一次绑定供两份记录使用

artifacts::collect 通过 Snapshot.original 读取 benchmark、sample 和 estimates 原件时，
已经根据同一份原始字节计算并登记摘要。此前生成 case 的字段与 ArtifactCopy.sha256
时又分别调用 digest，两次重新遍历同一文件内容。

现在两份记录从同一个 Snapshot 借用 member_digest，各自保留协议所需的拥有字符串。
没有增加缓存、第二套摘要算法或 Snapshot API。统计成员的相对名称也复用同一个
String，供读取、摘要查找及原件路径构造使用。

原始字节仍用于复制和 StrictJson 解析，摘要不来自 JSON 重新序列化。最终
Snapshot.finish 仍逐份重读复核；源路径、普通文件类型、大小、累计预算及重复成员
规则继续生效。缺失统计成员仍保留空 pin 和已绑定的部分产物，不能通过完整性检查。

## 完整目录与重复工作范围

新工具实际启动八个原生假 worker，完成两轮 348-case 合成目录：2,784 个观测、
8,352 个 benchmark/sample/estimates 原件。独立读取器核对了原件 SHA-256、case pin、
copy pin 和 canonical 字节；完整统计 document、copy 列表及 canonical 字节与上一轮
采集一致。新采集的单调时钟 journal 和 controller 身份仍属于新一轮，没有重标历史来源。

按源码调用点与完整原件数量计算，本次接收阶段去掉 16,704 次额外整文件摘要调用。
原始输入共 2,543,952 字节，对应去掉两轮共 5,087,904 字节的重复摘要输入。
这是确定的调用点/输入目录统计，没有进行运行时 hash-call 监测或速度计时。

初始绑定和最终重读各自保留。两份 receipt 所需的字符串、成员表、文件字节、JSON
解析与报告仍会分配；不能将删除摘要调用等同于相同数量的分配减少或已测吞吐提升。

## 验证

新增两组契约：带 CRLF、额外空白和 128 KiB metadata 的原件必须以原始字节摘要同时
绑定两份记录，落盘副本逐字节一致；缺失 sample 必须保留空 pin、空均值和仅有的两份
已绑定 copy。前者还确认摘要与重新序列化 JSON 的摘要不同。

stable Rust 1.99.1 workspace all-features：1,039 项通过、43 项既有忽略。严格 Clippy、
格式、diff 和 Linux xtask 测试目标离线交叉编译通过。交叉编译没有执行 Linux collector。
collection、execution、statistics、functional 四类历史产物只读重放通过，原件未变。
665 份 JSON、依赖继承、解释器入口及 package allowlist 审计的结果记录在验证 JSON，核心/WASM
包成员仍为 71/6。64 份 codec 源码及根 manifest、lock、build.rs 不变。

没有测量 allocator 调用、RSS、延迟或吞吐；capture_provenance_verified、
performance_accepted 和 release_qualified 保持 false。

## 后续执行顺序

1. 本地继续收敛冻结输入的生命周期：完整 capsule 安装需要 archive/manifest/binary
   字节；只校验或只复制 binary 的消费者仍调用完整 load。后续按用途拆分读取，复用
   同一套 manifest 解码、摘要绑定、预算和最终变更检查，避免重新建立验证规则。
2. 正式计时继续等待专用或明确授权的 Linux 保留资源，再验证 native 同源构建、完整
   未变 baseline、collector 实际监督开销与重复稳定性。
3. 完整 ABBA、A2 漂移及逐布局门槛通过后，按生产并发和峰值预算评估 verify 分块及
   oversized GF16 cache；本地合成输入不授予这些策略的性能归因。

## 冻结输入生命周期的执行进展

上述本地后续项已完成，三个用途共用严格 manifest 解析及原件绑定；完整安装和
消费后释放、四阶段功能采集等验证见[生命周期报告](frozen-load-lifecycle-2026-10-27.md)。
