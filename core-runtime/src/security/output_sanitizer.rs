//! Output Sanitizer
//!
//! Sanitizes model outputs for security and safety.
//! Combines PII detection, content filtering, and format validation.

use crate::security::{PIIDetector, pii_detector::PIIType};
use std::sync::Arc;

use super::sanitizer_rules;

/// Output sanitizer configuration
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    pub redact_pii: bool,
    pub filter_content: bool,
    pub max_length: usize,
    pub pii_confidence_threshold: f32,
    pub redact_types: Vec<PIIType>,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            redact_pii: true,
            filter_content: true,
            max_length: 100_000,
            pii_confidence_threshold: 0.7,
            redact_types: vec![
                PIIType::SSN, PIIType::CreditCard, PIIType::Email,
                PIIType::Phone, PIIType::APIKey, PIIType::Passport,
                PIIType::BankAccount, PIIType::MedicalRecord,
            ],
        }
    }
}

/// Sanitization result
#[derive(Debug, Clone)]
pub struct SanitizationResult {
    pub output: String,
    pub modified: bool,
    pub pii_redacted: usize,
    pub content_filtered: usize,
    pub warnings: Vec<String>,
}

/// Output sanitizer
pub struct OutputSanitizer {
    pii_detector: Arc<PIIDetector>,
    config: SanitizerConfig,
}

impl OutputSanitizer {
    pub fn new(config: SanitizerConfig) -> Self {
        Self { pii_detector: Arc::new(PIIDetector::new()), config }
    }

    pub fn default_sanitizer() -> Self { Self::new(SanitizerConfig::default()) }

    pub fn sanitize(&self, output: &str) -> SanitizationResult {
        let mut result = output.to_string();
        let mut modified = false;
        let mut pii_redacted = 0;
        let mut warnings = Vec::new();

        if result.len() > self.config.max_length {
            result.truncate(self.config.max_length);
            warnings.push(format!("Output truncated to {} characters", self.config.max_length));
            modified = true;
        }

        if self.config.redact_pii {
            let matches = self.pii_detector.detect(&result);
            for m in matches {
                if !self.config.redact_types.contains(&m.pii_type) { continue; }
                if m.confidence < self.config.pii_confidence_threshold { continue; }
                result = self.redact_pii(&result, &m);
                pii_redacted += 1;
                modified = true;
            }
        }

        let mut content_filtered = 0;
        if self.config.filter_content {
            let (filtered, count) = sanitizer_rules::filter_content_patterns(&result);
            if count > 0 { result = filtered; content_filtered = count; modified = true; }
        }

        SanitizationResult { output: result, modified, pii_redacted, content_filtered, warnings }
    }

    pub fn sanitize_chunk(&self, chunk: &str, state: &mut StreamingSanitizerState) -> String {
        let mut result = chunk.to_string();
        state.buffer.push_str(chunk);

        if self.config.redact_pii {
            let matches = self.pii_detector.detect(&state.buffer);
            for m in matches {
                if m.start >= state.processed_until && m.end <= state.buffer.len() {
                    let redacted = format!("[REDACTED:{}]", m.pii_type.name());
                    let cs = m.start.saturating_sub(state.processed_until);
                    let ce = m.end.saturating_sub(state.processed_until);
                    if cs < result.len() && ce <= result.len() {
                        result.replace_range(cs..ce, &redacted);
                    }
                    state.processed_until = m.end;
                }
            }
        }

        if state.buffer.len() > 1000 {
            let max_trim = state.buffer.len() - 500;
            let safe_trim = sanitizer_rules::find_safe_trim_point(&state.buffer, max_trim);
            if safe_trim > 0 {
                state.buffer.drain(0..safe_trim);
                state.processed_until = state.processed_until.saturating_sub(safe_trim);
            }
        }

        result
    }

    fn redact_pii(&self, text: &str, m: &crate::security::pii_detector::PIIMatch) -> String {
        let mut result = text.to_string();
        let replacement = format!("[REDACTED:{}]", m.pii_type.name());
        result.replace_range(m.start..m.end, &replacement);
        result
    }

    pub fn validate_format(&self, output: &str) -> Result<(), String> {
        sanitizer_rules::validate_format(output)
    }

    pub fn has_excessive_repetition(&self, text: &str) -> bool {
        sanitizer_rules::has_excessive_repetition(text)
    }
}

/// State for streaming sanitization
pub struct StreamingSanitizerState {
    pub(crate) buffer: String,
    pub(crate) processed_until: usize,
}

impl Default for StreamingSanitizerState {
    fn default() -> Self { Self { buffer: String::new(), processed_until: 0 } }
}

impl Default for OutputSanitizer {
    fn default() -> Self { Self::default_sanitizer() }
}

#[cfg(test)]
#[path = "sanitizer_tests.rs"]
mod tests;
