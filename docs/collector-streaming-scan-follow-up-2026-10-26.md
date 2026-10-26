# Collector 预算扫描与固定参数复用

在[准入与阶段预算改进](collector-admission-follow-up-2026-10-26.md)之后，本轮继续
做本地可验证的监督开销优化。提交 `340156b` 将监测与 metadata 发现归并为一次
有界遍历的两种用法，并复用预声明 argv。
[验证记录](../benchmarks/global-review/2026-10-26/collector-streaming-scan-validation.json)
保留源码身份、结构清单和兼容重放结果，没有新增正式计时结论。

## 监测不再建立完整文件名集合

原监测每次遍历 workers 时，为每个文件构建相对路径 String，再排序整个 Vec。
监测调用方只使用是否超限的结果，文件名集合及排序没有参与预算判断。随着已完成
worker 产物积累，这会重复分配和排序越来越多的名字。

新实现使用同一个有界 scan：所有目录、文件类型、深度、条目、字节和文件名 UTF-8
规则都经过检查，monitor 只返回计数，不保存普通文件名。目录遍历栈和临时 PathBuf
仍然存在，因此这不是零分配或常量总内存的声明。完整 workers 仍被检查，没有靠
跳过已完成目录或缓存旧字节数来放宽总预算。

产物解析复用该遍历，只保留 new/benchmark.json 路径，再按原规则排序。sidecar、
base 和其他被忽略的文件仍参与所有预算与类型检查；其内容不充当统计通过证据。
原路径规范化和完整 metadata 身份校验保持可用。

额外收紧根目录边界：root 本身也必须是普通目录，符号链接 root 在跟随前拒绝。
这与子条目的符号链接拒绝规则一致，防止已失败 worker 的 raw 目录替换后被解析。

## 固定 argv 按操作复用

原采集循环在 command、start journal、receipt 构造时分别调用 plan.args；重放
又为每个 worker 重建比较 Vec 和 JSON 值。现在采集循环构建一次，command 借用
它，receipt 保留协议要求的独立副本；每次重放也只构建一次 argv 和预期 JSON。

八 worker 完成场景的源码调用计数为：采集循环 24→1，重放 16→1。这是源码路径
分析，Command 自己的所有权缓冲、receipt 克隆和 JSON 序列化仍会分配，不是
allocator 实测。argv、日志、receipt、原件和规范 corpus 的协议内容均未改变。

## 本地结构与兼容验证

从已提交源码提取未修改的遍历前半段，用独立本地 wrapper 调用，Python 文件清单
另外核对目录、文件数、大小和 metadata 选择。完整合成场景结果为：

| 路径 | 原实现保留的文件名 | 新实现保留的文件名 |
| --- | ---: | ---: |
| workers 预算检查 | 8,392 | 0 个普通文件名集合 |
| 八次 raw 解析合计 | 8,352 | 2,784 个 metadata 路径 |

监测实际检查 13,978 个目录/文件条目。旧监测普通文件相对名的 UTF-8 payload 共
309,696 字节；解析时路径 payload 合计从 225,504 降到 77,952 字节。这些值不含
目录栈、临时路径、String/Vec 容量、allocator 元数据，也不是 RSS 或总堆内存。
没有测量吞吐、延迟或分配次数，不能把此清单换算成正式性能提升百分比。

新工具启动八个原生假 worker，完成两轮 348-case 合成目录：2,784 个观测、
8,352 份原件绑定，独立读取器一致。四类既有 collection、execution、statistics、
functional 产物只读重放通过，原件未变。capture_provenance_verified 和
performance_accepted 保持 false，所有新样本明确为 synthetic。

新增四组本地遍历契约覆盖完整计数与排序、ignored 成员的预算、sidecar 增长、
符号链接 root/条目拒绝；Linux 另有 ignored 无效 UTF-8 名称反例。APFS 无法创建
该类文件名，所以该反例仅在 Linux 启用，本机只通过交叉编译检查，不冒充执行。

stable Rust 1.99.1 完整 workspace all-features 为 1,030 项通过、43 项既有忽略；
本地 plan_bench 定向契约共 61 组。严格 Clippy、格式、diff，以及 Linux xtask
测试目标的离线交叉编译检查通过。64 份 codec 源码及根 manifest、lock、build.rs
不变，未改编解码、并行阈值或缓存策略。
662 份 JSON、workspace 依赖继承、解释器入口及 package allowlist 审计通过，
核心 71、WASM 6 个成员不变。上轮 head 8d87f1f 的 CI 为 31 项成功、3 项跳过；
新 head 以 PR 当前状态为准。

## 后续顺序与边界

本轮继续保持正式计时暂不运行，也不调整远程资源。下一步仍是在确认可用的
专用 Linux 资源后，使用 native 冻结输入完成同源 collector、监督开销与重复
稳定性基线。预算检查保留完整遍历，真实监督开销仍需要在该环境中验证。

完整未变 baseline、ABBA、A2 漂移与逐布局门槛通过后，再评估生产尺寸/并发/
峰值预算下的 verify 分块及 oversized GF16 cache 策略。所有历史失败与来源保留，
本轮未合并、打 tag 或发布。

## 交付来源映射

交付期间原 PR #115 已合并。最新 main 为 ec9211b，其完整源码树与已验证基线
8d87f1f 相同。本轮两个提交单独放到该 main 上，对应代码提交 a52eda3，
交付 [PR #116](https://github.com/houseme/rustfs-erasure-codec/pull/116)。
原采集源码 340156b 与 a52eda3 的完整树一致，验证数据和原源码身份保留，
没有因为移动交付分支而重标测量或重复运行正式计时。
