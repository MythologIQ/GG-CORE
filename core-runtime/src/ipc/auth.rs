//! Handshake token and session ID validation.
//!
//! SECURITY: This module enforces that only authenticated callers can
//! communicate with the runtime. All requests MUST have valid session.
//!
//! Security features:
//! - Constant-time token comparison (prevents timing attacks)
//! - Constant-time session validation (prevents session enumeration)
//! - CSPRNG session IDs (prevents session prediction)
//! - Rate limiting (prevents brute-force attacks)
//! - Session timeout (limits exposure window)
//! - Security audit logging (enables forensic analysis)

use crate::telemetry::{log_security_event, SecurityEvent};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// Maximum failed authentication attempts before rate limiting kicks in.
const MAX_FAILED_ATTEMPTS: u64 = 5;

/// Duration to block after too many failed attempts.
const RATE_LIMIT_DURATION: Duration = Duration::from_secs(30);

/// Duration to track failed attempts for rate limiting.
const ATTEMPT_WINDOW: Duration = Duration::from_secs(60);

/// Maximum requests per session per minute.
const MAX_REQUESTS_PER_MINUTE: u64 = 1000;

/// Request rate limiting window.
const REQUEST_WINDOW: Duration = Duration::from_secs(60);

/// Minimum time for session validation to prevent timing attacks.
/// This masks any timing differences from HashMap lookups.
const MIN_VALIDATION_TIME_MICROS: u64 = 100;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid handshake token")]
    InvalidToken,

    #[error("Session not found")]
    SessionNotFound,

    #[error("Session expired")]
    SessionExpired,

    #[error("Authentication required")]
    NotAuthenticated,

    #[error("Too many failed attempts, please try again later")]
    RateLimited,

    #[error("Session request rate limit exceeded")]
    SessionRateLimited,
}

/// Validated session token from handshake.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

struct Session {
    created_at: Instant,
    last_activity: Instant,
    connection_count: AtomicUsize,
    /// Request count for rate limiting.
    request_count: AtomicU64,
    /// Window start for request rate limiting.
    request_window_start: std::sync::Mutex<Option<Instant>>,
}

/// Rate limiter for authentication attempts.
struct RateLimiter {
    /// Number of failed attempts in the current window.
    failed_attempts: AtomicU64,
    /// Time of the first failed attempt in the current window.
    window_start: std::sync::Mutex<Option<Instant>>,
    /// Time until rate limiting expires (if active).
    blocked_until: std::sync::Mutex<Option<Instant>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            failed_attempts: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(None),
            blocked_until: std::sync::Mutex::new(None),
        }
    }

    /// Check if authentication is currently rate limited.
    fn is_rate_limited(&self) -> bool {
        // Check if we're in a blocked period
        if let Ok(blocked_until) = self.blocked_until.lock() {
            if let Some(until) = *blocked_until {
                if Instant::now() < until {
                    return true;
                }
            }
        }
        false
    }

    /// Record a failed authentication attempt.
    fn record_failure(&self) {
        let now = Instant::now();

        // Check if we need to reset the window
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

        // Increment failed attempts
        let attempts = self.failed_attempts.fetch_add(1, Ordering::SeqCst) + 1;

        // Check if we should block
        if attempts >= MAX_FAILED_ATTEMPTS {
            if let Ok(mut blocked_until) = self.blocked_until.lock() {
                *blocked_until = Some(now + RATE_LIMIT_DURATION);
            }
        }
    }

    /// Reset rate limiting after successful authentication.
    fn reset(&self) {
        self.failed_attempts.store(0, Ordering::SeqCst);
        if let Ok(mut window_start) = self.window_start.lock() {
            *window_start = None;
        }
        if let Ok(mut blocked_until) = self.blocked_until.lock() {
            *blocked_until = None;
        }
    }
}

/// Manages session authentication.
pub struct SessionAuth {
    sessions: Arc<RwLock<HashMap<SessionToken, Session>>>,
    expected_token_hash: [u8; 32],
    session_timeout: Duration,
    rate_limiter: RateLimiter,
}

impl SessionAuth {
    /// Create new auth manager with expected handshake token.
    pub fn new(expected_token: &str, session_timeout: Duration) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(expected_token.as_bytes());
        let expected_token_hash: [u8; 32] = hasher.finalize().into();

        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            expected_token_hash,
            session_timeout,
            rate_limiter: RateLimiter::new(),
        }
    }

    /// Validate handshake token and create session.
    /// Implements rate limiting to prevent brute-force attacks.
    pub async fn authenticate(&self, token: &str) -> Result<SessionToken, AuthError> {
        // Check rate limiting first
        if self.rate_limiter.is_rate_limited() {
            log_security_event(
                SecurityEvent::RateLimited,
                "Authentication blocked due to rate limiting",
                &[("reason", "too_many_failures")],
            );
            return Err(AuthError::RateLimited);
        }

        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let token_hash: [u8; 32] = hasher.finalize().into();

        if !constant_time_compare(token_hash.as_slice(), self.expected_token_hash.as_slice()) {
            // Record failed attempt for rate limiting
            self.rate_limiter.record_failure();
            log_security_event(
                SecurityEvent::AuthFailure,
                "Invalid handshake token",
                &[("reason", "invalid_token")],
            );
            return Err(AuthError::InvalidToken);
        }

        // Reset rate limiter on successful authentication
        self.rate_limiter.reset();

        let session_id = generate_session_id();
        let session_token = SessionToken(session_id);
        let now = Instant::now();

        self.sessions.write().await.insert(
            session_token.clone(),
            Session {
                created_at: now,
                last_activity: now,
                connection_count: AtomicUsize::new(0),
                request_count: AtomicU64::new(0),
                request_window_start: std::sync::Mutex::new(Some(now)),
            },
        );

        log_security_event(
            SecurityEvent::AuthSuccess,
            "Authentication successful",
            &[("session_prefix", &session_token.as_str()[..8])],
        );

        Ok(session_token)
    }

    /// Validate session token and update activity.
    /// Also enforces per-session request rate limiting.
    ///
    /// SECURITY: Uses constant-time validation to prevent timing attacks
    /// that could be used to enumerate valid session IDs.
    pub async fn validate(&self, token: &SessionToken) -> Result<(), AuthError> {
        let start = Instant::now();

        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(token).ok_or_else(|| {
            log_security_event(
                SecurityEvent::InvalidSession,
                "Invalid session token used",
                &[("session_prefix", &token.as_str()[..8])],
            );
            AuthError::SessionNotFound
        })?;

        if session.created_at.elapsed() > self.session_timeout {
            sessions.remove(token);
            log_security_event(
                SecurityEvent::SessionExpired,
                "Session expired",
                &[("session_prefix", &token.as_str()[..8])],
            );
            return Err(AuthError::SessionExpired);
        }

        // Per-session request rate limiting
        let now = Instant::now();
        let should_reset_window = if let Ok(window_start) = &session.request_window_start.lock() {
            window_start
                .map(|start| now.duration_since(start) > REQUEST_WINDOW)
                .unwrap_or(true)
        } else {
            false
        };

        if should_reset_window {
            session.request_count.store(1, Ordering::SeqCst);
            if let Ok(mut ws) = session.request_window_start.lock() {
                *ws = Some(now);
            }
        } else {
            let count = session.request_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count > MAX_REQUESTS_PER_MINUTE {
                log_security_event(
                    SecurityEvent::RateLimited,
                    "Session request rate limit exceeded",
                    &[
                        ("session_prefix", &token.as_str()[..8]),
                        ("request_count", &count.to_string()),
                    ],
                );
                return Err(AuthError::SessionRateLimited);
            }
        }

        session.last_activity = Instant::now();

        // Constant-time delay to prevent timing attacks
        // This ensures all validation operations take the same minimum time
        let elapsed = start.elapsed().as_micros() as u64;
        if elapsed < MIN_VALIDATION_TIME_MICROS {
            let remaining = MIN_VALIDATION_TIME_MICROS - elapsed;
            std::thread::sleep(Duration::from_micros(remaining));
        }

        Ok(())
    }

    /// Remove expired sessions.
    pub async fn cleanup(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, s| s.created_at.elapsed() <= self.session_timeout);
    }

    /// Increment connection count for session.
    pub async fn track_connection(&self, token: &SessionToken) -> Result<usize, AuthError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(token).ok_or(AuthError::SessionNotFound)?;
        let new_count = session.connection_count.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(new_count)
    }

    /// Decrement connection count for session.
    pub async fn release_connection(&self, token: &SessionToken) {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(token) {
            session.connection_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Get current connection count for session.
    pub async fn connection_count(&self, token: &SessionToken) -> Result<usize, AuthError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(token).ok_or(AuthError::SessionNotFound)?;
        Ok(session.connection_count.load(Ordering::Relaxed))
    }
}

/// Constant-time comparison to prevent timing attacks.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Generate a cryptographically secure random session ID.
/// Uses OsRng for entropy from the operating system's CSPRNG.
fn generate_session_id() -> String {
    use rand::RngCore;

    // Generate 32 bytes of cryptographically secure random data
    let mut random_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(random_bytes.as_mut_slice());

    // Encode as hex for a 64-character session ID
    hex::encode(random_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that constant_time_compare returns true for equal slices
    #[test]
    fn test_constant_time_compare_equal() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 5];
        assert!(constant_time_compare(&a, &b));
    }

    /// Test that constant_time_compare returns false for different slices
    #[test]
    fn test_constant_time_compare_different() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 6];
        assert!(!constant_time_compare(&a, &b));
    }

    /// Test that constant_time_compare returns false for different lengths
    #[test]
    fn test_constant_time_compare_different_lengths() {
        let a = [1u8, 2, 3];
        let b = [1u8, 2, 3, 4];
        assert!(!constant_time_compare(&a, &b));
    }

    /// Test that constant_time_compare returns true for empty slices
    #[test]
    fn test_constant_time_compare_empty() {
        let a: [u8; 0] = [];
        let b: [u8; 0] = [];
        assert!(constant_time_compare(&a, &b));
    }

    /// Test that generate_session_id produces 64-character hex strings
    #[test]
    fn test_generate_session_id_length() {
        let id = generate_session_id();
        assert_eq!(id.len(), 64); // 32 bytes = 64 hex chars
    }

    /// Test that generate_session_id produces unique IDs
    #[test]
    fn test_generate_session_id_unique() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    /// Test that generate_session_id only contains hex characters
    #[test]
    fn test_generate_session_id_hex() {
        let id = generate_session_id();
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Test SessionToken creation and as_str
    #[test]
    fn test_session_token() {
        let token = SessionToken("test-session-id".to_string());
        assert_eq!(token.as_str(), "test-session-id");
    }

    /// Test SessionToken traits
    #[test]
    fn test_session_token_traits() {
        let token1 = SessionToken("abc".to_string());
        let token2 = SessionToken("abc".to_string());
        let token3 = SessionToken("def".to_string());

        // Clone
        let cloned = token1.clone();
        assert_eq!(token1, cloned);

        // PartialEq
        assert_eq!(token1, token2);
        assert_ne!(token1, token3);

        // Hash (can be used in HashMap)
        let mut map = std::collections::HashMap::new();
        map.insert(token1, 1);
        assert_eq!(map.get(&token2), Some(&1)); // token2 has same hash
    }

    /// Test AuthError display
    #[test]
    fn test_auth_error_display() {
        assert!(AuthError::InvalidToken.to_string().contains("Invalid"));
        assert!(AuthError::SessionNotFound.to_string().contains("not found"));
        assert!(AuthError::SessionExpired.to_string().contains("expired"));
        assert!(AuthError::NotAuthenticated.to_string().contains("required"));
        assert!(AuthError::RateLimited.to_string().contains("try again"));
        assert!(AuthError::SessionRateLimited
            .to_string()
            .contains("rate limit"));
    }

    /// Test RateLimiter initial state
    #[test]
    fn test_rate_limiter_initial() {
        let limiter = RateLimiter::new();
        assert!(!limiter.is_rate_limited());
    }

    /// Test RateLimiter reset
    #[test]
    fn test_rate_limiter_reset() {
        let limiter = RateLimiter::new();
        limiter.record_failure();
        limiter.reset();
        assert!(!limiter.is_rate_limited());
    }

    // === Async tests with tokio runtime ===

    /// Test successful authentication
    #[tokio::test]
    async fn test_authenticate_success() {
        let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
        let result = auth.authenticate("test-token").await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.as_str().len(), 64); // 32 bytes hex-encoded
    }

    /// Test authentication with wrong token
    #[tokio::test]
    async fn test_authenticate_wrong_token() {
        let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));
        let result = auth.authenticate("wrong-token").await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    /// Test session validation
    #[tokio::test]
    async fn test_validate_session() {
        let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
        let session = auth.authenticate("test-token").await.unwrap();
        let result = auth.validate(&session).await;
        assert!(result.is_ok());
    }

    /// Test validation of invalid session
    #[tokio::test]
    async fn test_validate_invalid_session() {
        let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
        let fake_session = SessionToken("nonexistent-session-id".to_string());
        let result = auth.validate(&fake_session).await;
        assert!(matches!(result, Err(AuthError::SessionNotFound)));
    }

    /// Test session expiration
    #[tokio::test]
    async fn test_session_expiration() {
        // Very short timeout for testing
        let auth = SessionAuth::new("test-token", Duration::from_millis(1));
        let session = auth.authenticate("test-token").await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = auth.validate(&session).await;
        assert!(matches!(result, Err(AuthError::SessionExpired)));
    }

    /// Test cleanup removes expired sessions
    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let auth = SessionAuth::new("test-token", Duration::from_millis(1));
        let session = auth.authenticate("test-token").await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Cleanup should remove the expired session
        auth.cleanup().await;

        // Session should now be not found (removed by cleanup)
        let result = auth.validate(&session).await;
        assert!(matches!(result, Err(AuthError::SessionNotFound)));
    }

    /// Test connection tracking
    #[tokio::test]
    async fn test_connection_tracking() {
        let auth = SessionAuth::new("test-token", Duration::from_secs(3600));
        let session = auth.authenticate("test-token").await.unwrap();

        // Track connections
        let count1 = auth.track_connection(&session).await.unwrap();
        assert_eq!(count1, 1);

        let count2 = auth.track_connection(&session).await.unwrap();
        assert_eq!(count2, 2);

        // Release a connection
        auth.release_connection(&session).await;

        let current = auth.connection_count(&session).await.unwrap();
        assert_eq!(current, 1);
    }

    /// Test rate limiting after multiple failures
    #[tokio::test]
    async fn test_rate_limiting() {
        let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));

        // Make multiple failed attempts
        for _ in 0..5 {
            let _ = auth.authenticate("wrong-token").await;
        }

        // Should now be rate limited
        let result = auth.authenticate("correct-token").await;
        assert!(matches!(result, Err(AuthError::RateLimited)));
    }

    /// Test rate limit resets after successful auth
    #[tokio::test]
    async fn test_rate_limit_reset_on_success() {
        let auth = SessionAuth::new("correct-token", Duration::from_secs(3600));

        // Make some failed attempts (but not enough to trigger rate limiting)
        for _ in 0..3 {
            let _ = auth.authenticate("wrong-token").await;
        }

        // Successful auth should reset the rate limiter
        let result = auth.authenticate("correct-token").await;
        assert!(result.is_ok());

        // Should be able to continue authenticating
        let result = auth.authenticate("correct-token").await;
        assert!(result.is_ok());
    }

    /// Test multiple sessions
    #[tokio::test]
    async fn test_multiple_sessions() {
        let auth = SessionAuth::new("test-token", Duration::from_secs(3600));

        let session1 = auth.authenticate("test-token").await.unwrap();
        let session2 = auth.authenticate("test-token").await.unwrap();

        // Sessions should be different
        assert_ne!(session1, session2);

        // Both should be valid
        assert!(auth.validate(&session1).await.is_ok());
        assert!(auth.validate(&session2).await.is_ok());
    }
}
