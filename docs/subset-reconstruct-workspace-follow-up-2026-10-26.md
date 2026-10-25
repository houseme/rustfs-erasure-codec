# 数据与选择性恢复的计划复用

接续[泛型完整恢复 workspace](generic-reconstruct-workspace-follow-up-2026-10-26.md)，
本轮继续全局复审第 2 项，将相同所有者扩展到数据恢复和指定输出。
实现提交为 `476c8445f949a78147568efc7d77d04d291c6db7`，此前基线为 `6458de1`；
远程 main 已刷新，仍为 `07f02df`。

## 统一准备目标，沿用已有执行器

新增 prepare_reconstruct_data_workspace 和 prepare_reconstruct_some_workspace，
分别准备仅数据、指定 missing shard 的计划。完整恢复构造器与它们共用一个私有
目标准备函数，继续调用既有 Classic planner。ReconstructWorkspace 增加
`data_targets()` / `parity_targets()`，返回排序后的实际 missing 输出。

reconstruct_with_workspace 执行准备时固定的输出集合。计划拥有目标索引、逆矩阵和
组合 parity 行，修改或释放原 required mask 不会改变计划。所有 erasure 仍属于
严格匹配的输入布局，未请求分片保持原数据与 missing 状态。布局变化后需重新准备。

仅恢复 parity 时，即使一些 data missing，也可复用既有 planner 组合出的 survivor
系数行，直接计算请求的 parity。未请求的 missing data 不需要初始化。这些组合行
现在随调用者的 workspace 保留，避免在每次重复调用中重新组合。

本轮没有修改 recovery_plan.rs、矩阵、有限域内核或编码循环。脚本另外确认，
reconstruct.rs 中已有泛型/批量执行器、ordinary reconstruct/data/some、逆矩阵缓存及
workspace 执行/校验函数均与此前基线逐字节相同。

GF8 旧别名和 bulk 入口也接受新目标准备器；旧 prepare_reconstruct_opt_workspace
仍准备完整恢复。泛型入口保留 no_std、非 Send 元素支持，没有新增 runtime 开关、
unsafe、缓存策略、并行阈值或持久化格式。

## 完整资源对照

[资源目录、操作数与源码绑定](../benchmarks/global-review/2026-10-26/subset-reconstruct-workspace-resources.json)
比较同一候选构建中的普通 data/some 调用和复用 workspace 的调用。
固定 Classic/Vandermonde、3 parity、65 元素分片，missing 为 [0, 1, data_count]。
GF8 元素宽度 1 B，tower GF16 宽度 2 B；分别覆盖 4/33 data、缓存开/关，以及：

- data：只恢复 data 0、1，missing parity 保持未请求。
- parity：只恢复 parity data_count，missing data 0、1 保持未请求。
- mixed：恢复 data 0 和 parity data_count，missing data 1 保持未请求。

每项准备输入、参考编码、目标缓冲区和计划，各预热一次后计数三次。
所有恢复结果与参考分片核对，未请求缓冲区和状态也独立检查。

每次申请次数 / requested bytes 在三次重复中一致：

| 字段 / 布局 / 目标 | 逆矩阵缓存 | ordinary | workspace |
| --- | --- | ---: | ---: |
| GF8 / 4+3 / 三种目标 | 关闭 | 1 / 1,072 B | 0 / 0 B |
| tower GF16 / 4+3 / 三种目标 | 关闭 | 1 / 2,096 B | 0 / 0 B |
| 两字段 / 4+3 / 三种目标 | 开启 | 0 / 0 B | 0 / 0 B |
| GF8 / 33+3 / data | 关闭 | 4 / 3,697 B | 1 / 1,024 B |
| tower GF16 / 33+3 / data | 关闭 | 4 / 5,810 B | 1 / 1,024 B |
| GF8 / 33+3 / parity、mixed | 关闭 | 5 / 3,761 B | 1 / 1,024 B |
| tower GF16 / 33+3 / parity、mixed | 关闭 | 5 / 5,938 B | 1 / 1,024 B |
| 两字段 / 33+3 / data | 开启 | 2 / 1,536 B | 1 / 1,024 B |
| GF8 / 33+3 / parity、mixed | 开启 | 3 / 1,600 B | 1 / 1,024 B |
| tower GF16 / 33+3 / parity、mixed | 开启 | 3 / 1,664 B | 1 / 1,024 B |

std+SIMD 与 std+SIMD+benchmark-metrics 各保留完整 72 条配对记录，各 144 个 API
窗口，共 288 个窗口。两种构建的申请结果完全一致。指标开启且缓存开启时，ordinary
每次一次逆矩阵缓存请求，workspace 为零；指标关闭时的零值不作为查询见证。

此外用未启用 native byte opt-in 的 GF8 标量观察字段，测量 4+3、missing [0,1,4]、
只请求 parity 4 的重复组合成本。长度 1/65/4097 各三次，workspace 仍执行原有恢复
算术：4*N 次字段乘法、3*N 次加法。ordinary 每次另有 8 次乘法和 8 次加法，恰为
2 个 missing data 项 * 4 个 survivor 列 * 1 个请求 parity 行。该部分移到准备阶段，
没有改变 native kernel，也不把操作数减少直接写成耗时提升。

计数是串行调用线程的 alloc/alloc_zeroed/realloc 请求及完整目标大小。
计划准备和长期保留的矩阵/组合行在热窗口外，宽输入视图仍有分配；未测量 RSS、
存活峰值或稳定速度。调用者需管理计划与 backing buffer 的生命周期。

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test reconstruct_workspace -- --nocapture
cargo +stable test --release --locked -p rustfs-erasure-codec \
  --features 'simd-accel benchmark-metrics' --test reconstruct_workspace \
  subset_workspace_resources -- --nocapture
```

## 验证与后续顺序

新增契约穷举小布局合法缺失组合和 required masks，覆盖两字段、四矩阵、fast parity、
缓存开关、完整/短 data mask；并检查目标查询、重复调用、mask 所有权、原 codec
释放、GF8 旧 bulk 入口和非 Send 元素。

合法全 false 或仅指向 present 的 mask 不会计算未用的奇异逆矩阵，仍验证完整形状和
恢复 quorum；非法 mask 长度在 shard count 前拒绝。短目标容器仍保留原逐目标发布
顺序，后续出错不会把未完成的 parity 标记为 present。Leopard workspace 仍不支持。

stable Rust 1.99.1 / macOS ARM64：

- 全工作区全特性 956 项通过、43 项原有忽略，排除自启动子测试重复汇总。
- 独立 no_std+SIMD 库 273 项及 workspace 契约 10 项通过。
- std workspace 契约 15 项、既有 GF8 契约 14 项通过；严格 Clippy、格式、diff 检查通过。
- 发布版 9.0.0 与当前 9.1.0 的 200 个非 fast 配置精确 parity、双向恢复及历史向量通过。
- 实际生成 verify-scratch 3 个、stream-core 4 个、dynamic 9 个成员；每个注入副本
  27 项功能契约通过。仅 dynamic 摘要因当前源变化重新绑定，其他两个保持原摘要。
  此为编译/语义验证；资源计数来自未注入版本，未扩展正式生命周期采样目录。
- x86_64 Linux 全特性库与 WASM no_std+SIMD 库交叉检查通过。
- JSON 和分发包 allowlist 审计通过：核心包 71 个成员、WASM 包 6 个成员，测试、
  工具和本轮证据不进入分发包。

第 2 项现在覆盖 Classic 完整、仅数据和选择性恢复的计划复用；宽视图的执行分配
仍保留。第 3 项 verify 分块仍需生产瓶颈与稳定耗时，第 5 项缓存策略仍需生产线程/
大小分布和峰值预算。历史 ABBA 漂移失败记录保持，本轮未重新计时或宣称发布性能验收。
本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。
