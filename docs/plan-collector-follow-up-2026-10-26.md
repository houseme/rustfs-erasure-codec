# 受监督计划统计采集

按[执行绑定报告的后续顺序](plan-execution-follow-up-2026-10-26.md)，提交
`25cc67c` 增加受监督 collector，提交 `4531d00` 增加阶段统计门槛的提前终止。
[公开验证记录](../benchmarks/global-review/2026-10-26/plan-collector-validation.json)
区分完整合成采集、后续拒绝控制和各自来源，没有重标已有产物。

## 计划在执行前固定

collect-plan-statistics 要求输入目录中的 plan.json 及外部保留的摘要。计划使用
plan-bench-collection-v1，固定 origin、session、A/B 冻结 manifest、14 份原始
baseline 成员、renderer/analyzer、重复次数、统计策略和执行预算。冻结输入位于
corpus/sources/A、B，原始 baseline 位于 baseline/。

控制器先把固定成员复制到一个新输出目录，使用与核对摘要相同的字节，确认输入
未变，再进入执行窗口。只复制计划、冻结输入和原 baseline，不导入输入目录中的
历史样本、execution 或派生成功摘要。已有输出目录一律拒绝覆盖，不支持续跑。
控制器还核对自身编译时 source/asset fingerprint，源码变化时必须重新构建。

原 baseline 的终止状态、完整校准、分布、重复和布局规则直接复用现有模型。
正式 recorded 模式另外要求当前 Linux reservation 与原 context 匹配，阶段前后
继续观察；条件不满足时保存拒绝 receipt，不启动 worker，也不接管或修改资源。

## 固定启动参数和进程所有权

每轮严格执行 A1→B1→B2→A2。每次在新 worker 目录中保存冻结二进制副本，
固定 argv、工作目录、空 stdin、采样环境、allocator 环境和独立 CRITERION_HOME。
每个 worker 的 raw 目录初始为空，执行期间不构建、不换源，也不复用前次输出。

- synthetic 固定使用 --test，真实 Criterion 在该模式不会生成计时样本；协议测试
  使用明确构造的假 worker 产出合成数据。
- recorded 才使用 --bench。两者都固定 --noplot、sample-size、warm-up-time、
  measurement-time 和 confidence-level，不能临时追加任意 worker 参数。

复用现有进程组所有者，记录 start、stdout/stderr 和退出/回收结果。取消、超时、
输出超限和观察失败均停止后续 worker。未完成回收时立即停止解析 worker 产物，
保留目录；控制器不为清理输入而继续触碰仍可能被写入的原始文件。

进程回收后才解析产物。计划、冻结输入、baseline 或已执行二进制副本发生变化，
都会拒绝当前会话。原始失败、部分文件和终止状态保留，不能由成功 summary 覆盖。
硬终止可能留下未完成目录，没有完整可信 receipt 时不能接受。

## 原始产物与规范 corpus 一一绑定

控制器按 new/benchmark.json 的 full_id 核对完整冻结目录，拒绝未知、重复或
缺失 case。原 benchmark/sample/estimates 字节原样复制到规范 corpus，不能重新
生成估计值或选择成功子集。样本数必须与计划完全一致，数值检查复用上一轮模型。

每份预期文件都保存 worker 相对路径、规范路径和共同摘要。完整 receipt 还绑定
计划、编译控制器身份、固定 argv、start、outcome、日志、规范 samples 和 execution。
只读 replay-plan-collection 重算这些关系，并核对实际进程结果与统计 attempt 一致。
修改 worker 原件、规范副本、固定参数或已完成阶段绑定都会拒绝。

全部 raw 目录继续保留，包括 Criterion 的其他输出；其中只把完整预期 new/ 三件套
用于统计资格校验，base、其他派生文件或 collection.replay.json 不充当成功证据。
标准输出只打印 receipt 摘要和简要结果，完整诊断保存到文件，避免重复序列化大报告。

## 有界监测与阶段提前终止

worker 运行期间定期检查文件类型、目录深度、条目和字节预算，结束时强制再查。
单成员 32 MiB、workers 总原始字节监测 1 GiB；符号链接和特殊文件拒绝。定期
检查可能存在短暂超额，这不是内核文件系统硬配额，也不能代替真实资源保留。

只读采集重放给原件与规范副本合计 2 GiB 预算，同时为原执行/统计数据保留独立
256 MiB 子预算，没有因为附加 worker 原件而放宽旧数据门槛。子预算失败后外层
预算仍正确恢复，已经读取的字节继续计入总量。

CI 宽度在每阶段结束时立即检查。控制器只缓存已观察到的 typed mean，复用离线
模型的同一漂移计算，比较已可计算的 A2/A1、B2/B1 和跨重复配对均值；失败立即
停止后续 worker，不必为每个阶段重新读取不断增长的 corpus。未完成比较延后，
最终仍要求完整矩阵，非有限比值不能作为未知数据忽略。

回归反例先证明了旧 collector 会在宽区间失败后仍完成四个 worker。修复后，宽区间、
阶段内漂移、跨重复漂移分别在第 2、3、7 个 worker 后停止，完整保留已产生的原件。

## 本轮验证与边界

已提交来源的 CLI 启动 8 个原生假 worker，覆盖两轮完整 348-case 目录，产生
2,784 个合成观测、27,840 对样本和 8,352 份 raw-to-corpus 绑定。独立读取器
核对原 baseline 数值、完整统计、执行窗口、argv、退出、日志和逐文件摘要，结果一致。
这些假 worker 和冻结成员明确为协议输入，不是实际 codec 构建或计时测量。

实际控制包括错误外部 pin、错误 argv、worker 原件篡改、日志篡改、非零退出、
真实超时、真实 SIGINT 和本机 Linux 前置拒绝；后续 worker 均未被错误启动。
另以完整目录的原生假 worker 验证宽度、阶段漂移和跨重复漂移的提前停止，并独立
重算数值失败。即使所有已启动进程都正常退出并回收，统计失败也不能完成会话。

初始完整采集绑定 25cc67c，最终工具 4531d00 对其只读重放通过，原数据未变。
既有真实功能 campaign、统计 corpus、执行封装也通过兼容重放。新增 9 组 collector
契约与 1 组共享预算契约，全部计划协议 49 组；最终 stable Rust 1.99.1 完整
workspace all-features 为 1,018 项通过、43 项既有忽略，严格 Clippy、格式和 diff 通过。

accepted 只表示该采集记录及原始数据满足当前模型；capture_provenance_verified
和 performance_accepted 始终为 false。Linux recorded 路径尚未在合格 reservation
中用真实 Criterion 二进制完成正式验证。端点 context 不证明中间资源未变，也不
证明物理宿主独占；随机 bootstrap 和真实性能资格仍需要各自完整证据。

本轮没有运行真实 Criterion 计时、修改资源配置或更改编解码/并行/缓存策略。
660 份 JSON、workspace 依赖继承、解释器入口与 package allowlist 审计通过，
核心 71、WASM 6 个成员不变。上轮 head 856a36d 的 CI 为 31 项成功、3 项跳过；
新 head 以 PR 当前 CI 为准。

## 使用与下一步顺序

```bash
target/debug/xtask collect-plan-statistics --input target/plan-input \
  --plan-sha256 "$COLLECTION_PLAN_PIN" --output target/plan-capture

target/debug/xtask replay-plan-collection --input target/plan-capture \
  --plan-sha256 "$COLLECTION_PLAN_PIN" --receipt-sha256 "$COLLECTION_RECEIPT_PIN"
```

recorded 计划还需 --root 与 --isolation-cgroup，且必须先满足真实资源和原 baseline
条件；不能把此处 synthetic 控制转换为已完成的真实性或性能证明。

下一步先在真实合格 Linux reservation 中完成同源 collector 基线，核对实际 Criterion
产物布局、固定构建/环境、监督开销及重复稳定性。任何门槛失败都保留原件并停止归因。
之后才允许单变量候选比较，先通过完整 ABBA、重复和逐布局门槛，再按生产对象尺寸、
并发度与峰值预算推进 verify 分块和 oversized GF16 cache 策略。

原生编译身份、采集/重放统一预算与本地验证边界见
[collector 准入跟进报告](collector-admission-follow-up-2026-10-26.md)。
