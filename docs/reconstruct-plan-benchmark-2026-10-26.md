# Classic 计划生命周期微基准

本轮接续[shape 索引跟进报告](shape-index-follow-up-2026-10-26.md)的耗时验证准备。
先通过 `d3f05585c5e2c40d57bdbc5d1430514e11a48cda` 合入远程 main `758ce75` 的
最新测量证据，保留当前优化历史；基准实现为
`220ba1fe9523e44704e7f775219cbfef2680d815`。

[新 main 的独立 A2 报告](independent-a2-followup-2026-10-26.md)仍保留分布、重复
漂移及原 campaign 中断；独立 A2 属于另一资源窗口，不能补成原 ABBA。
本轮推进受控测量所需的工作负载和生命周期验证，只执行 Criterion 功能模式，
没有重跑原共享条件的速度实验，也没有新增运行时优化或候选速度结论。

## 一个目录，三个明确的调用窗口

新增独立的 `reconstruct_plans` bench，沿用资源测试的 58 个合成布局，覆盖两字段，
每布局三个 API，共 348 个唯一 Criterion ID。data=4/32/33/64/65/96、32/33 与
64/65 边界、头中尾与稀疏缺失、无缺失、quorum 不足、迟发空/短长度、宽缺失索引
控制全部保留。目录是功能覆盖，不是生产频率或加权后的典型工作负载。

原资源测试的局部 Case、cases 和 expected_error 抽取到
`tests/common/reconstruct_shape_cases.rs`，新 bench 和旧资源测试实际共用它，
避免两个目录随演进分叉。原资源预算、断言和记录字段不变。

| 动作 | 迭代中的操作 | 结果生命周期 |
| --- | --- | --- |
| prepare_full | 完整 Classic workspace 准备 | 消费 Result，包含返回计划的销毁 |
| prepare_none | 全 false mask 的 workspace 准备 | 消费 Result，包含返回计划的销毁 |
| call_none | 全 false mask 的普通 reconstruct_some | 消费成功或原验证错误 |

Criterion iter 自身包含每次返回值的销毁，故这里测量计划构建与销毁的完整生命周期，
不命名为排除 drop 的纯构建耗时。无输出调用保留真实的形状、quorum 与错误检查；
不能把不同动作的绝对耗时相减，解释为纯补集遍历或纯 allocator 成本。

每字段/布局只构造一次 Fixture，位于 Criterion 反复调用的回调外。codec、编码
参考条带、调用者 ShardSlot 缓冲区、mask 和错误布局均长期保留。65 元素、Classic/
Cauchy、逆矩阵缓存开启；需要逆矩阵的合法完整计划在注册动作前预热。
输入引用和每次 Result 经过 black_box 消费，内层没有 fixture 构造、编码、payload
clone 或正确性扫描。

每个动作注册前后都做独立 preflight：核对错误类型与目标列表，执行准备好的完整
计划并逐字节比较编码参考，验证无输出/错误保持原 dirty buffer 与 missing 状态。
这些校验使用临时副本，位于测量外。微基准只描述上述持有者和缓存政策，不能代表
每次克隆输出的调用者、端到端完整恢复或生产吞吐。

## 已执行的验证

[完整 ID、源码与验证绑定](../benchmarks/global-review/2026-10-26/plan-benchmark-validation.json)
保留全部 348 个 ID、两种功能构建状态及日志摘要。

```bash
cargo +stable test --locked -p rustfs-erasure-codec \
  --features 'simd-accel benchmark-metrics' --test reconstruct_plan_bench
cargo +stable bench --locked -p rustfs-erasure-codec --features simd-accel \
  --bench reconstruct_plans -- --test
cargo +stable bench --locked -p rustfs-erasure-codec \
  --features 'simd-accel benchmark-metrics' --bench reconstruct_plans -- --test
```

两种 bench 构建各 348 项 Success，ID 与共享资源布局的笛卡尔积完全一致。
`--test` 没有执行 Criterion 统计采样 campaign，不生成可归因的速度结果。

新增三项契约覆盖目录唯一性、两字段全部 fixture 的完整恢复/无输出/错误结果，以及
同一 fixture 三轮、每轮七次重复操作。每轮核对返回错误；重复后 owner 和全部 backing
地址保持不变。指标开启时，需逆矩阵的合法布局恰好有 21 次查询、21 次命中、零
新增 miss；不需要逆矩阵的布局为零查询。指标关闭的零计数不作为缓存见证。

共享目录抽取后，旧资源测试两种特性的各 696 行与此前源绑定记录逐条相同，
共复核 2,784 个 API 窗口；没有通过修改旧预算制造等价。

stable Rust 1.99.1 / macOS ARM64：

- 全工作区全特性 967 项通过、43 项既有忽略，排除自启动子测试重复汇总。
- no_std+SIMD 库 275 项、workspace 契约 14 项通过。
- 严格 Clippy、格式、diff、200 个正常格式兼容场景、Linux x86_64 全特性库与
  WASM no_std+SIMD 库检查通过。
- 首次 Clippy 发现无测试框架 bench 目标的 cfg(test) 导入未使用；契约移入独立
  integration target 后通过，没有放宽 lint。
- JSON 与 package allowlist 审计通过；核心包 71 成员、WASM 包 6 成员，新增
  benchmark、共享目录与测试不进入分发包。

全部 64 个 codec src 成员、build.rs、Cargo.lock 与上轮优化及合并后的父提交
逐字节相同。Cargo.toml 仅增加 bench target，整体源码/manifest 身份仍有变化，
历史测量标签和发布资格不能转贴到本提交。已有 benchmark 与正式采样器/门槛未改。

## 后续执行顺序

先按最新 main 的报告建立受控 Linux 上完整、可重复的发布基线，保留全部失败。
这次功能验证没有建立新的资源隔离或通过任何耗时门槛。

局部检查上轮“延迟分配与补集遍历”的取舍时，可使用 `0151f63` 的运行时作为 A，
本轮源码作为 B，给两边放置完全相同的新 bench、Fixture、共享目录和 target 定义。
必须记录原提交与 overlay/派生 manifest 的真实摘要，不能把覆盖后的目录仍标作
未修改的历史构建。新 workspace API 是前提，此微目录不能直接套在发布版 9.0.0
并宣称兼容的发布比较。

受控测量前先构建并冻结双方可执行文件：

```bash
cargo +stable bench --no-run --locked -p rustfs-erasure-codec \
  --features simd-accel --bench reconstruct_plans
```

保留编译器、features、全部源码/manifest/lock、二进制和输入绑定；测量阶段不穿插
构建或修改源码。完整记录 A1/B1/B2/A2 和所有布局，两组同版重复都需通过既有门槛。
任何失败均保留并停止归因。该微目录只补充局部诊断，不能替代正式完整基线、ABBA、
逐布局补充及发布正向预检。

可独立继续的工具工作是为此目录接入冻结二进制的阶段采集、完整 case/来源校验及
独立重放，保留所有失败后再进入受控计时。

真实调用频率、宽度/缺失分布及无输出比例仍需调用方证据。普通 verify 分块与
Leopard oversized 缓存策略继续等待各自的生产瓶颈和内存预算。
本轮已进行本地 main 合入、基准及证据 commit，未 push、创建 PR、打 tag 或发布 crate。

## 后续进展

已创建 PR 后继续实现的 Git 来源冻结、功能阶段采集、独立 pin 和离线重放见
[冻结计划基准跟进报告](frozen-plan-capture-follow-up-2026-10-26.md)。
上文的功能结果、源码身份和计时边界保持。
