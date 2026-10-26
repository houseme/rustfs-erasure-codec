# 冻结计划基准的连续功能 campaign

延续[上一轮执行顺序](frozen-plan-capture-follow-up-2026-10-26.md)，提交
`818a21389582885732aad59793c33e41511aad5c` 增加单个控制器的 A1→B1→B2→A2
采集与只读重放。它复用既有冻结、进程组所有者、阶段校验和原始成员快照，
不改编解码运行路径。[公开验证记录](../benchmarks/global-review/2026-10-26/plan-campaign-validation.json)
保留来源、完整功能结果、拒绝控制和验证边界。

## 固定配对，准备完成后进入窗口

`capture-plan-campaign` 接受两份已冻结输入及外部保留的 manifest 摘要。A1/A2
必须使用同一份基线，B1/B2 必须使用同一份候选；两者的编译器、features 和完整
case 目录必须一致。输入只从绑定的原始字节复制，四份自包含输入全部准备完成后
才开始窗口，执行期间不构建、不切换来源、不导入旧阶段，也不支持中断后续跑。

独立协议 `plan-bench-functional-campaign-v1` 固定 functional 模式，加入随机
campaign nonce。原始计划写入后立即输出 plan pin，再由同一个控制器按固定顺序
写入 start/end 记录。每个 start 绑定计划、阶段和上一阶段 end 摘要；end 绑定
start 和实际阶段 receipt。最后的 terminal receipt 固定全部阶段链和停止状态。
计划、start/end 和 terminal receipt 都只能新建，不覆盖已有记录。

所有阶段时间来自同一个 Instant 原点。默认阶段预算 120 秒、总窗口 600 秒、
相邻阶段间隔 5 秒；参数分别限制在 1–1200、1–4800、1–60 秒，阶段预算不能
大于总预算。原始阶段区间包含输入复核、功能子进程、回收及阶段结果校验。
控制器检查重叠、逆序、间隔、阶段和总窗口，清理开销超限也不能获得通过。
准备输入和最终离线重放不计入该窗口。

遇到失败、取消、超时或目录不完整，停止后续子进程并保存已产生的记录；如果存在
可写的终止路径，保存终止 receipt 并输出固定摘要。硬终止或写入失败可能只留下
部分 journal，没有完整可信 terminal receipt 时不能接受。失败后需换新目录重做。

这些是可信控制器的功能执行记录，不是主机证明，也不证明 CPU/内存隔离或跨系统
休眠的墙钟连续性。固定摘要必须从可信冻结/采集过程另行保留；临时从待审归档自取
摘要不能构成信任根。计时样本和发布性能验收需要独立协议。

## 一次快照重算全部原始成员

`replay-plan-campaign` 只读取调用方提供的 plan pin 和 terminal receipt pin。
它核对每个阶段的来源、顺序、哈希链、窗口、退出/回收、原始成员未变性及完整
Testing/Success 目录。整场使用同一个有界 Snapshot，每个成员仅从同一份字节
解析一次，最后重新核对全部已读成员，避免早期阶段在后续重放期间被修改。

单阶段和整场重放共享同一个功能结果校验器。重放不执行二进制、解包、构建或
修改输入，也不信任派生的 campaign.replay.json 或单阶段 replay.json。
performance_accepted 始终为 false，完整功能成功不授予速度、RSS 或发布资格。

## 本轮真实执行与反例

A 输入来自 `c8d1963`，B 输入来自 `818a213`；独立读取器将两份归档的全部
1,246/1,250 个文件与各自 Git tree/blob 及源摘要核对一致。二者编解码源码和根
Cargo.toml、Cargo.lock、build.rs 相同，因此这是不同来源提交下相同算法的功能
自对照，不能解释成候选提速结果。旧四个独立 capsule 没有被拼装成新 campaign。

| 固定阶段 | 输入 | 成功 case | Rust 重放 | 独立读取器 |
| --- | --- | ---: | --- | --- |
| A1 | A | 348 | 通过 | 通过 |
| B1 | B | 348 | 通过 | 通过 |
| B2 | B | 348 | 通过 | 通过 |
| A2 | A | 348 | 通过 | 通过 |

共 1,392 个功能结果。另在副本或外部参数上执行 11 项实际拒绝控制：错误计划 pin、
错误 terminal pin、阶段交换、跨 campaign 身份、重叠、间隔超限、阶段超限、总
窗口超限、错误 A/B 配对、部分失败加伪造派生成功，以及重复 JSON 字段。
Rust 和独立读取器全部拒绝，原 campaign 保持不变。

另用真实 A 基准和明确构造的取消 worker 做控制：B1 就绪后向自己创建的控制器
发送 SIGINT，B1 记录 cancelled 且 reaped=true；B2/A2 未启动，A1 的 348 个
成功结果、空/部分日志和终止 receipt 保留，离线重放保持失败。该合成 worker
只验证控制器取消行为，不是冻结构建的候选性能证据。单元契约另外覆盖非零退出
与真实超时，确认失败后不执行后续阶段且不覆盖原目录。

新增 7 组 campaign 契约，连同原阶段协议共 19 组。stable Rust 1.99.1 完整
workspace all-features 为 987 项通过、43 项既有忽略；严格 workspace Clippy、
格式和 diff 检查通过。上轮 head `2e333c2` 的远程 CI 已确认 31 项成功、3 项
跳过，包括 Windows stable/beta/nightly；新 head 的状态随 PR 实际 CI 更新。
核心 71 个、WASM 6 个 package 成员保持不变，工具与证据不进入发布包。

## 使用方式

先用已有 freeze-plan-bench 冻结两个已提交来源，并独立保存各自输出的摘要。
再在一个新目录执行：

```bash
target/debug/xtask capture-plan-campaign \
  --baseline target/plan-frozen-A --baseline-sha256 "$PLAN_A_PIN" \
  --candidate target/plan-frozen-B --candidate-sha256 "$PLAN_B_PIN" \
  --output target/plan-campaign \
  --stage-seconds 120 --window-seconds 600 --gap-seconds 5
```

把控制器输出的计划、终止 receipt 摘要另行保存在 CAMPAIGN_PLAN_PIN 和
CAMPAIGN_RECEIPT_PIN。后续只读重放：

```bash
target/debug/xtask replay-plan-campaign --input target/plan-campaign \
  --plan-sha256 "$CAMPAIGN_PLAN_PIN" --receipt-sha256 "$CAMPAIGN_RECEIPT_PIN"
```

采集当前要求 Unix；离线重放与原阶段协议一样可移植。旧单阶段入口保持可用，
但旧 capsule 缺少本协议的顺序/窗口记录，不能因此获得连续 campaign 结论。

## 下一步优化的执行顺序

1. 独立设计统计原始 sample/estimate 协议，绑定完整目录、来源和重复记录，保留
   分布、漂移和失败状态；不要给 functional-v1 增加计时通过开关。
2. 只有在资源隔离和完整未变基线条件满足后，才启用正式计时采集；先通过重复
   baseline、A2 漂移及逐布局门槛，再归因候选。
3. 以生产对象尺寸、调用方 workspace 生命周期、并发度和峰值预算决定后续
   verify 分块及 GF16 oversized cache 策略；缺少条件时保留现有策略。

本轮继续更新同一 [PR #115](https://github.com/houseme/rustfs-erasure-codec/pull/115)。
历史失败与原始测量保留，未合并、打 tag 或发布 crate。
