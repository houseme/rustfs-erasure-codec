# 跨有限域与容器的 Classic 恢复计划复用

接续[短尾更新与资源预热跟进](update-tail-follow-up-2026-10-26.md)，本轮继续执行
全局复审第 2 项：把现有的恢复计划所有权从 GF8 option-vector 入口推广到泛型调用者。
实现提交为 `62c49d2dd87203d720089c8bee5e9b0b96ee6334`，此前基线为 `639ae6d`，
已刷新远程 main，仍为 `07f02df`。

## 一个计划所有者，两类既有执行器

原 `OptionVecReconstructWorkspace` 仅在 std 下支持 GF8 的 Option<Vec<u8>>。
tower GF16、ShardSlot 及其他 ReconstructShard 容器每次调用普通 reconstruct 时，
仍重新建立缺失布局和计划。启用逆矩阵缓存可以复用矩阵，仍需要查询缓存；关闭缓存
则每次重新计算和拥有逆矩阵。

新增公开的 `ReconstructWorkspace<F>` 及两个泛型方法：

- prepare_reconstruct_workspace：验证输入形状并准备完整 Classic 恢复计划。
- reconstruct_with_workspace：检查 codec 格式与全部输入布局，委托既有泛型串行执行器。

计划拥有规范编码格式、缺失布局及独立的逆矩阵 Arc，不保存调用者分片引用或 payload。
原 codec 被释放、缓存关闭或原缓存条目不再保留时，计划仍可由等效 codec 使用。
每次执行必须保持分片数量、有效矩阵、缺失位置和长度，布局变化后重新准备。
可与 ShardSlot 配合保留目标缓冲区；首次计划准备及其长期存储由调用者管理。

GF8 原名称现在是该泛型所有者的别名，旧 prepare 方法委托统一准备入口，格式/布局
校验收敛到同一个函数。旧 option-vector 方法继续使用原批量执行器；泛型方法使用原
支持 fallible destination 的逐目标执行器。没有复制恢复算术或加入全局缓存。

泛型入口同时支持 std 与 no_std，不增加 Send/Sync 或 F: Clone/Debug 约束。
此次覆盖完整 Classic 恢复；Leopard 返回 UnsupportedCodecFamily，data-only、
selective 和既有并行入口继续使用原 API。旧 GF8 别名与入口仍保持原 std 特性边界。
中英文 README 与公共 rustdoc 已包含用法及布局复用条件。

## 资源与缓存查询见证

[完整记录与源码绑定](../benchmarks/global-review/2026-10-26/generic-reconstruct-workspace-resources.json)
比较同一个候选构建中的 ordinary reconstruct 和新 workspace 入口。脚本确认，去掉新增
workspace 方法及对应 import 后，reconstruct.rs 与此前基线逐字节相同。
这是一组 API 使用方式对照，没有把整个历史分支差异当作本次改动的前后速度比较。

固定 Classic/Vandermonde、3 parity，缺失位置为 [0, 1, data_count]；覆盖 GF8 和
高、低分量均有数据的 tower GF16，4/33 data，65/32,768 元素分片，缓存开/关。
每组输入、codec、参考编码、目标 backing storage 和 workspace 都在计数前准备，
两个入口各预热一次，再分别计数三次。每次恢复均逐字节核对全部分片。

下表在两个长度和三次重复中一致，数字为每次申请次数 / requested bytes：

| 字段 / 布局 | 逆矩阵缓存 | ordinary | workspace |
| --- | --- | ---: | ---: |
| GF8 / 4+3 | 关闭 | 1 / 1,072 B | 0 / 0 B |
| tower GF16 / 4+3 | 关闭 | 1 / 2,096 B | 0 / 0 B |
| GF8、tower GF16 / 4+3 | 开启 | 0 / 0 B | 0 / 0 B |
| GF8 / 33+3 | 关闭 | 4 / 3,697 B | 1 / 1,024 B |
| tower GF16 / 33+3 | 关闭 | 4 / 5,810 B | 1 / 1,024 B |
| GF8、tower GF16 / 33+3 | 开启 | 2 / 1,536 B | 1 / 1,024 B |

宽布局的执行输入视图仍会申请内存，没有将整个 API 标记为所有布局零分配。
计划保留了逆矩阵，不能把每次申请减少解读为常驻内存或 RSS 下降。

std+SIMD 和 std+SIMD+benchmark-metrics 分别采集完整的 48 条配对记录，各 96 个
API 计数窗口，共 192 个窗口。两种 feature 构建的申请量完全一致。
启用 benchmark-metrics 时，缓存开启的 ordinary 每次记录一次逆矩阵缓存请求，
workspace 为零；关闭缓存时两者请求均为零。指标关闭时计数器本来就返回零，
不使用那些零值作为未查询缓存的证据。

计数覆盖串行调用线程的 alloc/alloc_zeroed/realloc 请求及完整申请目标大小；
没有计时或采集 RSS。相应复现命令：

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test reconstruct_workspace -- --nocapture
cargo +stable test --release --locked -p rustfs-erasure-codec \
  --features 'simd-accel benchmark-metrics' --test reconstruct_workspace \
  caller_owned_plans_skip -- --nocapture
```

## 契约与工具验证

新增回归覆盖两字段、四种矩阵、fast parity、缓存开关、多种缺失形状、反复恢复、
不改变 backing address、等效 codec、原 codec 释放、计划并发共享和非 Send 元素。
计数/矩阵/缺失布局/尺寸错误在目标初始化前返回；所有 present 长度都会检查。
全 present 的计划也不能绕过后续的尺寸校验。奇异逆矩阵在准备阶段失败。

对于隐藏在 missing 状态下的短目标缓冲区，执行仍保持原来的逐目标发布行为：
第一个目标恢复正确后标记 present，后续目标拒绝初始化时返回错误，之后的 parity
目标保持 missing。测试与 ordinary 的结果和状态逐项对照，没有承诺整批输出原子发布。

stable Rust 1.99.1 / macOS ARM64：

- 全工作区全特性 949 项通过、43 项原有忽略，按顶层进程最终汇总排除子测试重复。
- 独立 no_std+SIMD 库 273 项及新 workspace 功能契约 6 项通过。
- std 新契约 9 项和原 GF8 workspace 契约 14 项通过；严格 Clippy、格式、diff 检查通过。
- 发布版 9.0.0 与当前 9.1.0 的 200 个非 fast 配置精确 parity、双向恢复及历史向量通过。
- 实际生成 verify-scratch 3 个、stream-core 4 个、dynamic 9 个成员并分别编译运行，
  每个注入副本通过 22 项功能契约。精确摘要按实际生成字节重新绑定，校验保持严格。
  这些是探针编译/语义检查；资源计数来自未注入版本，未扩展正式生命周期采样目录。
- x86_64 Linux 全特性库和 WASM no_std+SIMD 库交叉编译检查通过。
- JSON 和分发包 allowlist 审计通过；核心包 71 个成员、WASM 包 6 个成员，测试、
  工具和本轮证据不进入分发包。

本轮补齐完整 Classic 恢复的泛型计划复用。第 3 项 ordinary verify 分块仍需真实
业务瓶颈及稳定耗时证据，第 5 项缓存策略仍需生产线程/大小分布与峰值预算。
历史 ABBA 漂移失败记录保持，没有重新计时、跨平台实机吞吐或发布性能验收结论。
本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。

## 后续进展

数据恢复与选择性恢复的同一计划所有者、资源记录和契约见
[指定输出恢复 workspace 跟进](subset-reconstruct-workspace-follow-up-2026-10-26.md)。
上文保留完整恢复版本提交时的范围、源码身份和观测。
