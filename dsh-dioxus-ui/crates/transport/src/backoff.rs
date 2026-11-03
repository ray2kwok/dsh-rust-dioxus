//! 重连退避策略（纯函数，便于测试）。
//!
//! 与上游一致：`500ms → 1s → 2s → 4s → 8s → 10s`（封顶），
//! 抖动由调用方叠加（`delay * [0.5, 1.0]`），这里保持确定性便于断言。

use std::time::Duration;

/// 指数退避。
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            base: Duration::from_millis(500),
            max: Duration::from_secs(10),
        }
    }

    /// 连接成功后重置，下次断线仍从 500ms 起步。
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// 取下一次等待时长，并推进计数。
    pub fn next_delay(&mut self) -> Duration {
        let factor = 2u32.saturating_pow(self.attempt.min(16));
        let delay = self.base.saturating_mul(factor).min(self.max);
        self.attempt = self.attempt.saturating_add(1);
        delay
    }

    /// 当前尝试次数（0 表示尚未重试）。
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_upstream_sequence() {
        let mut backoff = Backoff::new();
        let delays: Vec<u64> = (0..7)
            .map(|_| backoff.next_delay().as_millis() as u64)
            .collect();
        assert_eq!(delays, vec![500, 1000, 2000, 4000, 8000, 10_000, 10_000]);
    }

    #[test]
    fn reset_returns_to_first_step() {
        let mut backoff = Backoff::new();
        backoff.next_delay();
        backoff.next_delay();
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(500));
    }

    #[test]
    fn never_exceeds_cap_even_with_many_attempts() {
        let mut backoff = Backoff::new();
        for _ in 0..64 {
            assert!(backoff.next_delay() <= Duration::from_secs(10));
        }
    }
}
