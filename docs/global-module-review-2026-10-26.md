# 远程 main 全局模块复审与收敛重构

基线：`origin/main` 的 `b8d7a910f4a741530b370822e5a121dffdd4cce3`。
代码在独立分支 `houseme/codec-global-review` 中修改。Rust edition 2024，
manifest 未声明 MSRV；本轮使用已安装的 stable Rust 1.99.1。

## 结论

当前库有局部重复状态和未完成的抽象迁移，但不足以认定整个实现需要推倒重写。
远程 main 已形成“编码格式 → 恢复规划 → 执行调度 → 有限域内核”的分层。
构造时生成的不可变矩阵、按存活输入复用的逆矩阵、分块执行和受限工作缓存应保留。
本轮把流式恢复中残留的长度表、系数引用表收回到现有数据所有者，删除重复状态，
并修正一个错误提示。没有加入新的调参开关、公共 API、动态分发或 unsafe。

本轮覆盖下表中的模块边界、关键调用链、存储所有权和现有回归契约；重点完整阅读了
构造、编码、恢复计划、缓存、流式 I/O 与受影响工具函数。SIMD 叶子内核按入口检查、
目标指令集与尾部边界审查；并非对全部架构每条指令完成独立数学证明或实机执行。
工具侧检查了共享模型、源码生成和资源所有权边界，并执行完整工具测试；没有把
所有历史诊断记录重新采集或回放。下列“保留”结论表示本轮未建立需要修改的具体缺陷，
不等于证明整个模块没有任何缺陷。

## 模块判断

| 模块 | 核对的职责与约束 | 处理结论 |
| --- | --- | --- |
| `lib.rs`、`errors.rs`、`macros.rs` | Field/ReconstructShard 扩展点、ShardSlot 状态、公共错误和尺寸校验 | 保留泛型边界；修正 TooFewShardsPresent 的错误阈值文字 |
| `core/codec.rs`、`options.rs`、`format.rs` | 分片数量溢出、家族选择、自定义矩阵、稳定格式身份、split/join | 编码格式与执行选项已分离；不以更快算法替换已持久化的数据格式 |
| `matrix.rs` | 系统矩阵、消元、逆矩阵、借用行范围 | 现有直接增广存储、稀疏行处理和标量/SIMD 路径有独立约束，保留 |
| `core/encode.rs`、`shard_by_shard.rs` | 完整/单片编码、增量更新、fast parity、native fallback、状态递增 | 原生路径在写入前检查；update 的分块 delta 已按需生成；保留共用执行器 |
| `core/recovery_plan.rs`、`reconstruct.rs` | 全量/仅数据/选择性恢复，奇异矩阵、目标发布次序、存活输入缓存 | 规划与执行已经集中；流式调用补齐对现有行视图的复用 |
| `core/verify.rs`、`workspace.rs` | 校验重编码、调用者 scratch、重复调用复用、Leopard 分流 | 保留；普通 verify 的整片 parity scratch 是后续有条件测量对象 |
| `core/stream.rs` | 独立 Reader/Writer、Cursor 适配、短读、Interrupted、EOF、截断、部分输出 | 删除重复长度表与系数表；保留既有错误与缓冲区生命周期 |
| `core/parallel.rs`、`cache_detect.rs`、`galois_8/policy.rs` | 字节/元素单位、可用线程和作业数、缓存估算、opt/workspace 入口 | 保留已测量阈值；相似的字节策略和元素切片操作不盲目合并 |
| `core/metrics.rs`、`galois_8/profile.rs` | 普通构建与 benchmark-metrics 的边界 | 性能计数已有 feature 边界；本轮基准关闭 benchmark-metrics |
| `galois_8` scalar/backend/codegen 与各 ISA 子目录 | 不可变后端选择、显式 override、生成内核、长度检查和尾部 fallback | 保留架构专用内核；aarch64 实际执行，x86/VSX 仅静态检查 |
| `galois_16.rs` | tower-field 算术、表示顺序、GF8 复用、GFNI 分流 | 与 Leopard GF16 不是同一表示；保留兼容向量与独立算法 |
| `core/leopard*` 的 driver/tables/ops/encode/decode/work/SIMD | 64 字节约束、FFT 规划、选择性输出、对齐存储、TLS 缓存和并行块 | GF8/GF16 保留独立变换；不为减少文件或行数引入统一运行时分支 |
| `wasm`、`fuzz` | 计数/布局/flag 检查、错误码映射、消费端和契约测试入口 | Rust workspace 编译/测试覆盖；本轮没有新增浏览器执行或长时间 fuzz 结果 |
| `xtask`、`scripts`、基准、CI、兼容性工具 | 共享校验模型、源/生成物绑定、捕获与回放边界、分发排除 | 更新受影响的诊断锚点与精确源码摘要；严格校验语义保持 |

## 已确认并完成的修改

### Minor：流式恢复重复保存每个已读缓冲区的长度

`read_present_streams_with_mode` 在每次读取后已经 `truncate(total)`，却另建
`Vec<(index, length)>`。并行模式每块还要 resize、写入、retain 这个表。
索引和长度都可以直接从已有 Reader/Vec 对取得。

现在读取结束后仅检查 `Some(reader)` 对应的 `buffer.len()`。这一过滤条件不可删除：
缺失分片的输出缓冲区可能仍保存上一块，不能参与本块最大长度、截断检查或 EOF 判断。
错误继续报告第一个长度不一致的存活分片，当前错误块仍不发布恢复输出。

新增空流资源回归在原实现上实际失败：4+2、全部 reader 存在且为空时分配 8 次。
重构后为 7 次，即六个输入缓冲区和一个所有者表，去掉第二个长度表。

### Minor：流式恢复仍持有旧的系数行引用表

Classic 已提供 `ClassicReconstructRows`，而流式恢复继续 collect 两张
`SmallVec<[&[u8]; 32]>`。超过 32 个目标时会为引用表分配内存。

现在从同一个恢复计划借用行视图，在数据目标数处 split，复用到所有块。
恢复矩阵仍由计划持有，LRU eviction 或禁用缓存都不会使行引用失效。
没有重新计算矩阵、重排目标或改动编码系数。

2+33、33 个校验目标、37 字节分片的资源回归在原实现上实际失败：41 次分配；
本次为 39 次，分别去掉长度表和系数行表。33+34 的全数据、全校验、混合缺失测试
还覆盖了两阶段边界、两种 I/O 模式、多个完整块和 37 字节短尾。

### Minor：恢复失败的错误提示把 data threshold 写成 parity threshold

例如 4+2 的配置只剩 3 个分片时，3 已大于 parity 数 2，却仍无法恢复。
原 `TooFewShardsPresent` 文案称存活数小于 parity 数，容易误导排障。
改为说明存活数小于恢复所需的 data shard 数；错误变体、WASM 数字映射不变。
原有错误字符串测试同步更新。

## 性能与资源证据

[原始阶段结果和源码绑定](../benchmarks/global-review/2026-10-26/stream-resources.json)
保留全部 A1/B1/B2/A2 结果及失败门槛。使用未修改的 `recovery_paths` 基准，
`std + simd-accel`、NEON、四线程环境，流式 I/O 和计算均沿用原串行路径。
两份可执行文件均先构建，再按 A1/B1/B2/A2 顺序运行；每场景预热 20 次，
每阶段三组样本、每组目标 500 ms。构建与测试不和计时同时进行。
基准逐字节检查输出；分配计数单独进行。

| 场景，分片均为 1 MiB，缺失 1 data + 1 parity | 分配次数，前 → 后 | 申请字节，前 → 后 | A1/A2 绝对漂移 |
| --- | ---: | ---: | ---: |
| 10+4，4 KiB block | 16 → 15 | 57,904 → 57,680 | 5.64% |
| 10+4，64 KiB block | 16 → 15 | 918,064 → 917,840 | 3.92% |
| 32+16，4 KiB block | 52 → 51 | 199,680 → 198,912 | 9.14% |

资源计数在 A1/A2、B1/B2 中分别一致。少一次长度表分配节省
`total_shards * size_of::<(usize, usize)>()`，本机分别为 224 和 768 字节。
这表示申请量下降，不表示峰值 RSS 或吞吐提升。

两个场景的基线漂移超过 5%，候选阶段也有明显漂移，因此停止速度归因；不选取
其中一个稳定场景宣布整体改善，不在相同条件下反复重跑获取有利结果。
本轮没有可靠证据声称吞吐更快、没有“全平台最优”结论，也不构成发布性能验收。

复现资源计数的入口：

```bash
cargo +stable test -p rustfs-erasure-codec --locked --test stream_reuse_regressions
RAYON_NUM_THREADS=4 RSE_RECOVERY_CASE_FILTER=stream.reconstruct RSE_RECOVERY_SAMPLE_MS=500 \
  cargo +stable bench -p rustfs-erasure-codec --locked --features simd-accel --bench recovery_paths
```

速度比较需要两个冻结源码构建、完整原始结果和稳定环境；单独运行这条 bench 不足以归因。

## 下一步优化的执行顺序

1. **先稳定测量环境，再选择算术或调度改动。** 保留既有完整目录、漂移门槛和布局对照。
   当前三个流式局部样本不能用来调低并行阈值或决定更换内核。
2. **优先优化调用者的资源生命周期。** 重用 codec、VerifyWorkspace、恢复 workspace 或
   ShardSlot，分别测量构造、计划、分配和纯编码成本。库内再加全局缓存可能引入锁和
   格式隔离问题，不能替代调用者已有的复用 API。
3. **普通 verify 的有界分块 scratch。** 只有实际工作负载显示 `parity * shard_len`
   的 scratch 是瓶颈时才实现。需保持所有输入先验证、所有矩阵/fast 模式、并行与原生
   内核选择，并测量大分片、小分片、早期/尾部损坏，不能只报告申请字节减少。
4. **宽布局流式视图。** 本次去掉系数表，输入/可变输出引用超过内联容量时仍可能每块
   分配。若它在实际宽布局中显著，再设计受 Rust 借用约束保护的分块适配器；不能用
   raw pointer 或第二套恢复执行器掩盖所有权问题。
5. **按峰值内存预算评估 Leopard 缓存策略。** GF16 decode 当前会在 oversized 调用时
   释放旧小缓存，相关测试明确要求这一行为。保留小缓存可以减少后续分配，也会增加
   oversized 调用期间的并存内存。应先确定线程数、峰值预算和大小交替频率，再决定；
   不能仅为了与 encoder 看起来一致而修改这个取舍。
6. **文档入口整理独立于数据兼容性。** 索引可指向最新模块判断，但历史源绑定结果不能
   改标签或删除来制造“已完成”的表象；本轮只增加当前报告入口。

## 验证与交付边界

- `cargo +stable test --workspace --all-features --locked`：912 项通过，43 项原有忽略。
  统计按每个顶层测试进程的最终汇总，排除了自启动子测试的重复计数。
- 独立 codec `--no-default-features --features simd-accel --lib`：273 项通过。
- `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings`：通过。
- 发布版 9.0.0 与本次路径 9.1.0：200 种非 fast 配置精确 parity、双向交叉恢复和历史
  tower-field 向量通过。fast 模式另由本地契约测试覆盖，不把旧版不一致行为算作兼容保证。
- 临时源码副本通过实际 native source-plan 注入，4 个生成成员实际编译；流式契约测试
  18 项通过。这是探针编译/语义检查，不是全量生命周期采集或正式发布诊断。
- `cargo fmt --all --check`、`git diff --check`、JSON/workspace/package allowlist 审计通过；
  核心包 71 个成员、WASM 包 6 个成员，仓库工具和本轮证据均不进入分发包。
- 结束前再次 fetch，远程 `main` 仍为上述基线。
- 全工作区初次执行仅因 stream-core 生成源码精确摘要过期失败；确认注入点与内容后更新
  摘要并重跑通过，没有放宽摘要检查。

本轮没有运行 x86/Windows/PowerPC 实机基准、浏览器端测试、Miri 或长时间 fuzz。
没有推送、创建 PR、打 tag 或发布 crate。改动保留在隔离 worktree，供代码审阅和后续提交。

## 后续进展

调用者资源生命周期的已提交实现、资源结果和其余步骤状态见
[2026-10-26 跟进报告](caller-workspace-follow-up-2026-10-26.md)。
上文保留最初复审时的源码身份、测量结果及交付状态。

Classic 完整、数据与选择性计划复用之后的宽执行视图进展见
[Classic 恢复视图跟进报告](recovery-view-follow-up-2026-10-26.md)，
包含精确容量、bulk 输出合并、完整申请对照及剩余执行顺序。

Classic 计划构造中的完整 required 目标索引复用、准备/普通调用成本及后续容量检查见
[Classic 完整目标索引跟进报告](target-index-follow-up-2026-10-26.md)。
