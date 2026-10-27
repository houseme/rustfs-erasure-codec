# Collector execution 单一账本

延续[观测拥有权](collector-context-ownership-2026-10-27.md)，提交 `8b99ea5` 去掉
collector 的第二份 execution 账本，并从 receipt 借用最终 envelope。
[验证记录](../benchmarks/global-review/2026-10-27/ledger-borrow-validation.json)
绑定源码、部分失败契约与完整目录的旧编码对照。正式计时继续暂缓。

## 一份拥有记录，按需生成视图

此前每条 execution Value 被 clone 到 Step.execution，原值放入另一个 executed Vec。
最终 json! envelope 又拥有一份数组及 plan metadata。两份并行账本依靠相邻写入维持
同步，持有相同阶段记录，增加不必要的拥有状态。

现在构造的 Value 直接移入 receipt.steps[*].execution。ExecutionAttempts 通过
Serialize::collect_seq 按 Step 顺序借用所有 Some 值，不 collect 第二张拥有表或引用表。
ExecutionEnvelope 借用这份账本、plan 字段和 samples pin，按旧 Value 的字母顺序
序列化；不先创建拥有的 envelope Value，也不复制 baseline_members。

None 表示尚未构造完成的 execution，不能凭 worker outcome 补成条目；已经完成的
Some 值即使随后触发 deadline/漂移等拒绝仍要保存，不按成功状态过滤。部分产物继续
保留，后续完整性校验继续拒绝失败采集。没有改变产生记录、采样或 clock 的位置。

receipt 仍拥有 execution、copy 与 outcome pin；统计 attempts 是另一种协议数据，
继续按原流程持有。编码 Vec、create_new、sync 和摘要也不变。这是执行记录的单一
拥有来源，不是整个 collector 的零分配或唯一状态表。

## 验证

新增字节契约覆盖五种账本状态与有/无 samples pin 的十种组合：空、未完成、完整、
中间缺项及显式 null 值。新 envelope 的 pretty 字节和摘要与旧 Value 构造完全一致。
同时加强两项实际错误路径：worker outcome 已保存但 clock 失败时 execution 为空；
完整记录后触发 post-worker deadline 拒绝时仍保存该条记录。

完整合成 collector 的八个 worker 生成 2,784 个观测、8,352 份原件绑定。独立读取器
验证统计、原件和执行链；case/copy 记录及 canonical 字节与上一轮一致。全部八条
execution 与 receipt 账本一致，按同一 plan、samples pin、记录及本轮结束时间重建旧
Value envelope，整份原字节一致。新的时间与 controller 身份仍属于本次运行。

按源码拥有关系，一次八 worker 完整目录去掉八次显式 execution.clone、独立
executed Vec 和临时 envelope Value；这是源码工作范围，不是实测 allocator 次数。
四类历史 collection/execution/statistics/functional 产物只读重放通过，原件未变。

stable Rust 1.99.1 workspace all-features：1,051 项通过、43 项既有忽略；严格 Clippy、
格式、diff 及 Linux xtask 测试目标离线交叉编译通过，未执行 Linux collector。
669 份 JSON、依赖继承、解释器入口和 package allowlist 审计见验证 JSON，核心/WASM 包
成员仍为 71/6。64 份 codec 源码及根 manifest/lock/build.rs 不变；共享摘要、冻结
加载、context 观测和 worker 协议未改。

## 后续顺序

正式计时与远程资源调整继续暂缓。待专用或明确授权的 native Linux 资源和完整未变
baseline 就绪，验证 collector 监督开销、重复稳定性及完整 ABBA/A2 漂移和逐布局
门槛后，再评估 verify 分块或 oversized GF16 cache。性能、正式采集与发布资格
保持未授予，本轮没有量化提速。
