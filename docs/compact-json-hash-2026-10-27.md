# Compact JSON 的流式摘要

延续[统计模型拥有权](statistical-model-ownership-2026-10-27.md)，提交 `24897c6` 去掉
三处仅用于计算摘要的完整 compact JSON 编码 Vec。
[验证记录](../benchmarks/global-review/2026-10-27/compact-hash-validation.json)
绑定源码、流式观察、旧 pin 与完整采集。正式计时继续暂缓。

## 只将摘要输入流式化

collector 的 typed attempt pin、统计重放的原 Value attempt pin，以及冻结源码的
排序成员目录 pin，此前均先 to_vec，再对完整字节计算 SHA-256。

现在三者共用 artifact::hashing::compact_digest：同一个 serde_json compact serializer
经配置为 8 KiB 的 BufWriter 将字节送入内存 SHA writer，显式 flush 尾部后才返回摘要。
它不增加排序、归一化或输入大小限制；输入的原有 Serialize 顺序仍决定确切字节。
序列化/flush 失败直接返回错误，不输出部分 pin，每次调用拥有独立 hash 状态。

原始 digest/hex 函数一并集中到 hashing，Snapshot 保留原路径重导出；函数实现、
原始文件读取循环及 Snapshot 的边界规则未改。无需将通用哈希实现反向依赖 Snapshot。

用于发布文件的完整编码、create_new、sync 与原件摘要继续保留。没有把文件发布改为
边编码边写入。配置的 8 KiB 只描述摘要输出缓冲，不是整个工具内存上限；大字符串
可直接交给内层 writer，不要求下游写入请求也小于 8 KiB。

每次操作仍会分配缓冲，短输入的 scratch 可能比原编码更大。元数据图、解析输入、
pretty 文档及排序目录仍拥有数据；序列化和最终发布仍是各自的处理步骤。本轮只
去掉 hash-only 的完整编码副本，未测分配次数、RSS、CPU 或吞吐收益。

## 验证

新增四组契约覆盖空/块边界/多块/转义/Unicode/数值编码与旧 to_vec 摘要一致；大序列
在序列化完成前就发生下游写入，尾部字节完整；序列化或 flush 失败拒绝 pin，之后的
独立调用正常；实际 source_hash 保留排序原成员编码、新鲜度与符号链接拒绝。
既有 typed attempt 字节契约也显式核对新 helper 与旧 compact pin。

完整合成 collector 的八个 worker、2,784 个观测、8,352 份原件绑定经独立读取器验证。
writer/replay 的全部 attempt pin 一致，case/copy 记录、canonical 字节及整份
samples.json 与上一轮逐字节相同。新的 execution 时间与 controller 身份属于本轮。
四类历史产物只读重放通过，原件未变。

stable Rust 1.99.1 workspace all-features：1,057 项通过、43 项既有忽略；严格 Clippy、
格式、diff 及 Linux xtask 测试目标离线交叉编译通过，未执行 Linux collector。671 份 JSON、
依赖继承、解释器入口及 package allowlist 审计见验证 JSON，核心/WASM 包成员仍为
71/6。64 份 codec 源码、根 manifest/lock/build.rs、统计模型、冻结加载、context、
execution 账本及 worker 协议不变。

## 后续顺序

正式计时与远程资源调整继续暂缓。待专用或明确授权的 native Linux 资源和完整未变
baseline 就绪，再验证 collector 监督开销、重复稳定性及完整 ABBA/A2 漂移和逐布局
门槛，之后评估 verify 分块或 oversized GF16 cache。正式采集、性能及发布资格
保持未授予，本轮没有量化提速。
