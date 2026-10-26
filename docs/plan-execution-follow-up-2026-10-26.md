# 计划基准执行绑定与资源预检

按[统计输入协议的后续顺序](plan-statistical-inputs-follow-up-2026-10-26.md)，
提交 `57a306a` 增加 `replay-plan-execution` 和 `preflight-plan-execution`，
将完整原始 baseline、统计 corpus、资源观察和执行窗口放进一个固定摘要的封装。
[公开验证记录](../benchmarks/global-review/2026-10-26/plan-execution-validation.json)
记录来源、合成控制、拒绝路径及未验证边界。

## 复用一份快照和原校验模型

执行根目录使用 execution.json（plan-bench-execution-inputs-v1）。它固定 session、
corpus/samples.json 摘要、baseline 原始成员摘要、原 renderer/analyzer 身份、
窗口预算与全部阶段记录。原统计输入协议保持不变。

原 baseline 和统计重放核心现在都可以借用调用方的 Snapshot，原独立 CLI 保持
可用。所有摘要核对与模型解析使用同一份原始字节，最后统一重读，拒绝包括早期
成员在后续资源观察期间发生的修改。单成员 32 MiB、整份封装 256 MiB 的边界
保持不变，原始输入不被改写。资源上下文规则直接复用现有 sampling 模型。

## baseline 必须保留原终止状态

baseline/ 必须包含 14 个成员：provenance.json、qualification.json，以及四个
原 profile 在 calibration、A1、A2 的原始 samples。必须是带完整 runtime 的
baseline-qualification-v2，源提交固定为已发布 9.0.0，源/tool 的 commit、tree、
lock 和 harness 身份都需要完整值。

重放继续使用原校准、准备、布局、最短采样时间、分布和重复模型，覆盖四个
profile 的全部 126 个 case。原终止记录还必须满足：

- baseline_controls_passed 为 true，但这不能替代原始数据重算。
- 来源、tool、context、harness 和采样协议与 provenance 一致。
- common_iterations 与完整原校准计算一致，每个样本成员的路径与摘要一致。
- 全部 126 个终止重复检查唯一且完整，指标、A1/A2 中位数和漂移与原始重算一致。
- release_approval 为 false，candidate_attribution 为空；baseline 合格不是发布批准。

原终止状态失败、缺少完整原成员、短样本、宽分布、布局不匹配或重复漂移，任何一项
都拒绝。派生的成功摘要不能覆盖原终止失败，伪造的成功终止也不能覆盖原样本失败。
旧 baseline 独立描述性重放仍支持其历史输入，不因此获得新的预检资格。

## 绑定每次执行及其资源观察

corpus/ 使用上一轮的完整统计协议。执行列表必须逐轮按 A1→B1→B2→A2 排列，
每条绑定冻结来源、该 attempt 的紧凑有序 JSON 摘要，以及前后资源观察的摘要。
attempt 摘要计算保留数组顺序、按 JSON object key 排序，无额外空白；输入根摘要
仍固定原始文件字节。直接序列化已解析 Value，避免为摘要再复制整份 typed 目录。

观察路径固定为 contexts/01/A1/before.json、after.json 等。观察包含同一 session、
相对单调时钟 observed_ns 和现有资源 context。要求：

- Linux guest CPU partition scope、完整 libc/allocator/page/THP 策略及原固定采样设置。
- CPU 列表有序、唯一且至少两个，数量与 reservation 相符，编译器版本行与冻结来源一致。
- 全部阶段前后 context 与原 baseline context 完全相同，session 不能跨场拼接。
- 前次观察结束 ≤ 本次前观察 ≤ worker 开始 ≤ worker 结束 ≤ 后观察，全部在总窗口内。
- 相邻间隔、整个阶段包络和终止时间分别受预算限制；原始 times 总和必须能放入 worker 窗口。

阶段预算允许 1–86,400 秒，总窗口 1–604,800 秒，间隔 1–60 秒，阶段不能大于
总窗口。采样总时间比较只允许 1 ns 或 1e-12 相对量级的浮点舍入余量；溢出或
无法容纳原样本均拒绝。它检查记录内部的一致性，不等于重新执行或证明墙钟连续性。

## 只读 live preflight 的边界

replay-plan-execution 只检查固定输入。preflight-plan-execution 另外在重放前后
调用现有 Linux 观察器，读取当前 cgroup v2 exclusive child partition、affinity、
CPU、编译器和 runtime；前后必须与记录相同。合成输入不能通过 live preflight。
这个入口不创建 cgroup、不调整亲和性、不接管服务，也不启动 benchmark worker。

bindings_verified 表示这些输入绑定和门槛都通过，baseline_quality_passed 是重算
数值结果；preflight_passed 额外要求 recorded 声明和两个真实资源观察匹配。
origin 仍是输入声明，记录的时间也来自输入。两个端点不能证明中间无变化，CPU
partition 不证明物理宿主独占，版本行不等于完整运行产物证明。因此即使预检通过，
capture_provenance_verified 和 performance_accepted 仍为 false。

受监督 collector 尚未接入本入口，不能把它当作历史样本确实在当前保留分区中
执行的证明，也不授予发布或候选性能资格。原始摘要必须从可信准备过程另行保留。

## 本轮验证

新增 10 组执行契约，全部计划协议 40 组。包括稳定的 fake observer 正向预检、
合成数据拒绝、真实原始失败、终止失败、错配、资源变化、窗口异常、迟发修改及
原始时间装不下窗口的反例；fake observer 通过不冒充 Linux 实机采集证明。

私有源码副本复用已提交测试工厂导出合成 baseline，再组合已有完整合成 corpus。
真实 CLI 与独立读取器核对 126 个 baseline case、2,784 个统计观测、27,840 对
样本、8 次执行和 16 份上下文，结果一致。9 项实际拒绝控制全部拒绝：错误外部
pin、attempt 摘要、来源、session、资源 context、过短 worker 窗口、原终止失败、
缺少 baseline 成员、以及原始 baseline 漂移。原完整输入未改变。

本机实际调用 live preflight 因缺少 Linux reservation 而拒绝。没有更改资源，
没有启动 benchmark worker。所有新控制明确为 synthetic，不是新采集的真实
9.0.0 qualification 或计时结果。既有真实功能 campaign 的 1,392 个结果和原
2,784 个合成统计观测，也由改造后的独立 CLI 只读重放通过。

stable Rust 1.99.1 完整 workspace all-features 为 1,008 项通过、43 项既有忽略；
严格 workspace Clippy、格式和 diff 检查通过。64 份编解码源码及根 manifest、
lock、build.rs 与初始 PR head 相同；新工具和证据不进入核心或 WASM 发布包。
659 份 JSON、workspace 依赖继承、解释器入口及 package allowlist 审计通过，
核心 71、WASM 6 个成员不变。上轮 head f38e29e 的 CI 为 31 项成功、3 项跳过；
新 head 以 PR 实际状态为准。

## 使用与下一步执行顺序

```bash
target/debug/xtask replay-plan-execution --input target/plan-execution \
  --execution-sha256 "$PLAN_EXECUTION_PIN"

target/debug/xtask preflight-plan-execution --input target/plan-execution \
  --execution-sha256 "$PLAN_EXECUTION_PIN" --root . \
  --isolation-cgroup "$RESERVED_CPU_PARTITION"
```

下一步为独立统计采集实现受监督 collector：在同一会话中绑定预声明计划、真实
资源观察、固定二进制启动参数、退出/回收和全部原始产物，拒绝中途构建、换源或
不受监督地导入历史样本。先用受控假 worker 验证失败、取消、超时和产物保留。

只有在真实 Linux 资源条件、完整未变 baseline 及这些执行前提满足后才运行正式
计时；通过重复 baseline、A2 漂移和逐布局门槛后，再按生产预算推进 verify 分块
与 oversized GF16 cache 策略。本轮继续更新同一 PR，未合并、打 tag 或发布。

受监督采集、原件逐份绑定、取消/超时及统计失败提前终止见
[collector 跟进报告](plan-collector-follow-up-2026-10-26.md)。
