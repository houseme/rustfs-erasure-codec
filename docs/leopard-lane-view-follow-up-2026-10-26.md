# Leopard encoder：跨块复用 lane 引用表

接续[GF16 encoder 闲置 scratch 清理](gf16-encoder-active-work-follow-up-2026-10-26.md)，
本轮将全局复审第 2、4 项的调用内视图复用扩展到 Leopard GF8/GF16 编码。

实现提交：`709a20886e25a0c329a4b2b4a7fba4123dff8227`。
基线为上一轮交付 `c8507b7`，已包含远程 main
`07f02df5f78ada84588a6f4fc737b7fb484d33d1`，前后锁文件相同。

## 借用与分块边界

原 encoder 在每个 chunk 内调用 with_lane_views，构造一张
SmallVec<[&mut [Elem]; 96]>。超过 96 个 work lanes 后，每块为引用表申请堆内存。
上一轮移除 GF16 scratch 后，这份逐块元数据成本仍存在。

现在同一个 with_lane_views 回调包含整次调用的分块循环，引用表只构造一次。
所有完整块沿用原长度；只有最后一个短尾通过安全可变重借用收窄各 lane 的切片。
共享帮助函数只修改借用长度，原始 lane 起始地址、分配跨度和 FlatWork 容量保持。
单块调用仍按实际 chunk_cap 创建视图，不扩大有效数据范围。

引用表在回调返回时销毁，随后 work 才按原规则返回缓存，没有把可变引用存进 TLS。
没有新增 unsafe、payload 复制、公共 API、调度开关或尺寸阈值。
GF8 的必要 scratch、GF16 的直接 work 运算、FFT 计划、内核选择、GF8 大 chunk 策略、
计数器的循环位置及缓存预算保持。末尾收窄不会改变下一次调用的工作区尺寸。

## 资源对照

[完整资源记录与源码绑定](../benchmarks/global-review/2026-10-26/leopard-lane-view-resources.json)
保留每版本 9 条配对记录、18 个计数窗口。每个布局重复三次，使用完全相同的最终外部
测试覆盖旧源码；旧实现收集全部记录并检查输出后预算断言实际失败，候选通过。

每组在新的 OS 线程先执行一次完整 encode_sep，再计数第二次调用。
codec、输入、调用者输出容器及同版参考 parity 在窗口外准备；计数包括原有政策下
仍需本地申请的 oversized work、plan、scratch 和其他 API 视图。
在计数前将输出填为另一非零值，计数后逐字节核对全部 parity。

计算保持现有串行路径，计数覆盖调用线程的 alloc/alloc_zeroed/realloc 请求及累计
申请字节。没有把其他线程的申请遗漏为零；计数不代表存活字节或进程 RSS。
短/长分片长度分别为 chunk + 64 B、5 * chunk + 64 B，均带 64 B 短尾，对应 2/6 块。

| 家族、布局、chunk | 2 块申请次数，前 → 后 | 6 块申请次数，前 → 后 | 6 块申请字节，前 → 后 |
| --- | ---: | ---: | ---: |
| GF8 / 4+64 / 32 KiB | 11 → 10 | 15 → 10 | 4,242,016 → 4,231,776 |
| GF8 / 80+64 / 128 KiB | 17 → 16 | 21 → 16 | 16,925,472 → 16,915,232 |
| GF16 / 4+64 / 64 KiB | 10 → 9 | 14 → 9 | 8,403,552 → 8,393,312 |

GF8 80+64 达到既有的总数 144 且带 IFFT remainder 的条件，长短输入都沿用
128 KiB chunk；其他配置沿用原 driver 选择。没有强制切换 chunk 来制造资源差异。

本次三个布局均为 128 个 lane views，每次被消除的表对应 2,048 B 请求。
2 块由两张表变为一张，6 块由六张变为一张。旧版从 2 块增加到 6 块时多申请
4 次、8,192 B；候选两项计数均保持不变。
每次 API 调用仍保留一张宽引用表，以及 payload 和其他元数据申请，不能写成整个 API
零分配，也不能把请求量下降等同于峰值 RSS 或吞吐提升。

```bash
cargo +stable test --release --locked -p rustfs-erasure-codec --features simd-accel \
  --test leopard_encode_reuse_regressions wide_lane_view_requests -- --nocapture
```

## 功能与工具验证

新增功能回归覆盖 GF8 普通/大 chunk、GF16、宽布局及可缓存的 10+4 对照。
长度依次包含单个 64 B、整块、整块加短尾、两整块及回到整块。
整片 parity 与逐个独立 64 B wire block 对照，并检查复用工作区后的全零输入和脏输出。
这验证短尾有效范围、后续调用容量及原有工作区初始化。

stable Rust 1.99.1 / macOS ARM64：

- 全工作区全特性 930 项通过、43 项原有忽略，排除自启动子测试重复计数。
- 独立 no_std+SIMD 库 273 项及同模式 encoder 集成回归 3 项通过。
- 严格全目标全特性 Clippy、格式与 diff 检查通过。
- 发布版 9.0.0 与路径 9.1.0 的 200 种非 fast 配置精确 parity、双向恢复及历史向量通过。
- x86_64 Linux 全特性库和 WASM no_std+SIMD 库交叉检查通过，未执行 x86 实机基准。
- JSON 与分发包 allowlist 审计通过：核心包 71 个成员、WASM 包 6 个成员，工具和本轮证据不进入分发包。

实际 dynamic source-plan 在新旧冻结源码副本各生成 9 个成员，编译并通过 15 项功能
回归。work 帮助函数文档改变后按实际生成字节更新严格摘要，未放宽 ledger 或锚点校验。
encoder 文件随完整副本构建；该 observer 没有 encoder 内部所有者探针，本轮资源数据
来自未注入版本的外部分配计数。这不是全量生命周期采集或发布验收。

## 后续顺序

本轮完成 Leopard encoder 逐块宽引用表的调用内复用。第 3 项普通大分片 verify 的
有界分块 scratch，仍需要真实负载与稳定时间证据；第 5 项 GF16 decode 的 oversized
调用保留旧小缓存策略，仍需要线程数、大小交替频率及进程峰值内存预算。
历史 ABBA 漂移失败记录保持，本轮没有重新计时或声称吞吐提升。

本轮执行 commit，未 push、创建 PR、打 tag 或发布 crate。

## 后续进展

GF16 并行恢复按既有作业组准备输出视图及全线程资源对照见
[并行输出视图跟进报告](gf16-parallel-views-follow-up-2026-10-26.md)。
以上保留本轮原始实现身份及资源记录。
