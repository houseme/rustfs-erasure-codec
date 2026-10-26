# 计划基准统计输入协议与只读审计

按[连续功能 campaign 的后续顺序](plan-campaign-follow-up-2026-10-26.md)，
提交 `aaae989c438e9418bba2368429e2fd0cd619bccb` 实现独立的统计输入协议和
`replay-plan-samples`。它只审计已有原始数据，不启动计时采集，不扩展原功能
协议的性能资格。[公开验证记录](../benchmarks/global-review/2026-10-26/plan-statistical-inputs-validation.json)
记录完整合成目录、独立读取器、反例与验证来源。

## 来源、完整目录与重复记录

输入根目录的 samples.json 使用 `plan-bench-statistical-inputs-v1`，外部固定
其 SHA-256。它引用 sources/A 和 sources/B 的原始冻结 manifest、归档和二进制；
重放复用既有来源校验器，要求两份输入的编译器、features、完整有序目录一致。
A1/A2 固定使用 A，B1/B2 固定使用 B；每轮都按该顺序声明，不能删去失败轮次
或以另一轮成功数据替代。冻结来源身份不等于这些样本确实由该二进制执行产生。

| 记录 | 必要字段与约束 |
| --- | --- |
| samples.json | schema、origin、baseline_sha256、candidate_sha256、repetitions、minimum_samples、confidence_level、max_ci_width、max_drift、attempts |
| origin | synthetic 或 recorded，均为输入声明；不能据此授予采集来源证明 |
| 策略 | 1–9 轮，最少样本数 10–100,000，置信水平在 0 与 1 之间，宽度/漂移阈值在 0–1 之间 |
| attempt | repetition、stage、manifest_sha256、outcome、originals_unchanged、cases；保留实际声明的失败和回收状态 |
| case | case_index 与 benchmark_sha256、sample_sha256、estimates_sha256；必须覆盖冻结目录且严格按索引排序 |

每项 case 的原始成员路径由校验器生成：

```text
records/01/A1/0000/benchmark.json
records/01/A1/0000/sample.json
records/01/A1/0000/estimates.json
```

编号对应轮次、阶段和完整目录索引，不接受任意输入路径。benchmark.json 的
full_id 必须匹配冻结目录，其余元数据保留在原始字节中。每轮每阶段必须包含
全部 case；不完整或失败的 attempt 可以保留，但不能通过完整性检查。

## 从原始样本重算，保留诊断失败

按当前依赖 Criterion 0.8.2 的 SavedSample/Estimates 布局，校验器要求 iters/
times 等长、有限且为正。迭代次数必须是 1–2^53 的精确整数，Linear 使用等差
倍数，Flat 使用相同次数。Linear 必须带 slope，Flat 的 slope 为 null 或省略。

固定指标是每个原始 time/iters 的算术均值，单位 ns/iteration，不使用默认
"typical" 指标，也不推导吞吐量。重算均值、中位数和 Linear 回归斜率，与原
point_estimate 进行相对误差 1e-9 的一致性检查。中间计算使用归一化，避免有限
原始值在求和或点积时溢出。报告保留样本数、均值、中位数、最小/最大值和均值 CI。
完整原始分布保持在绑定的 sample.json 中，不筛除离群点或重写原文件。

所有 estimate 均检查有限值、区间顺序、置信水平和非负 standard_error；MAD/
标准差允许零值。这里不独立重做随机 bootstrap，也不声称验证其随机分布、MAD/
标准差 point 的统计正确性；原始 estimates.json 和这些限制一并保留。

CI 宽度为 (upper-lower)/mean_point。对每个 case、每轮分别计算 A2/A1-1 和
B2/B1-1，并计算各轮 A 配对均值、B 配对均值相对第一轮的变化。每项都检查
绝对漂移阈值，不以跨 case 平均值、最快轮次或部分成功记录覆盖失败。
只要有宽度或漂移失败，diagnostics_passed 即为 false，CLI 非零退出。

## 完整性与性能资格分开

派生报告使用 `plan-bench-statistical-audit-v1`，固定输入摘要、目录规模、轮数和
原策略。integrity_accepted 只表示声明和原始字节/确定性统计的一致性；
diagnostics_passed 另外要求所有 case 的宽度和漂移检查通过。
performance_accepted 与 capture_provenance_verified 始终为 false。

原 worker outcome、原始成员未变声明、origin 都由输入提供；没有受控执行封装、
资源隔离证据和完整未变基线时，不能把这些声明当成实际采集来源证明。本入口
不读取功能 campaign 的成功结果来提升统计资格，也不读取 summary.json 等派生
成功标志。输入不允许额外的性能通过开关。

重放只读原始成员，不执行二进制、不解包或构建、不修改输入。整场使用同一个
Snapshot：单成员上限 32 MiB，总原始字节上限 256 MiB，最后重读全部已绑定成员。
摘要、编码、路径或预算失败立即拒绝；能解析的数值诊断失败与部分 attempt 则
留在完整报告中。任何失败都不删除原始输入。

## 本轮验证

完整目录的协议控制明确标为 synthetic：348 个 case、两轮、8 个 attempt，
共 2,784 个观测、27,840 对合成样本及 8,352 份 Criterion 格式成员。使用既有
`c8d1963`/`818a213` 冻结来源作为身份引用，没有把合成样本说成它们的真实执行结果。
Rust CLI 与独立读取器逐项重算结果一致，性能资格与采集来源证明均未授予。

在完整目录副本和外部参数上执行 13 项真实拒绝控制：错误输入 pin、阶段乱序、
错误来源配对、错误 case 身份、原始均值不一致、分数迭代次数、非有限样本、
过宽 CI、阶段内漂移、跨重复漂移、失败 worker、部分输入加伪造成功 summary，
以及重复 JSON 字段。两份读取器全部拒绝，原完整 corpus 保持不变。

新增 11 组统计契约，全部计划协议共 30 组；另直接重算历史恢复路径四阶段的
原始 Criterion 记录，验证数值读取兼容性，没有重标历史来源或改变其既有失败结论。
stable Rust 1.99.1 完整 workspace all-features 为 998 项通过、43 项既有忽略，
严格 workspace Clippy、格式和 diff 检查通过。编解码的 64 份源码及根 manifest、
lock、build.rs 与初始 PR head 相同；核心 71、WASM 6 个 package 成员不变。
658 份 JSON、workspace 依赖继承及解释器入口审计通过；原真实功能 campaign
的 1,392 个结果也由新工具只读重放通过，保留 performance_accepted=false。

上轮 head `3ffdaf5` 的远程 CI 已确认 31 项成功、3 项跳过。新提交的 CI 以
[PR #115](https://github.com/houseme/rustfs-erasure-codec/pull/115) 当前状态为准。

## 使用与后续顺序

从可信准备过程另行保存 samples.json 的摘要到 PLAN_SAMPLES_PIN，再执行：

```bash
target/debug/xtask replay-plan-samples --input target/plan-samples \
  --manifest-sha256 "$PLAN_SAMPLES_PIN"
```

不能从待审目录临时自取摘要作为信任根。本协议只提供只读审计，没有新增计时
collector；所有完整目录控制都是合成数据，本轮没有运行统计计时基准。

下一步先复用已有资源/采样校验模型，为独立统计采集定义受控执行封装，绑定资源
隔离、完整未变基线、执行窗口及原始 sample/estimate 产物。前置条件不满足时
保留明确拒绝，不把本轮数据完整性结果提升为真实性或性能结论。

满足条件后先通过重复 baseline、A2 漂移及逐布局门槛，再依据生产对象尺寸、
调用方 workspace 生命周期、并发度与峰值预算推进 verify 分块和 oversized GF16
cache 策略。历史资源记录及所有失败保留，本轮未改运行时策略、合并、打 tag 或发布。

完整原始 baseline、执行窗口和只读资源预检的绑定及后续顺序见
[执行绑定跟进报告](plan-execution-follow-up-2026-10-26.md)。
