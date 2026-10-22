# Performance Index

The [repository evidence catalog](../benchmarks/README.md) describes JSON
retention, directory purposes and the boundary between GitHub archives and the
published crate. Use `cargo xtask audit-json` to refresh the file inventory.

## Current Useful References

- `docs/main-vs-origin-main-performance-2026-05-28.md`
  - baseline commit vs current working tree on shared smoke operations
- `docs/ec-update-benchmark-results-2026-05-28.md`
  - current-tree `update` vs `encode` paired comparison
- `docs/task-10-classic-parity-update-api.md`
  - implementation status and benchmark summary for `update`
- `docs/task-12-reconstruct-plan-unification.md`
  - specialized reconstruct planner consolidation status
- `docs/stream-path-performance-optimization-v2-plan-2026-06-28.md`
  - stream path v2 analysis, implementation phases, and risk controls
- `benchmarks/stream-path/2026-06-28-cooldown/README.md`
  - archived stream path baseline/current artifacts and cooldown validation summary
- `docs/benchmark-methodology.md`
  - stream benchmark commands, artifact schema, cooldown protocol, and optional release gate
- `docs/release-7.0.1-checklist.md`
  - 7.0.1 release checklist for stream path benchmark governance and release gates
- `docs/release-7.0.2-checklist.md`
  - 7.0.2 release checklist for streaming-path correctness and robustness hardening
