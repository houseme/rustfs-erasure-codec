# 9.1.0 发布准备

准备基线为远程 main `6167745b3583bfe8a4f87827b7fedb13b5fce0d4`。
其完整源码树与本地已验证交付 `55bcf2b` 一致；main 普通
[CI 40457944158](https://github.com/houseme/rustfs-erasure-codec/actions/runs/40457944158)
已成功，严格证据/发布任务属于单独门槛。当前没有未合并 PR。

本轮整理 changelog、[发布说明草稿](release-9.1.0-notes.md)和操作顺序，不修改 codec、
manifest、lockfile、构建脚本或发布工作流。源码与运行时验证沿用上述完全相同的基线；
准备提交的实际分发包另行执行 locked dry-run、解包测试及 published-9.0.0 对照。
实际结果已追加在本页及[准备验证记录](../benchmarks/release-preparation/2026-10-27/validation.json)。

## 当前状态

| 项目 | 状态 |
| --- | --- |
| 核心版本、workspace 与 lock | 9.1.0；已有一致性检查，本轮复核 |
| WASM companion | Cargo/npm 均为 0.2.5，独立版本，不随核心任意升级 |
| 普通 CI 与源码功能验证 | 上述 main CI 已通过；同树本地全工作区 1,057 项通过、43 项既有忽略 |
| changelog 与说明 | 同一 9.1.0 候选入口，保持 unreleased，无发布日期或整体速度宣称 |
| 实际包与兼容性 | 干净提交 5f68261 的实际 crate 已验证，见下文 |
| 完整性能证据 | 待完成；正式计时按既有选择继续暂缓 |
| RSE_RELEASE_EVIDENCE_REF | 当前未配置；没有填入占位 SHA |
| 最终 main strict preflight | 待证据验收与准备 PR 合并后执行 |
| tag / registry upload | 未执行 |

本页准备结果不会补签历史性能失败或授予发布资格。历史源码、原始数据、拒绝结果和
测试计数保留其原有范围。此阶段不新增通用诊断框架，也不调整性能门槛。

## 证据与发布操作顺序

1. 合并发布准备 PR，确认最终 main 普通 CI。冻结整个候选 tree、lockfile、harness
   与发布文案后再采集；文档改动同样会改变 tree。
2. 在专用或明确授权的 native Linux 保留资源上，完成当前规则要求的稳定全基线、
   完整 ABBA 和配对布局补充；保留全部失败与原件，不择优重跑或放宽阈值。
3. 对最终候选执行既有 verify，审阅成功结果：

```bash
cargo +stable xtask release-evidence verify \
  --repository . --manifest "$EVIDENCE_MANIFEST" --report "$NEW_VERIFY_REPORT"
```

4. 将通过验收的 manifest 及其引用的 summary、校准、样本和布局数据，提交并推送
   到同仓库独立数据分支，根路径为 `release-evidence/manifest.json`。候选 tree 保持不变。
   不创建空白或伪造的成功 manifest。
5. 仅在已有验收成功的数据提交后，设置仓库 Actions variable。下面的占位值必须换成
   实际数据提交的完整 40 位小写 SHA：

```bash
EVIDENCE_SHA="<accepted-data-commit-sha>"
gh variable set RSE_RELEASE_EVIDENCE_REF \
  --repo houseme/rustfs-erasure-codec --body "$EVIDENCE_SHA"
gh workflow run ci.yml --repo houseme/rustfs-erasure-codec --ref main \
  -f release_preflight=true -f evidence_ref="$EVIDENCE_SHA"
```

手动 evidence_ref 优先于仓库变量。数据 SHA 选择证据提交，manifest 单独绑定候选
源码；分支名、候选源码 SHA 或本机文件路径不能替代已验收数据的引用。

6. 核对 strict evidence 和 release-preflight 的实际成功结果。只有所有前置项通过后，
   再单独执行 tag 和 registry 上传；本轮不执行这一步。

规则以[发布阻塞清单](release-9.1.0-blockers.md#release-sequence-and-benchmark-decision)、
[证据约定](release-evidence.md#publish-a-data-snapshot-after-acceptance)和实际
[工作流](../.github/workflows/ci.yml)为准。证据 data-only 提交不改变候选源树；当前
准备文档尚未形成最终冻结候选。

## 本轮实际分发包验证

验证对象为干净准备提交 `5f68261282e4932112fc8ca831b54688612e3e3a` 生成的实际 crate，
其 .cargo_vcs_info 确认提交及非 dirty 状态。publish dry-run 与独立 package 均成功；
归档已保留，71 个文件中 64 份 codec 源码、build.rs、README、LICENSE 与原始 manifest
逐字节匹配。工具、文档、证据及 changelog 不进入核心包。

| 检查 | 实际结果 |
| --- | --- |
| 核心/workspace/lock 与两种 tag 写法 | 9.1.0 一致；仅验证写法，没有创建 tag |
| 实际 crate publish dry-run | 通过；上传由 dry-run 明确中止 |
| 解包后的 all-features 库测试 | 404 通过、0 失败、5 项既有忽略 |
| 解包后的 no_std+SIMD 库测试 | 275 通过、0 失败、0 忽略 |
| published-9.0.0 对照 | 200 组普通 non-fast parity、双向恢复与历史 tower 向量通过 |
| 比较对象 | 实际 registry 9.0.0 与上述解包 9.1.0；六份原比较程序源码未改 |
| registry/tag 只读复核 | 当前 registry 9.0.0，未发现 9.1.0 或对应远程 tag |
| 发布 secret | 仅确认 CRATES_IO_TOKEN 名称存在，未读取值或实测上传权限 |

执行使用 stable Rust 1.99.1/macOS ARM64，未运行正式计时。标准库和 no_std 是独立
配置，不将重复覆盖的测试相加为新增用例。WASM/Node/Chromium 的现有成功 CI 仍绑定
main 6167745；本轮没有为文档改动重复浏览器测试。

本记录本身位于后续文档提交，测试 archive 保留原 5f68261 身份；其运行时/构建/README/
manifest/lock 与后续准备内容一致。合并后最终 main 的普通 CI 和严格 preflight 必须
重新验证其自身 head，不能把本轮包元数据或本地测试视为最终发布批准。候选整树包含
这些文档，正式采集须在准备合并并冻结之后进行。

仓库审计通过：672 份 JSON 无错误，5 个成员依赖继承一致，无 Python 入口；
核心/WASM allowlist 为 71/6，格式及 diff 检查通过。
