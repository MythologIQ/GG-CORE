//! PII (Personally Identifiable Information) Detector
//!
//! Detects and redacts PII in text outputs using pattern matching.
//!
//! # Security
//! Uses NFKC normalization before pattern matching to prevent Unicode
//! homograph attacks where visually similar characters bypass detection.

use regex::Regex;
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

use super::pii_patterns;

/// PII types that can be detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PIIType {
    CreditCard, SSN, Email, Phone, IPAddress, MACAddress,
    DateOfBirth, Address, Passport, DriverLicense,
    BankAccount, MedicalRecord, APIKey,
}

impl PIIType {
    pub fn name(&self) -> &'static str {
        match self {
            PIIType::CreditCard => "Credit Card",
            PIIType::SSN => "Social Security Number",
            PIIType::Email => "Email Address",
            PIIType::Phone => "Phone Number",
            PIIType::IPAddress => "IP Address",
            PIIType::MACAddress => "MAC Address",
            PIIType::DateOfBirth => "Date of Birth",
            PIIType::Address => "Street Address",
            PIIType::Passport => "Passport Number",
            PIIType::DriverLicense => "Driver's License",
            PIIType::BankAccount => "Bank Account",
            PIIType::MedicalRecord => "Medical Record",
            PIIType::APIKey => "API Key",
        }
    }

    pub fn severity(&self) -> u8 {
        match self {
            PIIType::SSN | PIIType::CreditCard | PIIType::Passport
            | PIIType::BankAccount | PIIType::MedicalRecord | PIIType::APIKey => 5,
            PIIType::DriverLicense | PIIType::DateOfBirth => 4,
            PIIType::Email | PIIType::Phone | PIIType::Address => 3,
            PIIType::IPAddress | PIIType::MACAddress => 2,
        }
    }
}

/// Detected PII instance
#[derive(Debug, Clone)]
pub struct PIIMatch {
    pub pii_type: PIIType,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f32,
}

/// PII Detector with compiled regex patterns
pub struct PIIDetector {
    patterns: Arc<Vec<(PIIType, Regex)>>,
    validate_credit_cards: bool,
}

impl PIIDetector {
    pub fn new() -> Self {
        Self {
            patterns: Arc::new(pii_patterns::build_patterns()),
            validate_credit_cards: true,
        }
    }

    /// Detect PII in text. Uses NFKC normalization to prevent homograph attacks.
    pub fn detect(&self, text: &str) -> Vec<PIIMatch> {
        let normalized: String = text.nfkc().collect();
        let mut matches = Vec::new();

        for (pii_type, regex) in self.patterns.iter() {
            for m in regex.find_iter(&normalized) {
                let matched_text = m.as_str();
                if *pii_type == PIIType::CreditCard && self.validate_credit_cards {
                    let digits: String = matched_text.chars().filter(|c| c.is_ascii_digit()).collect();
                    if !pii_patterns::luhn_check(&digits) { continue; }
                }
                let confidence = pii_patterns::calculate_confidence(pii_type, matched_text);
                matches.push(PIIMatch {
                    pii_type: *pii_type, text: matched_text.to_string(),
                    start: m.start(), end: m.end(), confidence,
                });
            }
        }

        matches.sort_by_key(|m| m.start);
        pii_patterns::remove_overlaps(matches)
    }

    /// Check if text contains any PII. Uses NFKC normalization.
    pub fn contains_pii(&self, text: &str) -> bool {
        let normalized: String = text.nfkc().collect();
        self.patterns.iter().any(|(_, regex)| regex.is_match(&normalized))
    }

    /// Redact PII in text. Uses NFKC normalization.
    pub fn redact(&self, text: &str) -> String {
        let normalized: String = text.nfkc().collect();
        let matches = self.detect(&normalized);
        if matches.is_empty() { return text.to_string(); }

        let mut result = normalized;
        let mut offset = 0isize;
        for m in matches {
            let start = (m.start as isize + offset) as usize;
            let end = (m.end as isize + offset) as usize;
            if start < result.len() && end <= result.len() {
                let replacement = format!("[REDACTED:{}]", m.pii_type.name());
                result.replace_range(start..end, &replacement);
                offset += replacement.len() as isize - (m.end - m.start) as isize;
            }
        }
        result
    }
}

impl Default for PIIDetector {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[path = "pii_tests.rs"]
mod tests;
