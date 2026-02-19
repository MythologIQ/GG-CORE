//! Security audit logging for CORE Runtime.
//!
//! SECURITY: This module provides structured logging for security-relevant events
//! to enable forensic analysis and intrusion detection.

use std::time::{SystemTime, UNIX_EPOCH};

/// Security event types for audit logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityEvent {
    /// Successful authentication.
    AuthSuccess,
    /// Failed authentication attempt.
    AuthFailure,
    /// Rate limiting triggered.
    RateLimited,
    /// Session created.
    SessionCreated,
    /// Session expired.
    SessionExpired,
    /// Session validated.
    SessionValidated,
    /// Invalid session token used.
    InvalidSession,
    /// Path traversal attempt detected.
    PathTraversalAttempt,
    /// Input validation failure.
    InputValidationFailure,
    /// Output filter triggered.
    OutputFiltered,
    /// Resource limit exceeded.
    ResourceLimitExceeded,
    /// Model hash verification failure.
    ModelHashMismatch,
    /// Sandbox violation attempt.
    SandboxViolation,
}

impl SecurityEvent {
    /// Get the severity level for this event.
    pub fn severity(&self) -> SecuritySeverity {
        match self {
            Self::AuthSuccess => SecuritySeverity::Info,
            Self::AuthFailure => SecuritySeverity::Warning,
            Self::RateLimited => SecuritySeverity::Warning,
            Self::SessionCreated => SecuritySeverity::Info,
            Self::SessionExpired => SecuritySeverity::Info,
            Self::SessionValidated => SecuritySeverity::Debug,
            Self::InvalidSession => SecuritySeverity::Warning,
            Self::PathTraversalAttempt => SecuritySeverity::Critical,
            Self::InputValidationFailure => SecuritySeverity::Warning,
            Self::OutputFiltered => SecuritySeverity::Info,
            Self::ResourceLimitExceeded => SecuritySeverity::Warning,
            Self::ModelHashMismatch => SecuritySeverity::Critical,
            Self::SandboxViolation => SecuritySeverity::Critical,
        }
    }

    /// Get a string representation of the event type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthSuccess => "auth_success",
            Self::AuthFailure => "auth_failure",
            Self::RateLimited => "rate_limited",
            Self::SessionCreated => "session_created",
            Self::SessionExpired => "session_expired",
            Self::SessionValidated => "session_validated",
            Self::InvalidSession => "invalid_session",
            Self::PathTraversalAttempt => "path_traversal_attempt",
            Self::InputValidationFailure => "input_validation_failure",
            Self::OutputFiltered => "output_filtered",
            Self::ResourceLimitExceeded => "resource_limit_exceeded",
            Self::ModelHashMismatch => "model_hash_mismatch",
            Self::SandboxViolation => "sandbox_violation",
        }
    }
}

/// Severity levels for security events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl SecuritySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Log a security event with structured data.
///
/// # Arguments
/// * `event` - The type of security event
/// * `message` - Human-readable description
/// * `details` - Additional structured details as key-value pairs
///
/// # Example
/// ```
/// use veritas_sdr::telemetry::{log_security_event, SecurityEvent};
///
/// log_security_event(
///     SecurityEvent::AuthFailure,
///     "Invalid handshake token",
///     &[("source", "ipc"), ("attempt_count", "3")]
/// );
/// ```
pub fn log_security_event(event: SecurityEvent, message: &str, details: &[(&str, &str)]) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let event_type = event.as_str();
    let severity = event.severity();

    // Build structured log message
    let details_str = details
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(" ");

    // Format as structured log line
    let log_line = if details_str.is_empty() {
        format!(
            "[{}] SECURITY {} {}: {}",
            timestamp,
            severity.as_str(),
            event_type,
            message
        )
    } else {
        format!(
            "[{}] SECURITY {} {}: {} | {}",
            timestamp,
            severity.as_str(),
            event_type,
            message,
            details_str
        )
    };

    // Log to appropriate level using tracing
    match severity {
        SecuritySeverity::Debug => tracing::debug!("{}", log_line),
        SecuritySeverity::Info => tracing::info!("{}", log_line),
        SecuritySeverity::Warning => tracing::warn!("{}", log_line),
        SecuritySeverity::Error => tracing::error!("{}", log_line),
        SecuritySeverity::Critical => tracing::error!("🚨 {}", log_line),
    }
}

/// Convenience macro for logging security events.
#[macro_export]
macro_rules! security_log {
    ($event:expr, $message:expr) => {
        $crate::telemetry::security_log::log_security_event($event, $message, &[])
    };
    ($event:expr, $message:expr, $($key:expr => $value:expr),+) => {
        $crate::telemetry::security_log::log_security_event(
            $event,
            $message,
            &[$(($key, $value)),+]
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_severity() {
        assert_eq!(
            SecurityEvent::AuthSuccess.severity(),
            SecuritySeverity::Info
        );
        assert_eq!(
            SecurityEvent::AuthFailure.severity(),
            SecuritySeverity::Warning
        );
        assert_eq!(
            SecurityEvent::PathTraversalAttempt.severity(),
            SecuritySeverity::Critical
        );
    }

    #[test]
    fn test_event_as_str() {
        assert_eq!(SecurityEvent::AuthSuccess.as_str(), "auth_success");
        assert_eq!(
            SecurityEvent::PathTraversalAttempt.as_str(),
            "path_traversal_attempt"
        );
    }

    #[test]
    fn test_severity_ordering() {
        assert!(SecuritySeverity::Critical > SecuritySeverity::Error);
        assert!(SecuritySeverity::Error > SecuritySeverity::Warning);
        assert!(SecuritySeverity::Warning > SecuritySeverity::Info);
        assert!(SecuritySeverity::Info > SecuritySeverity::Debug);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(SecuritySeverity::Debug.as_str(), "DEBUG");
        assert_eq!(SecuritySeverity::Info.as_str(), "INFO");
        assert_eq!(SecuritySeverity::Warning.as_str(), "WARNING");
        assert_eq!(SecuritySeverity::Error.as_str(), "ERROR");
        assert_eq!(SecuritySeverity::Critical.as_str(), "CRITICAL");
    }
}
