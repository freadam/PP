//! Two clocks, deliberately separated.
//!
//! Wall time is what you *display* — it can jump backwards when the user
//! changes the system clock or a DST transition lands mid-session.
//! Monotonic time is what you *count with* — it never goes backwards and, on
//! every platform Fruit targets, it does not advance while the machine is
//! suspended.
//!
//! That difference is not an implementation detail: the gap between the two
//! deltas is precisely how sleep is detected (§4.5, D10), and counting on the
//! monotonic one is what makes `elapsed_sec` immune to a clock change (D9).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub trait Clock: Send + Sync + std::fmt::Debug {
    /// UTC milliseconds since the epoch. May jump.
    fn now_ms(&self) -> i64;
    /// Milliseconds from an arbitrary origin. Never jumps, never runs backwards,
    /// and does not advance across suspend.
    fn mono_ms(&self) -> i64;
}

#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock {
            origin: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
    fn mono_ms(&self) -> i64 {
        self.origin.elapsed().as_millis() as i64
    }
}

/// A clock the tests drive by hand, so "the user slept for 45 minutes" and
/// "the user moved the clock back an hour" are ordinary unit tests rather than
/// things we hope about.
#[derive(Debug, Clone, Default)]
pub struct TestClock {
    wall: Arc<AtomicI64>,
    mono: Arc<AtomicI64>,
}

impl TestClock {
    pub fn new(wall_ms: i64) -> Self {
        TestClock {
            wall: Arc::new(AtomicI64::new(wall_ms)),
            mono: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Current wall reading, without having to import the trait.
    pub fn now(&self) -> i64 {
        self.wall.load(Ordering::SeqCst)
    }

    /// Ordinary passage of time: both clocks advance together.
    pub fn advance(&self, ms: i64) {
        self.wall.fetch_add(ms, Ordering::SeqCst);
        self.mono.fetch_add(ms, Ordering::SeqCst);
    }

    /// Suspend: wall time passes, monotonic time does not.
    pub fn sleep(&self, ms: i64) {
        self.wall.fetch_add(ms, Ordering::SeqCst);
    }

    /// The user (or NTP) moves the system clock. Monotonic time is unaffected.
    pub fn shift_wall(&self, ms: i64) {
        self.wall.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.wall.load(Ordering::SeqCst)
    }
    fn mono_ms(&self) -> i64 {
        self.mono.load(Ordering::SeqCst)
    }
}
