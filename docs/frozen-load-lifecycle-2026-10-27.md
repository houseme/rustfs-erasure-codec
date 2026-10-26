# 冻结输入按消费用途持有字节

延续[collector 摘要复用报告](collector-digest-reuse-2026-10-27.md) 的下一步，提交
`57e58a6` 将冻结输入加载按用途拆分，减少未使用原件的持有。
[验证记录](../benchmarks/global-review/2026-10-27/frozen-load-validation.json)
绑定源码与完整功能验证。正式计时继续暂缓，本轮没有量化提速。

## 一个解析入口，三种拥有结果

原 load 总是返回 manifest 原字节、archive、binary 三个 Vec。collector 准备 worker
只复制 binary；已经安装的 functional capture 只需源身份与执行后的变更检查，仍会
加载并持有完整输入。单次 capture/campaign 在部分移走 snapshot 后，其余字节也可能
继续留在调用方作用域中。

现在 frozen.rs 的 load_with 统一执行外部 pin 检查、Snapshot 创建、原始 manifest
绑定、StrictJson 解析与 Manifest.validate，然后只构造当前消费者需要的 payload：

| 入口与结果 | 保留的原件字节 | 消费方 |
| --- | --- | --- |
| load → Frozen<CapsuleBytes> | manifest、archive、binary | 单次完整安装、campaign 两个完整源 |
| load_binary → Frozen<Vec<u8>> | binary | collector 每个 worker 的 binary 副本 |
| load_verified → Frozen<()> | 无原件 payload | freeze 末尾检查、已安装 functional 执行 |

三个入口都绑定 manifest、archive、binary。binary-only 的 archive，以及校验入口的
archive/binary 使用现有 Snapshot 流式摘要 API；它们没有被跳过。manifest 原字节仍
用于同一次严格解析，除完整安装外在解析后释放。capture loader 原先的 UTF-8 与
重复键规则保持不变，没有改为另一套宽松 JSON 解码策略；离线 replay 的入口未改。

Snapshot 仍持有路径、成员摘要和按需摘要 scratch。上表只描述结果 payload 的拥有
关系，不表示整个工具零分配或零内存，也没有据此推导总申请次数或峰值 RSS。

## 消费后显式结束所有权

Frozen 的 snapshot 字段现在私有，调用方通过消费整个 Frozen 的 finish/into_snapshot
结束 payload 及已解析 manifest 的所有权，再进行最终复核或保留 Snapshot 跨 worker
执行。完整安装和 binary 复制仍使用已绑定的原始字节，之后不再让未使用 payload
跟随调用方作用域继续存活。

functional capture 只先保存 catalog 数量，再转为 Snapshot，启动 worker 时已经释放
原件 payload 和已解析 manifest。最终逐份重读不变；worker 修改 archive 时，即使
退出成功，receipt 仍记录 originals_unchanged=false，不能通过重放。

没有引入运行时模式标志、可选 payload、缓存、锁或 unsafe。路径/符号链接、普通
文件类型、非空源成员、32 MiB 单成员与累计预算、原始摘要及最终变更检查保持原规则。

## 验证

新增四组契约覆盖三种入口的精确原件与安装副本、错误 pin/空成员/重复键/UTF-16
拒绝、payload 释放后的三个源成员最终变化，以及实际 worker 修改归档后的失败
receipt。大归档、跨 64 KiB binary 与带 CRLF/空白 manifest 仍按原始字节绑定。

完整合成 collector 启动八个原生假 worker，生成 2,784 个观测、8,352 份原件绑定。
独立摘要/统计读取器一致；完整 case/copy 记录及 canonical 字节与上一轮一致。

另用先前冻结的功能二进制执行新的四阶段 campaign，完成 1,392 个功能场景。四份
安装 capsule 的 manifest/archive/binary 副本逐字节一致，独立阶段链读取器与只读
重放通过，冻结源原件未变。这些二进制保留原有源码身份，本轮没有重新构建最新
head 的 native worker，也没有赋予新源码性能资格。

collection、execution、statistics、functional 四类历史产物也只读重放通过，原件
未变。stable Rust 1.99.1 workspace all-features：1,043 项通过、43 项既有忽略；严格
Clippy、格式、diff 及 Linux xtask 测试目标离线交叉编译通过。交叉编译未执行 Linux
collector。666 份 JSON、依赖继承、解释器入口与 package allowlist 审计记录在验证 JSON，
核心/WASM 包成员仍为 71/6。64 份 codec 源码、根 manifest/lock/build.rs 及共享
Snapshot 实现不变。

没有测量 allocator 调用、RSS、吞吐或延迟。capture_provenance_verified、
performance_accepted、release_qualified 保持 false。

## 后续顺序

本地冻结输入生命周期拆分已完成。正式计时继续等待专用或明确授权的 native Linux
保留资源，再验证完整未变 baseline、collector 实际监督开销及重复稳定性。
完整 ABBA、A2 漂移与逐布局门槛通过后，再按生产并发及峰值预算评估 verify 分块
和 oversized GF16 cache 策略。
