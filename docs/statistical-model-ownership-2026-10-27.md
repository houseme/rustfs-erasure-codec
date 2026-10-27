# 统计采集与重放共享类型模型

延续[execution 单一账本](collector-execution-ledger-2026-10-27.md)，提交 `105f393`
将统计 case、attempt、document 的中间 Value 对象图改为共用已有严格读取模型。
[验证记录](../benchmarks/global-review/2026-10-27/typed-statistical-validation.json)
绑定源码、原字节对照与完整采集。本轮正式计时继续暂缓。

## 类型与数据各保留一份

此前 Produced.cases 为每个目录项构造四字段 Value Map；生成 attempt 的 json! 会
再复制 case 数组，plan.document 的 json! 又复制所有 attempt 及其嵌套数据。

现在读取端既有 Entry、Attempt、Document 同时用于序列化。采集直接填充 Entry 的
Option pin，case Vec 移入 Attempt，attempt Vec 再移入 Document，不经过这些临时
Value 图。缺失 pin 仍输出 null，目录索引、原件复制、原始摘要与最终复核不变。

Plan 的配置校验直接调用同一 Document.validate；报告直接比较经过验证的类型字段，
去掉构造 Value 再反序列化的往返。读取端仍从 StrictJson 原字节解码，三个模型保留
deny_unknown_fields；数值限制和完整目录/统计检查继续执行，没有另建解析规则。

Outcome 移入 attempt 前仅保存原来的纯成功谓词，仍在原 budget/worker 拒绝位置
使用该结果。已完成/部分失败的产物保留顺序与后续 worker 停止规则不变。

## 兼容两种既有 outcome 编码

attempt 的 compact 摘要原先来自排序后的 Value，内嵌 outcome 顺序为 exit_code、
reaped、reason；独立 outcome 文件则保持结构体顺序 exit_code、reason、reaped。
因此只有内嵌字段使用借用的规范序列化，不改变 Outcome 的独立编码，也不放宽摘要
核对。Entry、Attempt、Document 的其余字段按旧排序声明，保留 compact pin 和 pretty
文件字节。这三个模型不派生 Clone，避免重新引入整份目录复制。

schema 与两份冻结源 pin 字符串仍拥有数据；原始 JSON 解析、字节复制、copy receipts
及最终编码 Vec 也继续分配。这是去掉中间对象图，不是零分配或已测吞吐结论。

## 验证

新增两组契约覆盖空/部分/完整 attempt 与 document 的旧 compact/pretty 字节、摘要、
缺失 pin、Unicode、负零及独立 outcome 编码；六个配置字段不匹配与八种非法数值仍
拒绝。73 项 plan benchmark 契约通过，包含未知字段、重复键、部分产物与停止门槛。

完整合成 collector 启动八个 worker，生成 2,784 条 case 元数据、8,352 份原件绑定。
独立读取器核对 canonical attempt pin、原件和执行链；case/copy 记录、canonical
字节均与上一轮一致，整份 samples.json 与旧 Value 采集器输出逐字节一致。新的
execution 时间及 controller 身份仍属于本次运行，没有重标旧产物。

按源码数据流，本次完整目录将 2,784 个 producer case Map 换成类型条目，去掉八份
临时 attempt Value 和最终 document Value；这是拥有范围统计，不是实测 allocator
计数。四类历史产物只读重放通过，原件未变。

stable Rust 1.99.1 workspace all-features：1,053 项通过、43 项既有忽略；严格 Clippy、
格式、diff 及 Linux xtask 测试目标离线交叉编译通过，未执行 Linux collector。670 份 JSON、
依赖继承、解释器入口及 package allowlist 审计见验证 JSON，核心/WASM 包成员仍为
71/6。64 份 codec 源码、根 manifest/lock/build.rs、共享摘要、冻结加载和 worker
协议不变。

## 后续顺序

正式计时与远程资源调整继续暂缓。待专用或明确授权的 native Linux 资源与完整未变
baseline 就绪，再验证 collector 监督开销、重复稳定性及完整 ABBA/A2 漂移和逐布局
门槛，之后评估 verify 分块或 oversized GF16 cache。正式采集、性能与发布资格
保持未授予，本轮没有量化提速。
