# 9.1.0 发布准备：2026-11-06

本记录针对源码 `c44b29cc1cccd08b9a128bd121f7e01aca62f926`，不构成发布批准。
后续仅有的准备文档或 changelog 改动仍会改变 Git tree；正式证据和严格预检必须绑定
最终候选，不能把本次包的 Git 元数据或普通 CI 结果转记为其他提交的结果。

- 核心、workspace 和 lock 的版本检查通过，`v9.1.0` tag 写法检查通过；WASM companion
  继续使用独立的 0.2.5 版本。远端没有 9.1.0 对应 tag 或 GitHub Release；核查时
  crates.io 索引最新为 9.0.0。
- 当前源码的 `cargo publish --dry-run --locked` 通过并明确中止上传。独立打包产物
  包含 76 个文件；其中 73 个源码及原始元数据文件与当前源码逐字节一致，包内
  `.cargo_vcs_info.json` 记录上述提交。文档和 changelog 不进入核心包。
- 对解包产物执行 locked、offline 库测试：all-features 为 432 通过、5 忽略；
  no-default-features + simd-accel 为 297 通过。临时兼容性工作区使用实际解包
  产物及已发布 9.0.0，离线解析自身 lock；200 组普通配置的精确 parity 和双向
  恢复、36 组大 GF8、12 组 6+2、32 组小块损坏检测和历史 tower 向量通过。
  小块 `ShardSlot`、高扇出及 `Option` 交叉恢复测试也通过。
- 本地 `check-release`、`v9.1.0` tag 格式、包清单、无 Python 和拼写检查通过。
  上述源码的[普通 CI](https://github.com/houseme/rustfs-erasure-codec/actions/runs/42237358389)
  已通过；最终候选须核对其自身的实际结果。
- 仓库存在 `CRATES_IO_TOKEN` 密钥名称；未读取值，也未验证它的上传权限。
  `RSE_RELEASE_EVIDENCE_REF` 当前未配置。

## 仍需完成

1. 冻结最终候选的源码、lockfile、harness 和 tree。最近的
   [小块恢复结果](packed-leopard-recovery-staging-2026-11-06.md)只覆盖所列场景，
   不代替发布要求的完整性能验收。
2. 在资源可用的授权 native Linux 环境中，按现有发布工作流采集并验证完整的
   9.0.0/最终候选基线与 A1/B1/B2/A2，覆盖全部 126 个案例及规定的配对布局；
   保留原始样本和失败记录。检查时两台已授权服务器均有其他任务，未启动正式采集。
3. 验收通过后，将证据写入独立数据提交并配置其完整 SHA，然后在最终 main 上运行
   `release_preflight=true` 的严格预检。证据格式、完整性和实际执行以
   [发布证据约定](release-evidence.md)及 CI 工作流为准，不使用占位或旧源码证据。
4. 全部检查通过后再创建匹配 tag 并确认实际 registry 上传；dry-run 不会发布。
   9.1.0 更新日志在实际发布前继续标记为 unreleased。
