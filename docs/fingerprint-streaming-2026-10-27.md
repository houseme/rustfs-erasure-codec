# 文件与源码指纹的共享流式读取

PR #116 已合并，当前 main `07d543a` 的源码树与此前交付 head `cf91800` 一致。
本轮从该 main 新建后续分支，提交 `122a7ef`，对应
[草稿 PR #117](https://github.com/houseme/rustfs-erasure-codec/pull/117)。
[验证记录](../benchmarks/global-review/2026-10-27/fingerprint-stream-validation.json)
保留新的源码身份与完整重放，历史采集没有重标为合并后的提交。

## 指纹只消费字节流

冻结输入生命周期拆分后，collector 执行完 worker 仍通过 fingerprint::file 将整个
binary 读入 Vec 再计算摘要。native、sampling、views 与 harness 的原始源码文件
也按文件分别创建 Vec，仅供 hash.update 使用。

现在 update_file 打开输入后按需创建一个 64 KiB scratch，在一次指纹操作的多个
文件之间复用。opaque 原字节经过共享 artifact::hashing::update，读到 EOF；短读
继续读取，Interrupted 重试，其他 I/O 错误直接向上传递，不返回部分输入的最终摘要。

Snapshot 同样使用这一个读取循环，由调用方提供 limit+1 的有界 reader，再检查
实际读取字节数。其 32 MiB 成员限制、路径/类型、累计预算与最终变更检查保留。
普通文件指纹原本不受 Snapshot 成员限制，仍可处理超过 32 MiB 的文件；既有空文件、
符号链接跟随及每次重读语义不变，没有借机把两种调用合同合并。

源码文件顺序、相对名称、NUL 分隔与原始字节保持不变。需要文本替换和 block 选择
的模板/源文本仍拥有完整字符串；路径排序表、摘要字符串及报告继续分配，没有缓存。
小/空文件使用的固定 scratch 可能比原文件缓冲更大。这个修改提供与文件大小无关
的摘要 I/O scratch，并未测得整个工具的分配次数、峰值 RSS 或吞吐收益。

## 验证

新增四组契约覆盖共享循环的短读/中断/部分失败、文件块边界与 scratch 复用、超过
32 MiB 的普通文件可计算而 Snapshot 仍拒绝，以及符号链接、后续修改和读取失败。
定向 fingerprint 共 6 项、artifact 共 19 项通过；原有 native/sampling/harness 固定
摘要向量及 Snapshot 契约仍通过。

独立实现按旧有字节拼接规则核对当前仓库 native、sampling、views、harness 四种
生产指纹，全部一致。native 域包含 140 个原始文件。私有 wrapper 使用已提交的
共享 update 原实现，四份空、小、多块输入的摘要与独立 SHA-256 一致，最大读取请求
为 65,536 字节。未进行 allocator 或计时监测。

完整合成 collector 的八个 worker、2,784 个观测、8,352 个原件绑定经独立读取器
验证，case/copy 记录及 canonical 字节与上一轮一致。新的四阶段 functional campaign
完成 1,392 个场景，安装副本逐字节一致、冻结源未变；使用既有冻结二进制，没有
重新构建或重标 native worker。四类历史产物只读重放通过，原件未变。

stable Rust 1.99.1 workspace all-features：1,047 项通过、43 项既有忽略；严格 Clippy、
格式、diff 与 Linux xtask 测试目标离线交叉编译通过，后者没有执行 Linux collector。
667 份 JSON、依赖继承、解释器入口及 package allowlist 审计结果记录在验证 JSON，核心/WASM
包成员仍为 71/6。64 份 codec 源码、根 manifest/lock/build.rs、冻结加载与 worker
命令协议不变。

## 后续顺序与边界

正式计时及远程资源调整继续暂缓。待专用或明确授权的 native Linux 资源和完整未变
baseline 就绪后，验证 collector 实际监督开销及重复稳定性，再依据完整 ABBA、A2
漂移和逐布局门槛评估 verify 分块或 oversized GF16 cache。正式采集、性能及发布资格
保持未授予，本轮没有量化提速。

## Collector 观测拥有权跟进

本页保留指纹读取的原验证。后续 context 借用、journal 序列化与真实观测释放见
[观测拥有权报告](collector-context-ownership-2026-10-27.md)。
