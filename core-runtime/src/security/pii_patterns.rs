//! PII detection patterns and confidence calculation.

use regex::Regex;
use super::pii_detector::{PIIType, PIIMatch};

/// Build the compiled regex patterns for all PII types.
pub fn build_patterns() -> Vec<(PIIType, Regex)> {
    vec![
        (PIIType::CreditCard, Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap()),
        (PIIType::CreditCard, Regex::new(r"\b\d{13,19}\b").unwrap()),
        (PIIType::SSN, Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap()),
        (PIIType::Email, Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()),
        (PIIType::Phone, Regex::new(r"\b(?:\+?1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}\b").unwrap()),
        (PIIType::Phone, Regex::new(r"\b\+?[1-9]\d{1,14}\b").unwrap()),
        (PIIType::IPAddress, Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap()),
        (PIIType::IPAddress, Regex::new(r"\b(?:[a-fA-F0-9]{1,4}:){7}[a-fA-F0-9]{1,4}\b").unwrap()),
        (PIIType::MACAddress, Regex::new(r"\b(?:[a-fA-F0-9]{2}[:-]){5}[a-fA-F0-9]{2}\b").unwrap()),
        (PIIType::DateOfBirth, Regex::new(r"\b\d{1,2}[-/]\d{1,2}[-/]\d{2,4}\b").unwrap()),
        (PIIType::DateOfBirth, Regex::new(r"\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}\b").unwrap()),
        (PIIType::Address, Regex::new(r"\b\d+\s+[A-Za-z\s]+(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Way|Court|Ct)\b").unwrap()),
        (PIIType::Passport, Regex::new(r"\b[A-Z]{1,2}\d{6,9}\b").unwrap()),
        (PIIType::Passport, Regex::new(r"\b\d{9}\b").unwrap()),
        (PIIType::DriverLicense, Regex::new(r"\b[A-Z]\d{7,12}\b").unwrap()),
        (PIIType::DriverLicense, Regex::new(r"\b\d{7,12}[A-Z]\b").unwrap()),
        (PIIType::BankAccount, Regex::new(r"\b\d{8,17}\b").unwrap()),
        (PIIType::MedicalRecord, Regex::new(r"\bMRN[:\s]?\d{6,10}\b").unwrap()),
        (PIIType::MedicalRecord, Regex::new(r"\b\d{2}[A-Z]\d{5}[A-Z]\d{2}\b").unwrap()),
        (PIIType::APIKey, Regex::new(r"\b(?:api[_-]?key|token|secret|auth)[_-]?[a-zA-Z0-9]{16,}\b").unwrap()),
        (PIIType::APIKey, Regex::new(r"\bsk-[a-zA-Z0-9]{20,}\b").unwrap()),
        (PIIType::APIKey, Regex::new(r"\bghp_[a-zA-Z0-9]{36}\b").unwrap()),
        (PIIType::APIKey, Regex::new(r"\bxox[baprs]-[a-zA-Z0-9-]{10,}\b").unwrap()),
    ]
}

/// Calculate confidence score for a match.
pub fn calculate_confidence(pii_type: &PIIType, text: &str) -> f32 {
    match pii_type {
        PIIType::Email => {
            if text.contains('@') && text.contains('.') { 0.95 } else { 0.7 }
        }
        PIIType::CreditCard => 0.95,
        PIIType::SSN => {
            let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() == 9 {
                let area = &digits[0..3];
                if area != "000" && area != "666" && area < "900" { 0.9 } else { 0.5 }
            } else { 0.6 }
        }
        PIIType::Phone => {
            if text.starts_with('+') || text.chars().filter(|c| c.is_ascii_digit()).count() == 10 {
                0.85
            } else { 0.6 }
        }
        PIIType::APIKey => {
            if text.starts_with("sk-") || text.starts_with("ghp_") || text.starts_with("xox") {
                0.98
            } else { 0.7 }
        }
        _ => 0.75,
    }
}

/// Luhn algorithm for credit card validation.
pub fn luhn_check(number: &str) -> bool {
    let digits: Vec<u32> = number.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 || digits.len() > 19 { return false; }
    let mut sum = 0u32;
    let mut double = false;
    for &digit in digits.iter().rev() {
        let mut d = digit;
        if double { d *= 2; if d > 9 { d -= 9; } }
        sum += d;
        double = !double;
    }
    sum % 10 == 0
}

/// Remove overlapping matches, keeping highest confidence.
pub fn remove_overlaps(mut matches: Vec<PIIMatch>) -> Vec<PIIMatch> {
    if matches.len() <= 1 { return matches; }
    let mut result = Vec::new();
    let mut current = matches.remove(0);
    for m in matches {
        if m.start < current.end {
            if m.confidence > current.confidence { current = m; }
        } else {
            result.push(current);
            current = m;
        }
    }
    result.push(current);
    result
}
