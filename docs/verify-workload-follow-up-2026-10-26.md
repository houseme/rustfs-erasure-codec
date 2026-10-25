# 普通 verify：完整资源矩阵与精确视图容量

接续[GF16 并行输出视图](gf16-parallel-views-follow-up-2026-10-26.md)，本轮回到
全局复审第 3 项，先完善普通 verify 的测量与验证门禁，再收拢实测确认的视图容量浪费。

工具提交：`41a9af1230131fc2e1236cc83797eff63863bd23`，库源码与此前 `e9f2ced` 相同。
运行时提交：`dc70f4dbda1f16538790892bec0cbc649e496493`。
两次测量的工具源码、锁文件和参数相同；远程 main 保持 `07f02df`。

## 新测量入口

新增独立的 --verify-resource-report，不改变原有 45 场景报告及 schema。
同一份 Rust probe 分别绑定真实发布版 9.0.0 与当前 9.1.0 API。
每次先执行既有 200 配置精确 parity 和双向恢复预检，再执行可选报告。
已存在的报告不会被覆盖，新报告在重型目录运行前以 create_new 打开。

目录固定为 Classic/Vandermonde、fast_one_parity=false、max_parallel_jobs=1：

- GF8 与 tower GF16 两种字段，明确区分元素数量、元素字节数和每片字节数。
- 4+2/1024、4+2/16384、10+4/32768、4+2/1048576、10+4/1048576，以及 2+33/65。
  数值均为每片元素数；16/32 KiB 的 GF8 项沿用既有下游验证 workload 尺寸。
  1 Mi 元素项是大分片压力对照，不是生产分布采样。
- verify、verify_with_buffer、verify_with_workspace 三个独立入口。
- 有效输入、首个 parity 的首元素损坏、最后一个 parity 的尾元素损坏。

共 12 个 fixture、108 个观察项，每项每版本预热两次并计数三次，即每份报告 648 个
计数窗口。每个返回布尔值都检查；每个 fixture 的完整 parity 跨版本逐字节比较，
随后释放该 fixture，避免为每个 API/损坏项重复保留整片 parity。
校验器拒绝缺失/乱序样本、错误结果、损坏位置、元素单位及 parity 见证不一致。

构造 codec、输入、调用者 buffer/workspace、输入引用适配器均在计数外。
普通 verify 的 scratch 留在窗口内；另两项使用已准备并预热的调用者资源。
原子计数器覆盖同步调用期间所有 Rust allocator 线程的请求；本目录测的是串行 API。
这些是请求量，不是峰值、RSS 或耗时；保留 workspace 仍占用相应驻留存储。

```bash
cargo +stable run --release --locked -p rse-release-compatibility \
  -- --verify-resource-report NEW_OUTPUT_FILE.json
```

[前报告](../benchmarks/global-review/2026-10-26/verify-workload-before.json)、
[后报告](../benchmarks/global-review/2026-10-26/verify-workload-after.json)完整保留全部观察，
[源码绑定及独立重放](../benchmarks/global-review/2026-10-26/verify-workload-bindings.json)
说明工具/运行时提交、文件摘要、调用方式及每个差异。
两份报告的 baseline 列均为发布版 9.0.0；本轮容量改动的前后对照使用 candidate 列。
不能把整段 9.0.0 → 当前源码差异归因于本轮调整。

## 实测驱动的容量调整

108 项矩阵确认：2+33、65 元素时，普通 verify 的 payload scratch 仍可内联，
但引用表 collect 从 32 个内联槽增长到 64 个 heap 槽，申请 1,024 B。
当前已知目标数仅为 33 个；本机每个 slice view 为 16 B，所需表为 528 B。

普通串行和并行策略入口现在先按 parity_count 预留容量，再 extend 原有 chunks_mut
迭代器。没有新增阈值、unsafe、公共 API、分块策略或数学路径；引用顺序、scratch
长度、输入验证、内核选择和 workspace 行为保持。
在与锁文件对应的 SmallVec 1.16.4 中，with_capacity 使用 reserve_exact。

外部回归先在未改动源码上收集两字段、三种损坏状态、三次重复的全部记录并实际失败，
修改后同一回归通过。72 个计数窗口/版本覆盖普通串行、并行策略和对应直接 buffer
控制；并行策略控制固定一作业，不代表多 worker 吞吐测量。
两个普通入口均由 1 次 / 1,024 B 变为 1 次 / 528 B，控制入口保持原成本。

独立逐项重放两份完整矩阵，只有以下六项 candidate 请求字节改变：
GF8/tower GF16 的 2+33/65 普通 verify，在有效/首部/尾部损坏三种状态下均为
1,024 → 528 B。所有已发布版样本及其余 102 项 candidate 观察完全不变。
没有选取单个损坏状态或省略其他目录来宣布改进。

## 整片 scratch 的判断边界

本轮没有把普通 verify 改为分块。有效、首部损坏和尾部损坏在当前资源矩阵中申请相同
大小的整片 scratch。当前大样本结果如下，三次计数均一致：

| 字段 / 布局 / 每片元素数 | 每片字节数 | 普通 verify 每次申请字节 |
| --- | ---: | ---: |
| GF8 / 4+2 / 1,048,576 | 1 MiB | 2 MiB |
| GF8 / 10+4 / 1,048,576 | 1 MiB | 4 MiB |
| tower GF16 / 4+2 / 1,048,576 | 2 MiB | 4 MiB |
| tower GF16 / 10+4 / 1,048,576 | 2 MiB | 8 MiB |

当前 buffer/workspace 的热调用在这些对照项中没有新增请求；初始化和驻留成本在窗口外。
同一调用者可沿用已有复用 API，但这不替代普通 API 的真实生命周期。
这些结果建立了 scratch 请求量，尚未证明其是生产瓶颈，也没有提供实际并发/进程内存
预算或稳定时间证据，不能仅据此选择 chunk 阈值或声称更快。

另新增异常输入优先级回归：先有 parity 首部损坏，再让最后一个 shard 长度非法。
两字段、普通/fast parity、六个串行/并行策略入口均须先返回 IncorrectShardSize，
不准备 scratch、不改变调用者 buffer，也不调整原本不同形状的 workspace。
这防止未来分块实现因提前返回 false 而隐藏后面的非法输入。

## 验证与交付

stable Rust 1.99.1 / macOS ARM64：

- 最终全工作区全特性 936 项通过、43 项原有忽略；排除自启动子测试重复计数。
- 独立 no_std+SIMD 库 273 项通过，严格 Clippy、格式、diff 及被 include probe 的显式 rustfmt 检查通过。
- 200 配置发布版兼容性及双向恢复预检保持通过。
- JSON 与分发包 allowlist 审计通过：核心包 71 个成员、WASM 包 6 个成员，工具与本轮证据不进入分发包。
- [原有 45 场景报告](../benchmarks/global-review/2026-10-26/verify-workload-legacy45.json)
  实际运行通过，完整保留三次计数；未用新目录替换旧协议。
- x86_64 Linux 全特性库及 WASM no_std+SIMD 库交叉编译检查通过，未执行 x86 实机性能验收。
- 实际生成并编译 verify-scratch 3 个成员和 stream-core 4 个成员；两个注入副本各通过
  25 项功能回归。严格源码摘要更新，资源计数留在未注入版本；没有放宽 probe 校验。

本轮完成第 3 项的串行资源矩阵、损坏/非法输入门禁及已确认的引用表容量收敛。
普通大分片分块 scratch 的生产负载、稳定耗时及并行/native 路径速度保真仍需单独验证。
第 5 项 oversized decode 保留旧小缓存的替代策略仍等待线程/峰值预算。
历史 ABBA 漂移失败记录保持，本轮没有重新计时、吞吐提升结论或发布性能验收。

本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。
