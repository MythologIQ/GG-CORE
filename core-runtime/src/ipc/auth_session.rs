//! Session management and rate limiting for IPC authentication.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Maximum failed authentication attempts before rate limiting kicks in.
pub(super) const MAX_FAILED_ATTEMPTS: u64 = 5;
/// Duration to block after too many failed attempts.
pub(super) const RATE_LIMIT_DURATION: Duration = Duration::from_secs(30);
/// Duration to track failed attempts for rate limiting.
pub(super) const ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
/// Maximum requests per session per minute.
pub(super) const MAX_REQUESTS_PER_MINUTE: u64 = 1000;
/// Request rate limiting window.
pub(super) const REQUEST_WINDOW: Duration = Duration::from_secs(60);
/// Minimum time for session validation to prevent timing attacks.
pub(super) const MIN_VALIDATION_TIME_MICROS: u64 = 100;

/// Internal session state.
pub(super) struct Session {
    pub created_at: Instant,
    pub last_activity: Instant,
    pub connection_count: AtomicUsize,
    pub request_count: AtomicU64,
    pub request_window_start: std::sync::Mutex<Option<Instant>>,
}

/// Rate limiter for authentication attempts.
pub(super) struct RateLimiter {
    failed_attempts: AtomicU64,
    window_start: std::sync::Mutex<Option<Instant>>,
    blocked_until: std::sync::Mutex<Option<Instant>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            failed_attempts: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(None),
            blocked_until: std::sync::Mutex::new(None),
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        if let Ok(blocked_until) = self.blocked_until.lock() {
            if let Some(until) = *blocked_until {
                if Instant::now() < until {
                    return true;
                }
            }
        }
        false
    }

    pub fn record_failure(&self) {
        let now = Instant::now();

        if let Ok(window_start) = self.window_start.lock() {
            let should_reset = window_start
                .map(|start| now.duration_since(start) > ATTEMPT_WINDOW)
                .unwrap_or(true);

            if should_reset {
                self.failed_attempts.store(1, Ordering::SeqCst);
                drop(window_start);
                if let Ok(mut ws) = self.window_start.lock() {
                    *ws = Some(now);
                }
                return;
            }
        }

        let attempts = self.failed_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempts >= MAX_FAILED_ATTEMPTS {
            if let Ok(mut blocked_until) = self.blocked_until.lock() {
                *blocked_until = Some(now + RATE_LIMIT_DURATION);
            }
        }
    }

    pub fn reset(&self) {
        self.failed_attempts.store(0, Ordering::SeqCst);
        if let Ok(mut window_start) = self.window_start.lock() {
            *window_start = None;
        }
        if let Ok(mut blocked_until) = self.blocked_until.lock() {
            *blocked_until = None;
        }
    }
}

/// Constant-time comparison to prevent timing attacks.
pub fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Generate a cryptographically secure random session ID.
pub fn generate_session_id() -> String {
    use rand::RngCore;
    let mut random_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(random_bytes.as_mut_slice());
    hex::encode(random_bytes)
}
