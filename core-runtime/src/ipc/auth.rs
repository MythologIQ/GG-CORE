//! Handshake token and session ID validation.
//!
//! SECURITY: Enforces that only authenticated callers can communicate
//! with the runtime. Uses constant-time comparisons, rate limiting,
//! CSPRNG session IDs, and session timeouts.

use crate::telemetry::{log_security_event, SecurityEvent};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

use super::auth_session::{
    constant_time_compare, generate_session_id, RateLimiter, Session,
    MAX_REQUESTS_PER_MINUTE, MIN_VALIDATION_TIME_MICROS, REQUEST_WINDOW,
};

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
pub struct SessionToken(pub(super) String);

impl SessionToken {
    pub fn as_str(&self) -> &str { &self.0 }
}

/// Manages session authentication.
pub struct SessionAuth {
    sessions: Arc<RwLock<HashMap<SessionToken, Session>>>,
    expected_token_hash: [u8; 32],
    session_timeout: Duration,
    rate_limiter: RateLimiter,
}

impl SessionAuth {
    pub fn new(expected_token: &str, session_timeout: Duration) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(expected_token.as_bytes());
        let expected_token_hash: [u8; 32] = hasher.finalize().into();
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            expected_token_hash, session_timeout,
            rate_limiter: RateLimiter::new(),
        }
    }

    pub async fn authenticate(&self, token: &str) -> Result<SessionToken, AuthError> {
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

        if !constant_time_compare(&token_hash, &self.expected_token_hash) {
            self.rate_limiter.record_failure();
            log_security_event(SecurityEvent::AuthFailure, "Invalid handshake token",
                &[("reason", "invalid_token")]);
            return Err(AuthError::InvalidToken);
        }

        self.rate_limiter.reset();
        let session_id = generate_session_id();
        let session_token = SessionToken(session_id);
        let now = Instant::now();

        self.sessions.write().await.insert(session_token.clone(), Session {
            created_at: now, last_activity: now,
            connection_count: AtomicUsize::new(0),
            request_count: AtomicU64::new(0),
            request_window_start: std::sync::Mutex::new(Some(now)),
        });

        log_security_event(SecurityEvent::AuthSuccess, "Authentication successful",
            &[("session_prefix", &session_token.as_str()[..8])]);
        Ok(session_token)
    }

    pub async fn validate(&self, token: &SessionToken) -> Result<(), AuthError> {
        let start = Instant::now();
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(token).ok_or_else(|| {
            log_security_event(SecurityEvent::InvalidSession, "Invalid session token used",
                &[("session_prefix", &token.as_str()[..8])]);
            AuthError::SessionNotFound
        })?;

        if session.created_at.elapsed() > self.session_timeout {
            sessions.remove(token);
            log_security_event(SecurityEvent::SessionExpired, "Session expired",
                &[("session_prefix", &token.as_str()[..8])]);
            return Err(AuthError::SessionExpired);
        }

        let now = Instant::now();
        let should_reset = if let Ok(ws) = &session.request_window_start.lock() {
            ws.map(|s| now.duration_since(s) > REQUEST_WINDOW).unwrap_or(true)
        } else { false };

        if should_reset {
            session.request_count.store(1, Ordering::SeqCst);
            if let Ok(mut ws) = session.request_window_start.lock() { *ws = Some(now); }
        } else {
            let count = session.request_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count > MAX_REQUESTS_PER_MINUTE {
                log_security_event(SecurityEvent::RateLimited,
                    "Session request rate limit exceeded",
                    &[("session_prefix", &token.as_str()[..8]),
                      ("request_count", &count.to_string())]);
                return Err(AuthError::SessionRateLimited);
            }
        }

        session.last_activity = Instant::now();
        let elapsed = start.elapsed().as_micros() as u64;
        if elapsed < MIN_VALIDATION_TIME_MICROS {
            std::thread::sleep(Duration::from_micros(MIN_VALIDATION_TIME_MICROS - elapsed));
        }
        Ok(())
    }

    pub async fn cleanup(&self) {
        let timeout = self.session_timeout;
        self.sessions.write().await.retain(|_, s| s.created_at.elapsed() <= timeout);
    }

    pub async fn track_connection(&self, token: &SessionToken) -> Result<usize, AuthError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(token).ok_or(AuthError::SessionNotFound)?;
        Ok(session.connection_count.fetch_add(1, Ordering::SeqCst) + 1)
    }

    pub async fn release_connection(&self, token: &SessionToken) {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(token) {
            session.connection_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub async fn connection_count(&self, token: &SessionToken) -> Result<usize, AuthError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(token).ok_or(AuthError::SessionNotFound)?;
        Ok(session.connection_count.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
