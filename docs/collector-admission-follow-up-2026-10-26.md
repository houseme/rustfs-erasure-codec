# Collector 原生准入与完整阶段预算

延续[collector 执行顺序](plan-collector-follow-up-2026-10-26.md)，本轮先只读核实
Linux 条件，再完成可独立验证的代码改进。提交 `9ab1fb8` 修复原生编译身份准入、
采集与重放预算不一致及异常时退出记录保留。
[公开验证记录](../benchmarks/global-review/2026-10-26/collector-admission-validation.json)
区分本地验证、交叉编译和仍未进行的正式计时。

## 资源检查的实际边界

有限深度只读检查观察到活跃负载，没有确认可供本任务使用的专属 CPU 分区。这不
证明整个宿主机不存在其他分区，也不证明当前测试资源已经合格。原始检查记录私有
保留，公开记录不含地址、路径、进程身份或机器指纹。

本轮不创建分区、不调整亲和性、不接管服务，不运行正式计时。合格 Linux 同源
collector 基线仍待资源确认；合成测试不能替代这一步。

## 同一预算模型用于采集和重放

旧代码在离线重放时检查完整阶段区间，但采集时主要约束 worker 自身。worker
已经退出后，产物解析、复制或后续观察仍可能令整个阶段超限；旧采集器会继续
启动后续 worker，直到最终重放拒绝。

回归先用可注入的单调时钟复现：第一阶段包络超限，旧采集器仍完成四个 worker，
completed=true，最终 accepted=false。修复后只启动第一个 worker，保存其结果、
原始产物和失败 receipt，再停止后续阶段。

新的预算组件同时服务于统计采集与 execution 重放：

- 预算范围、顺序、阶段包络、相邻间隔和总窗口只定义一次。
- worker 的剩余时长扣除观察到启动之间已经消耗的阶段预算，并受总预算限制。
- 阶段后观察使用一次固定时钟读数写入原始记录，立即校验完整包络；下一阶段
  的间隔从这个相同读数计算，包含记录写入等控制器开销。
- 最终 samples 保存后再检查终止窗口，超限保持失败，并保留已产生的完整记录。

采集器不把“子进程退出及时”解释为“阶段完成及时”。时间检查仍是控制器监督
边界，不是内核级实时保证，也不授予性能资格。原历史记录的预算字段和协议未变。

## recorded 启动前核对完整原生身份

同一个 rustc 版本行可能来自不同 host、commit 或 LLVM 构建。recorded 路径现在
复用原 Linux 资源观察后，再获取 stable rustc 的完整 --version --verbose：

- 冻结的完整编译器身份必须与本机观察逐字一致。
- host 必须唯一、有效且为 Linux，并与构建 collector 的 Cargo TARGET 一致。
- 缺失或重复 host、跨平台冻结身份、同版本但 commit/LLVM 不同都在 worker 前拒绝。

xtask/build.rs 固定 collector 的目标身份，原资源 context 仍保留原来的版本行格式，
不改变历史上下文结构。此检查针对可信冻结输入声明，不独立解析 ELF/Mach-O，
也不构成二进制构建或主机真实性证明。

synthetic 控制和只读历史重放保持原语义。无法匹配时应在目标 Linux 环境用同一
工具链重新冻结输入，并构建匹配 native host 的 collector，而非放宽身份检查。

## 异常结果保留与验证

时钟观察失败也不能丢掉已回收 worker 的结果。现在先保存 owned outcome，再处理
时钟错误；失败会话保存退出/回收状态，停止下一阶段，产物继续保留。

新增八组契约：三个编译身份反例组、两个共享预算组，以及完整阶段超限、编译
错配零 worker、时钟失败仍保留 outcome 三个集成组。全部计划协议共 57 组。
stable Rust 1.99.1 完整 workspace all-features 为 1,026 项通过、43 项既有忽略，
严格 workspace all-targets/all-features Clippy、格式及 diff 检查通过。

已安装的 x86_64-unknown-linux-gnu target 对 xtask 离线交叉编译检查通过，构建输出
中的 native target 绑定正确；这只是编译证据，没有在 Linux 执行 collector。

新工具另外实际启动八个原生假 worker，完成两轮 348-case 合成目录，得到 2,784
个观测和 8,352 份原件绑定，独立读取器核对通过。原 collection、execution、
statistics、functional 四类既有产物只读重放通过，原件未变。

capture_provenance_verified 与 performance_accepted 保持 false，没有真实 Criterion
计时或新的已合格 baseline。64 份编解码源码及根 Cargo.toml、Cargo.lock、build.rs
不变；工具的 build.rs 改动用于记录 native target，未改变发布包运行路径。
661 份 JSON、workspace 依赖继承、解释器入口及 package allowlist 审计通过，
核心 71、WASM 6 个成员不变。上轮 head 5e72488 的 CI 为 31 项成功、3 项跳过；
新 head 以 PR 当前状态为准。

## 下一步顺序

下一步仍是确认专用或明确获准使用的 Linux 保留资源，在目标环境冻结 native
Criterion 产物和同源输入，完成完整未变 baseline、监督开销和重复稳定性验证。

当前不据本地合成或交叉编译结果推进候选性能归因。真实完整 ABBA、A2 漂移和
逐布局门槛通过后，才按生产对象尺寸、并发度和峰值预算评估 verify 分块及
oversized GF16 cache 策略。本轮未合并、打 tag 或发布。

预算扫描的流式遍历、metadata 路径保留与 argv 复用见
[扫描开销跟进报告](collector-streaming-scan-follow-up-2026-10-26.md)。
