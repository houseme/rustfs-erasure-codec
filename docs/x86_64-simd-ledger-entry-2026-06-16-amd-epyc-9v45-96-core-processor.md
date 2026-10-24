# 2026-06-16 x86_64 SIMD Ledger Entry (amd-epyc-9v45-96-core-processor)

## 已执行命令

1. `git pull --rebase`
2. `cargo check --features 'std simd-accel' --lib`
3. `./scripts/collect_x86_simd_benchmarks.sh`

## 关键结果

1. 新产物：
   - [benchmarks/x86_64-simd/2026-06-16-amd-epyc-9v45-96-core-processor.json](/data/rustfs/reed-solomon-erasure/benchmarks/x86_64-simd/2026-06-16-amd-epyc-9v45-96-core-processor.json)
   - [benchmarks/x86_64-simd/2026-06-16-amd-epyc-9v45-96-core-processor.run-meta.json](/data/rustfs/reed-solomon-erasure/benchmarks/x86_64-simd/2026-06-16-amd-epyc-9v45-96-core-processor.run-meta.json)
   - [docs/x86_64-simd-benchmark-summary-2026-06-16-amd-epyc-9v45-96-core-processor.md](/data/rustfs/reed-solomon-erasure/docs/x86_64-simd-benchmark-summary-2026-06-16-amd-epyc-9v45-96-core-processor.md)
2. `auto` 在当前主机上实际选中 `rust-gfni-avx512`
3. `10x4_1m` 的 `encode / verify / reconstruct / reconstruct_data` 四项 release smoke 中，`auto` 都是第一
4. 本轮汇总后的 `recommended_default_priority` 为：
   - `rust-gfni-avx512`
   - `rust-avx512`
   - `rust-gfni-avx2`
   - `rust-avx2`
   - `rust-ssse3`
   - `scalar`
   - `simd-c`

## 备注

1. 本轮开始前，先修复了 [src/core/mod.rs](/data/rustfs/reed-solomon-erasure/src/core/mod.rs:99) 的模块路径编译错误，否则 benchmark 无法启动
2. 本轮同时校正了 [原汇总脚本的 Rust 实现](../xtask/src/summary.rs) 中对当前 `x86_64` runtime 优先级的旧假设，使结果记录与当前代码实现一致
