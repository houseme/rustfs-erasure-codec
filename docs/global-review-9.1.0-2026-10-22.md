# 9.1.0 全局复核与增量更新重构

本文件保留上一轮复审过程。其源码绑定、资源和验证记录已归档至
[`previous-review/`](../benchmarks/incremental-update/2026-10-22/previous-review/provenance.json)。
当前实现与漂移诊断以[最新报告](drift-analysis-9.1.0-2026-10-22.md)为准，
下文中的计数及“最终实现”仅指该归档快照。

复核日期：2026-10-22（Asia/Shanghai）。已 fetch 并合并 `origin/main`，
本地与远程均为 `e671c0ac24f9deaf2ee2f1e5ebbd8358cd4d6133`，无需新增合并提交。
该 main 的[普通 CI](https://github.com/houseme/rustfs-erasure-codec/actions/runs/39525047445)
实际成功；本次改动尚未提交或运行新的远程 CI。

## 全局判断

现有编码格式、Classic 恢复计划、流式持有者与 Leopard 工作区边界足以支持
本轮修复。已经完成的恢复 scratch、流式生命周期和 WASM 功能不需要重新实现。
优先消除可以复现的资源浪费，保留编码格式、完整校验和恢复发布语义。

性能验收目前仍没有通过。[最新完整基线](https://github.com/rustfs/backlog/issues/2897#issuecomment-6316335129)
比较的是 9.0.0 自身，252 项分布检查通过，但 8/126 项重复漂移超过 5%。
[随后强制启动的独立 ABBA](https://github.com/rustfs/backlog/issues/2897#issuecomment-6316688014)
在 A1 的 `reconstruct:4:2:65536` 因分布跨度 20.6528% 超过 10% 停止，
B1/B2/A2 和逐布局补充均未开始。不能据此认定 9.1.0 回退，也不能通过重构
其他运行时路径证明这些漂移已经消失。本轮只读取这些终态，未独立回放私有原始记录。

## 确认问题与实现

Classic `update` 原先在扫描变更之前创建整片长度的 delta。即使所有新分片都是
`None`，它仍分配、初始化并释放这片存储；真实变更也需要与整个分片等长的 scratch。
两个新增资源测试在原实现上实际失败：65 元素空更新分别申请 GF8 的 65 字节和
GF16 的 130 字节。

复审发现上一版仍有一处可复现的资源浪费：33 个 parity 时，行视图容器越过内联容量，
额外申请 528 字节。65 字节更新因此申请两次、共 593 字节；新增边界测试在上一版
实际失败。这属于 Minor 资源缺陷，未发现新增的编码正确性回归。

同时收回两处局部补丁式扩展：只有一个调用者的 offset helper，以及为一种新增场景
引入的第二条 CLI/report 路径。它们是结构与维护性问题，不能仅凭 helper 或迭代器
存在就认定编译后的运行时变慢。

最终实现将变更检测合入已有的新分片长度校验循环。没有变更时直接成功；存在变更时，
沿用 `code_chunk_len` 策略，只申请一个初始化过的分块 delta，在所有变更之间复用。
短尾只使用有效前缀；直接从不可变编码矩阵取系数，将 delta 应用到对应的 parity 区间。
不再收集行视图，也不再维护 offset helper 或首元素回拼迭代器。
首分片重读后的非空检查保留：泛型 `AsRef` 可以返回变化的视图，不能假设它稳定。
删除该检查会使一类空更新从原来的 `EmptyShard` 变成成功，定向回归已先失败再通过。

这将 delta 存储从 O(分片长度) 限制为 O(现有串行分块长度)，不增加缓存、公共 API、
调参开关、并行策略或 unsafe。分块单位仍是 `F::Elem`；GF16 的字节数相应翻倍。
`Some(old)` 仍按请求执行增量更新，不额外扫描整片来猜测它是否相同。
fast-one-parity 保持原来的直接 XOR 和零 scratch 路径。

## 可复算资源结果

[最终数值与源码绑定](../benchmarks/incremental-update/2026-10-22/previous-review/provenance.json)
包含原有 15 个场景与新增 30 个 update 场景，共 270 条计数记录。
独立工具实际链接发布版 9.0.0 和本次路径候选；两个版本均启用 `std simd-accel`。
新增目录覆盖 GF8/GF16、65/65536/1048576/4194305 元素、0/1/2 个变更，
并增加各字段、各变更数的 33-parity 边界场景。
每个场景先预热两次，保留三次计数；每次从原 parity 开始，结果必须等于完整重编码，
两个版本的全部输出也必须相同。原有 200 项正常格式 parity/roundtrip 对照同时通过。

| GF8 分片长度 | 9.0.0 空更新申请 | 本次空更新申请 | 9.0.0 有变更 delta | 本次有变更 delta |
| --- | ---: | ---: | ---: | ---: |
| 65 B | 1 次 / 65 B | 0 | 65 B | 65 B |
| 64 KiB | 1 次 / 64 KiB | 0 | 64 KiB | 16 KiB |
| 1 MiB | 1 次 / 1 MiB | 0 | 1 MiB | 64 KiB |
| 4 MiB + 1 B | 1 次 / 4 MiB + 1 B | 0 | 4 MiB + 1 B | 256 KiB |

有变更时，上表每次仍只有一个 scratch 申请，跨分块和多个变更复用。
GF16 对应申请字节数是两倍，申请次数一致。
33-parity/65 元素有变更场景中，GF8 从两次/593 字节降为一次/65 字节，
GF16 从两次/658 字节降为一次/130 字节；空更新仍是零申请。
这是申请量与算法工作存储的改进，未测量峰值 RSS，也不代表耗时或吞吐提升百分比。
报告显式保持 `latency_claim=null`、`release_approval=false`。

普通 CI 沿用一个 `--allocation-report` 命令及原有计数器，统一输出 45 个场景；
原有场景 ID 保持，正式采样器未改。复现命令见[对照工具说明](../scripts/release-compatibility/README.md)。
[上一版报告](../benchmarks/incremental-update/2026-10-22/previous-review/first-pass/provenance.json)
连同原数值、验证记录与可重建源码补丁保留在 `first-pass/`，不重新标记为最终源码结果。

## 局部计时的结论范围

复审使用未改动的现有 `throughput_matrix_update` 六场景，按上一版 A1、中间重构版本
B1/B2、上一版 A2 执行一次。每个场景预热 0.5 秒、测量目标 1 秒、20 个 Criterion
观测；保留全部 480 个观测及估计值。两边的编译器、锁文件、SIMD 特性与四线程配置相同。
它计入原有 parity 克隆、视图构造与 update 调用，不把逻辑数据吞吐换算成纯更新吞吐。

上一版 1 MiB 的四个场景 A1/A2 描述性中位数漂移为 8.485%～16.978%，
64 KiB 两场景为 0.207%/0.502%；中间版本 B1/B2 为 0.123%～0.896%。
全部 480 个原始观测与源码绑定私有保留，不选择有利子集、不重跑。
该环境未隔离，Criterion 自行选择各次迭代数，且不是正式采样协议；最终实现还恢复了
上述空视图检查。因此这些数据既不能归因版本收益，也不是最终源码的计时验收，
不用于证明吞吐提升或发布放行。

## 正确性与发布边界

新增回归覆盖 GF8/GF16、四种矩阵、fast 模式、多分块/短尾、完整重编码对照、反向更新
及 32/33 parity 元数据边界。
晚出现的新分片长度错误必须先于任何 parity 写入；空更新仍必须拒绝非法计数、
旧分片/parity 长度和空分片，包括验证后变化的首分片视图；Leopard 仍返回不支持。

本轮 stable Rust 1.99.1 / macOS ARM64 的最终验证：

- 默认工作区 506 项通过、43 项忽略；全特性工作区 528 项通过、43 项忽略，
  均包含文档测试，且只计每个测试进程的最终汇总，排除嵌套子进程的重复结果。
- no_std+SIMD 库 237 项、新增 no_std 回归 7 项通过。
- 主工作区与独立对照工具的 Clippy（warnings denied）、372 项 Python 门禁测试、
  Rust 格式、actionlint、typos、diff 和 manifest 9.1.0 检查通过。
- 实际生成的 WASM 包完成 TypeScript、14 项 Node 和 8 项 Chrome 集成测试。
- 一次性无 Git 元数据的源码副本实际通过 `cargo publish --dry-run --locked
  --registry crates-io --package rustfs-erasure-codec`，构建了解包后的分发源码，
  并明确中止上传；没有使用 `--allow-dirty` 或 `--no-verify`。

本轮[验证记录](../benchmarks/incremental-update/2026-10-22/previous-review/validation.json)
仅描述本地执行，不将此前 main 的 CI 结果移作本次改动结果。

9.1.0 的版本已同步，无需重复升版本；changelog 将候选明确标为 unreleased。
仍需按[发布清单](release-9.1.0-checklist.md)冻结最终完整源码/锁文件/文档/采样器，
在有明确区分条件的环境完成稳定全目录基线、最终 ABBA 与逐布局补充，再验证不可变证据
及实际严格正向预检。保留原 5% 重复、10% 分布和时长门槛。
不自动重跑相同失败条件，不将 PMU、锁频或迁移变成新增发布要求。

本次 update 资源修复没有解释或关闭 #2897。它改变候选运行时源码，旧实验不能重新
标记为当前候选证据；#2897 仍仅条件 4 在既定正常格式范围完成，其余验收状态保持。
本轮未创建 tag、上传 registry 或发表吞吐结论。
