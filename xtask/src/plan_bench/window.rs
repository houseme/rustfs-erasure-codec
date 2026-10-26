//! One deadline model for live admission and original-record replay.
use crate::Result;
use std::time::Duration;
const SECOND: u64 = 1_000_000_000;
pub(super) struct Budget {
    stage: u64,
    total: u64,
    gap: u64,
}
impl Budget {
    pub fn new(stage_seconds: u64, window_seconds: u64, gap_seconds: u64) -> Result<Self> {
        if !(1..=86400).contains(&stage_seconds)
            || !(1..=604800).contains(&window_seconds)
            || !(1..=60).contains(&gap_seconds)
            || stage_seconds > window_seconds
        {
            return Err("invalid execution deadline budget".into());
        }
        Ok(Self {
            stage: stage_seconds * SECOND,
            total: window_seconds * SECOND,
            gap: gap_seconds * SECOND,
        })
    }
    pub fn admit(&self, previous: u64, before: u64) -> Result<()> {
        if before < previous || before >= self.total || before.saturating_sub(previous) > self.gap {
            return Err("collection gap or total deadline expired before worker".into());
        }
        Ok(())
    }
    pub fn worker_timeout(&self, before: u64, started: u64) -> Result<Duration> {
        let remaining = self
            .stage
            .saturating_sub(started.saturating_sub(before))
            .min(self.total.saturating_sub(started));
        if started < before || remaining == 0 {
            return Err("stage or total deadline expired before worker spawn".into());
        }
        Ok(Duration::from_nanos(remaining))
    }
    pub fn check(&self, previous: u64, times: [u64; 4]) -> Result<()> {
        let [before, started, ended, after] = times;
        if !(previous <= before
            && before <= started
            && started <= ended
            && ended <= after
            && after <= self.total)
            || before.saturating_sub(previous) > self.gap
        {
            return Err("overlapping, discontinuous or expired execution window".into());
        }
        if after.saturating_sub(before) > self.stage {
            return Err("stage envelope exceeded its deadline".into());
        }
        Ok(())
    }
    pub fn finish(&self, previous: u64, ended: u64) -> Result<()> {
        if ended < previous || ended > self.total || ended.saturating_sub(previous) > self.gap {
            return Err("invalid terminal execution window".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_timeout_accounts_for_observation_and_journal_overhead() {
        let b = Budget::new(2, 10, 1).unwrap();
        assert_eq!(
            b.worker_timeout(SECOND, SECOND + 500_000_000).unwrap(),
            Duration::from_millis(1500)
        );
        assert_eq!(
            b.worker_timeout(9 * SECOND, 9 * SECOND + 500_000_000)
                .unwrap(),
            Duration::from_millis(500)
        );
        assert!(b.worker_timeout(0, 2 * SECOND).is_err());
        assert!(b.worker_timeout(2, 1).is_err());
    }
    #[test]
    fn envelope_and_terminal_bounds_include_processing_after_worker_exit() {
        let b = Budget::new(1, 10, 1).unwrap();
        assert!(b.check(0, [0, 10, 20, SECOND]).is_ok());
        assert!(b.check(0, [0, 10, 20, SECOND + 1]).is_err());
        assert!(b.check(100, [99, 110, 120, 130]).is_err());
        assert!(b.admit(0, SECOND + 1).is_err());
        assert!(b.admit(9 * SECOND, 10 * SECOND).is_err());
        assert!(b.finish(SECOND, 2 * SECOND).is_ok());
        assert!(b.finish(SECOND, 2 * SECOND + 1).is_err());
        assert!(b.finish(1, u64::MAX).is_err());
    }
}
