# 冻结计划基准的功能阶段与离线重放

按“先创建 PR，再继续执行”的顺序，首先推送已有优化并创建
[PR #115](https://github.com/houseme/rustfs-erasure-codec/pull/115)，初始 head 为 `4e15407`。
随后实现提交 `c8d196326c109c21331254a178532fb12c9f722b`，增加指定 Git 来源构建、
功能阶段采集及离线重放。Windows 测试输入修复另提交为 `805d2d5`。

[公开验证汇总](../benchmarks/global-review/2026-10-26/frozen-plan-capture-validation.json)
记录源提交、完整目录、四阶段功能结果和拒绝控制。原始构建记录、归档、二进制、
日志及固定摘要在私有证据目录保留；公开汇总不包含私有路径、二进制摘要或机器身份。

## 构建一次，固定来源和完整目录

`freeze-plan-bench` 从指定 Git 提交生成源归档，在新目录中解包并构建独立 bench。
它仅使用已提交来源，未提交的工作树修改不进入冻结输入。使用 stable、locked/
offline、固定 std+simd-accel 和宿主 target；编译阶段不属于任何测量阶段。

构建前后核对完整源目录摘要，要求 Cargo 返回唯一、成功完成且属于该源目录/目标
目录的 reconstruct_plans 可执行文件。随后固定源归档、二进制、源码和编译器身份，
并运行该二进制的 --list，逐项核对源码中的完整规范目录。

新增 cases.txt 是可移植的目录契约；bench 本身也从共享 fixtures 生成全部 ID，
核对它与该契约一致。采集器因此不需要复制 Rust shape 布局规则。
本次真实构建的目录为 348 项；独立检查器从归档重算 source digest，并将全部
1,246 个文件的原始字节与声明 Git 提交的 blob/tree 核对一致。

冻结输出保留源归档、可执行文件、manifest 和构建日志。成功后清理临时源码目录；
失败目录和日志保留。输出目录必须是新目录，不能覆盖旧结果。

## 只采集功能阶段，完整保留失败

`capture-plan-bench` 要求调用方提供冻结时记录的 manifest pin，校验后复制成自包含
capsule。执行期间不构建、不切换源码，固定参数仅为 --test；子进程清空继承环境，
设置固定 PATH、NO_COLOR=1、RAYON_NUM_THREADS=1。

复用已有进程组所有者，提供截止时间、取消和输出上限。保存 stdout、stderr、退出/
回收状态及原始成员是否改变，再固定独立的 receipt pin。空日志可以被保留和重放，
但缺少任何 case 都不能通过。源文件/JSON 的非空约束保持原强度。

阶段名称只允许 A1/B1/B2/A2。每个预期 case 必须且只能出现一次 Testing/Success，
重复、未知、截断、孤立 Success、失败退出、超时或未完成回收均拒绝。即使全部功能
记录通过，performance_accepted 仍固定为 false；此协议不接收统计计时模式。

## 离线从原始成员重算

`replay-plan-bench` 需要外部保留的 manifest pin、receipt pin 和预期 stage。
固定摘要应在可信构建/采集完成时另行保存，不能临时从待审归档自取摘要作为信任根。

重放只读取原始成员，不执行二进制、不解压或构建、不修改输入，也不读取派生的
replay.json 作为结论。它校验每份原始内容及最终未变性，重新计算完整目录结果。
Rust 采集与重放共用校验模型；本轮另执行独立的私有只读检查器交叉核对归档和日志。

本次同一个来源、同一个冻结二进制依次执行四个功能阶段：

| 阶段 | 功能 case | Rust 离线重放 | 独立读取器 | 性能资格 |
| --- | ---: | --- | --- | --- |
| A1 | 348/348 | 通过 | 通过 | 不授予 |
| B1 | 348/348 | 通过 | 通过 | 不授予 |
| B2 | 348/348 | 通过 | 通过 | 不授予 |
| A2 | 348/348 | 通过 | 通过 | 不授予 |

共 1,392 个功能结果。全部绑定 `c8d1963`，不把后续测试修复或文档提交重标为原捕获源。
这是同源功能自对照，不是 A/B 速度比较、受控资源证明或通过的 ABBA 计时 campaign。

另在副本和外部参数上执行四项实际拒绝控制：错误 stage、错误 source pin、错误
receipt pin，以及“仅一个成功 case、失败退出，但派生结果被改成成功”的自洽失败
记录。Rust 和独立读取器都保留该部分记录并拒绝成功，原四阶段数据未改变。

## 复现与验证边界

构建/采集入口当前支持 Unix。先构建工具以创建 target 目录，使用新输出目录：

```bash
cargo +stable build --locked -p xtask
target/debug/xtask freeze-plan-bench --root . --revision HEAD \
  --output target/plan-frozen --target-dir target
```

把上述输出的固定摘要另行保存在 PLAN_MANIFEST_PIN，再执行单阶段：

```bash
target/debug/xtask capture-plan-bench --frozen target/plan-frozen \
  --manifest-sha256 "$PLAN_MANIFEST_PIN" --stage A1 --output target/plan-A1
```

另行保存采集输出的 receipt 摘要到 PLAN_RECEIPT_PIN；后续只读重放：

```bash
target/debug/xtask replay-plan-bench --input target/plan-A1 --stage A1 \
  --manifest-sha256 "$PLAN_MANIFEST_PIN" --receipt-sha256 "$PLAN_RECEIPT_PIN"
```

新工具有 12 项契约，覆盖来源/产物归属、目录完整性、独立固定摘要、JSON 重复键、
符号链接、空日志、迟发修改、实际成功/失败子进程、不覆盖旧目录、超时/取消和回收。
初始 PR 的 Windows stable/beta/nightly 均在同一个 GF16 观察器测试失败：该测试绕过
正式 source::read，直接把 CRLF 文件交给精确匹配器。修复接回共享读取器，并在真实
文件上覆盖 LF、CRLF、CR，保留现有严格/歧义拒绝断言，未改编解码或观察器算法。

stable Rust 1.99.1 的最终本地验证为 980 项通过、43 项既有忽略；严格 Clippy、
格式、diff、no_std+SIMD 库 275 项及 workspace 契约 14 项通过。JSON 与 package
allowlist 审计通过，核心 71、WASM 6 成员不变。Windows 远程结论需看新 head 的
实际 CI 重跑，不能用本机换行回归冒充 Windows 实机通过。

## 后续执行顺序

下一步为独立 capsules 补齐跨阶段 campaign 顺序、冻结 A/B 配对和连续采集窗口的
可信绑定。统计原始样本/estimate 模型需独立设计，并保留既有分布、重复漂移和所有
失败记录，不能把此功能协议扩展解释成计时验收。

受控资源、完整发布基线和正式 ABBA/逐布局门槛仍按最新 main 报告执行。当前全部
codec 源码及根 Cargo.toml、Cargo.lock、build.rs 字节均与创建 PR 时相同，未更改
并行阈值或缓存策略。
本轮在同一 PR 中提交并推送后续实现和证据，未合并、打 tag 或发布 crate。

跨阶段固定配对、连续功能窗口、独立重放及后续执行顺序见
[连续计划 campaign 跟进报告](plan-campaign-follow-up-2026-10-26.md)。
