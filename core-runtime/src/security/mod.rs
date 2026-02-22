//! Security module for COREFORGE CORE
//!
//! This module provides comprehensive security features including:
//! - Prompt injection protection
//! - Output sanitization and PII detection
//! - Model file encryption with key rotation (SOC2-2)
//! - FIPS 140-3 self-tests (FIPS-3)
//! - Secure communication
//! - Enterprise audit logging

pub mod audit;
pub mod audit_types;
pub mod encryption_core;
pub mod encryption_io;
pub mod encryption_key;
pub mod encryption;
pub mod fips_tests;
pub mod key_rotation;
pub mod output_sanitizer;
pub mod pii_detector;
pub mod pii_patterns;
pub mod prompt_injection;
pub mod sanitizer_rules;

pub use audit::{AuditCategory, AuditEvent, AuditLogger, AuditSeverity};
pub use encryption::ModelEncryption;
pub use fips_tests::{run_power_on_self_tests, SelfTestError, SelfTestResults};
pub use key_rotation::{KeyRotationError, KeyRotationManager};
pub use output_sanitizer::OutputSanitizer;
pub use pii_detector::{PIIDetector, PIIMatch};
pub use prompt_injection::{InjectionMatch, PromptInjectionFilter};

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable prompt injection detection
    pub enable_prompt_injection_detection: bool,
    /// Block requests with detected prompt injection
    pub block_prompt_injection: bool,
    /// Enable PII detection in outputs
    pub enable_pii_detection: bool,
    /// Redact PII in outputs
    pub redact_pii: bool,
    /// Enable model encryption
    pub enable_model_encryption: bool,
    /// Encryption key (if None, generates from machine ID)
    pub encryption_key: Option<[u8; 32]>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_prompt_injection_detection: true,
            block_prompt_injection: true,
            enable_pii_detection: true,
            redact_pii: true,
            enable_model_encryption: false,
            encryption_key: None,
        }
    }
}

/// Security scan result
#[derive(Debug, Clone)]
pub struct SecurityScanResult {
    /// Whether the content passed security checks
    pub passed: bool,
    /// Detected issues
    pub issues: Vec<SecurityIssue>,
    /// Risk score (0-100)
    pub risk_score: u8,
}

/// Security issue type
#[derive(Debug, Clone)]
pub struct SecurityIssue {
    /// Issue type
    pub issue_type: SecurityIssueType,
    /// Severity (1-5)
    pub severity: u8,
    /// Description
    pub description: String,
    /// Location in text (byte offset)
    pub location: Option<usize>,
}

/// Types of security issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityIssueType {
    /// Prompt injection attempt
    PromptInjection,
    /// PII detected
    PII,
    /// Sensitive data exposure
    SensitiveData,
    /// Malformed input
    MalformedInput,
    /// Suspicious pattern
    SuspiciousPattern,
}
