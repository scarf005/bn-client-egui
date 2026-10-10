//! Time source for debounce and settle delays, so tests can advance time instead of sleeping.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The wall clock, or a manual one that moves only on `advance`. Clones of a manual clock share its time.
#[derive(Clone, Debug)]
pub struct Clock {
    origin: Instant,
    manual: Option<Arc<AtomicU64>>,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            manual: None,
        }
    }
}

impl Clock {
    pub fn manual() -> Self {
        Self {
            manual: Some(Arc::default()),
            ..Self::default()
        }
    }

    /// Time since the clock was made.
    pub fn now(&self) -> Duration {
        match &self.manual {
            Some(t) => Duration::from_micros(t.load(Ordering::SeqCst)),
            None => self.origin.elapsed(),
        }
    }

    /// Moves a manual clock forward; a wall clock ignores it.
    pub fn advance(&self, by: Duration) {
        if let Some(t) = &self.manual {
            t.fetch_add(by.as_micros() as u64, Ordering::SeqCst);
        }
    }
}
