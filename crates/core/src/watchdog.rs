//! Progress-based timeout for live tests: a slow machine is fine as long as the engine keeps talking.
use std::time::{Duration, Instant};

/// Gives up after `stall` without a new engine message, or after `cap` in total.
pub struct Watchdog {
    start: Instant,
    last_progress: Instant,
    seen: u64,
    stall: Duration,
    cap: Duration,
}

impl Watchdog {
    pub fn new(stall: Duration, cap: Duration) -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last_progress: now,
            seen: 0,
            stall,
            cap,
        }
    }

    /// The live-test default: 120 s without progress, 10 min overall.
    pub fn live() -> Self {
        Self::new(Duration::from_secs(120), Duration::from_secs(600))
    }

    /// Feed the running count of received messages; true once the wait should be abandoned.
    pub fn expired(&mut self, received: u64) -> bool {
        if received != self.seen {
            self.seen = received;
            self.last_progress = Instant::now();
        }
        self.last_progress.elapsed() > self.stall || self.start.elapsed() > self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_extends_the_stall_timeout_but_not_the_cap() {
        let mut quiet = Watchdog::new(Duration::from_millis(30), Duration::from_secs(60));
        assert!(!quiet.expired(0));
        std::thread::sleep(Duration::from_millis(50));
        assert!(quiet.expired(0), "no messages for longer than the stall");
        assert!(!quiet.expired(1), "a new message resets the stall clock");

        let mut chatty = Watchdog::new(Duration::from_secs(60), Duration::from_millis(30));
        std::thread::sleep(Duration::from_millis(50));
        assert!(chatty.expired(5), "the absolute cap holds despite progress");
    }
}
