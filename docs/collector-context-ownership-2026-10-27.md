# Collector 资源观测与 journal 的拥有权

在[流式指纹](fingerprint-streaming-2026-10-27.md)之后，提交 `ad57e55` 处理 collector
before/after 资源观测中的重复拥有与持有期限。
[验证记录](../benchmarks/global-review/2026-10-27/context-borrow-validation.json)
绑定完整目录、旧编码逐字节对照与源码身份。正式计时和远程资源调整继续暂缓。

## 借用固定数据，保留真实观测

原 observe 对 synthetic reference 做深 clone；两次 journal 的 json! 又将 context
转换成临时拥有的 Value，再进入 save 的编码缓冲。真实观测返回的 Value 在写完
before journal 后，还会跟随迭代作用域继续持有到 worker 执行结束。

现在 observe 返回 Cow<Value>。synthetic 借用固定 reference，仍不请求 live 资源；
recorded 每次调用原 observer，拥有实际返回值并继续完整相等检查，错误直接返回。
相等检查后仍保留实际值，而非改用 reference；例如 +0.0 与 -0.0 相等，但原始编码
不同，必须保留 observer 返回的表示。没有缓存观测或降低调用频率。

ContextObservation 以借用字段直接序列化，按原 Value 的字母顺序声明 context、
observed_ns、session_sha256，保留旧 JSON 字节与摘要。before/after 的实际观测值
写完对应 journal 后显式释放；before 观测不会继续跨 worker 持有。

save 仍先编码为 Vec，再 create_new 写入、sync 和计算 pin。未改变失败处理顺序、禁止
覆盖规则、clock 顺序、执行窗口或后续 context-chain 检查。基准 reference、命令
环境、编码 Vec 和报告仍拥有数据，不是零分配流程。

## 验证与工作范围

新增三组契约覆盖 synthetic 原对象借用与禁止 live 调用、recorded 每次独立读取及
真实数值表示/错误、journal 旧 pretty 字节及写入一次的 pin。定向 observation filter
共 12 项通过，其中包含原有观测/时钟路径测试。

完整合成 collector 启动八个 worker，完成 2,784 个观测、8,352 份原件绑定。独立
读取器验证 context、执行链和统计门槛；case/copy 记录及 canonical 字节与上一轮
一致。用相同字段值重建旧 Value 编码，16 份 before/after journal 全部逐字节一致；
新 observed_ns 和 controller 身份仍属于本轮，没有重标旧产物。

按源码调用点计，一次完整八 worker 合成目录去掉 17 次 reference.clone 调用和
16 份临时 journal Value 构造；这不是运行时 clone/allocator 计数，也不代表相同
数量的 heap allocation 减少。真实 Linux recorded 采集未执行，live 路径由本地注入
observer 契约验证，实际资源资格仍待确认。

四类历史 collection/execution/statistics/functional 产物只读重放通过，原件未变。
stable Rust 1.99.1 workspace all-features：1,050 项通过、43 项既有忽略；严格 Clippy、
格式、diff 与 Linux xtask 测试目标离线交叉编译通过。668 份 JSON、依赖继承、解释器入口
及 package allowlist 审计见验证 JSON，核心/WASM 包成员仍为 71/6。64 份 codec
源码、根 manifest/lock/build.rs、共享摘要、冻结加载与 worker 协议不变。

## 后续顺序

正式计时继续等待专用或明确授权的 native Linux 资源与完整未变 baseline。之后
验证 collector 监督开销和重复稳定性，完整 ABBA、A2 漂移与逐布局门槛通过后，再
评估 verify 分块及 oversized GF16 cache。capture_provenance_verified、
performance_accepted 和 release_qualified 保持 false，本轮未量化提速。
