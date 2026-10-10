use std::time::{Duration, Instant};

const RETARGET_INTERVAL: Duration = Duration::from_secs(60);
/// Retarget early once a miner has sent this many times the shares expected per interval.
const BURST_FACTOR: f64 = 3.0;
const MAX_STEP: f64 = 4.0;
const TOLERANCE: std::ops::RangeInclusive<f64> = 0.7..=1.3;

#[derive(Debug, Clone, Copy)]
pub struct VarDiffConfig {
    pub start: f64,
    pub min: f64,
    pub max: f64,
    pub share_target_secs: f64,
}

/// Per-connection difficulty that steers a miner towards one share per `share_target_secs`.
pub struct VarDiff {
    cfg: VarDiffConfig,
    diff: f64,
    window_start: Instant,
    shares: u32,
}

impl VarDiff {
    pub fn new(cfg: VarDiffConfig, now: Instant) -> Self {
        Self { cfg, diff: cfg.start.clamp(cfg.min, cfg.max), window_start: now, shares: 0 }
    }

    pub fn diff(&self) -> f64 {
        self.diff
    }

    pub fn on_share(&mut self) {
        self.shares += 1;
    }

    /// Returns the new difficulty if it changed.
    pub fn retarget(&mut self, now: Instant) -> Option<f64> {
        let elapsed = now.duration_since(self.window_start);
        let per_interval = RETARGET_INTERVAL.as_secs_f64() / self.cfg.share_target_secs;
        if elapsed < RETARGET_INTERVAL && f64::from(self.shares) < per_interval * BURST_FACTOR {
            return None;
        }
        let expected = elapsed.as_secs_f64() / self.cfg.share_target_secs;
        let ratio = f64::from(self.shares) / expected;
        self.window_start = now;
        self.shares = 0;
        if TOLERANCE.contains(&ratio) {
            return None;
        }
        let next = (self.diff * ratio.clamp(1.0 / MAX_STEP, MAX_STEP)).clamp(self.cfg.min, self.cfg.max);
        if next == self.diff {
            return None;
        }
        self.diff = next;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: VarDiffConfig = VarDiffConfig { start: 1.0, min: 0.01, max: 100.0, share_target_secs: 15.0 };

    fn after(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn on_target_rate_keeps_difficulty() {
        let t0 = Instant::now();
        let mut v = VarDiff::new(CFG, t0);
        (0..4).for_each(|_| v.on_share());
        assert_eq!(v.retarget(after(t0, 60)), None);
    }

    #[test]
    fn fast_miner_is_raised_before_the_interval_ends() {
        let t0 = Instant::now();
        let mut v = VarDiff::new(CFG, t0);
        (0..12).for_each(|_| v.on_share());
        assert_eq!(v.retarget(after(t0, 5)), Some(4.0));
    }

    #[test]
    fn silent_miner_is_lowered_stepwise_down_to_min() {
        let t0 = Instant::now();
        let mut v = VarDiff::new(CFG, t0);
        assert_eq!(v.retarget(after(t0, 30)), None);
        assert_eq!(v.retarget(after(t0, 60)), Some(0.25));
        assert_eq!(v.retarget(after(t0, 120)), Some(0.0625));
        assert_eq!(v.retarget(after(t0, 180)), Some(0.015625));
        assert_eq!(v.retarget(after(t0, 240)), Some(0.01));
        assert_eq!(v.retarget(after(t0, 300)), None);
    }

    #[test]
    fn proportional_adjustment() {
        let t0 = Instant::now();
        let mut v = VarDiff::new(CFG, t0);
        (0..8).for_each(|_| v.on_share());
        assert_eq!(v.retarget(after(t0, 60)), Some(2.0));
    }
}
