//! Uniform formal service windows, declared before any campaign/calibration.
//! The controller reads these same constants; no per-case/count exceptions.
pub(super) const TARGET_WALL_NS: u64 = 2_000_000_000;
pub(super) const TARGET_TIMED_NS: u64 = 100_000_000;
pub(super) const MINIMUM_WALL_NS: u64 = 1_000_000_000;
pub(super) const MINIMUM_TIMED_NS: u64 = 40_000_000;
