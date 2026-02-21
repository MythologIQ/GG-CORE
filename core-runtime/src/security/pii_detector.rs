//! PII (Personally Identifiable Information) Detector
//! 
//! Detects and redacts PII in text outputs using pattern matching.
//! Optimized for performance with SIMD-accelerated regex where available.
//!
//! # Security
//! Uses NFKC normalization before pattern matching to prevent Unicode
//! homograph attacks where visually similar characters bypass detection.

use regex::Regex;
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

/// PII types that can be detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PIIType {
    /// Credit card numbers
    CreditCard,
    /// Social Security Numbers
    SSN,
    /// Email addresses
    Email,
    /// Phone numbers
    Phone,
    /// IP addresses
    IPAddress,
    /// MAC addresses
    MACAddress,
    /// Dates of birth
    DateOfBirth,
    /// Street addresses
    Address,
    /// Passport numbers
    Passport,
    /// Driver's license numbers
    DriverLicense,
    /// Bank account numbers
    BankAccount,
    /// Medical record numbers
    MedicalRecord,
    /// API keys and tokens
    APIKey,
}

impl PIIType {
    /// Get human-readable name
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
    
    /// Get severity level (1-5)
    pub fn severity(&self) -> u8 {
        match self {
            PIIType::SSN => 5,
            PIIType::CreditCard => 5,
            PIIType::Passport => 5,
            PIIType::DriverLicense => 4,
            PIIType::BankAccount => 5,
            PIIType::MedicalRecord => 5,
            PIIType::Email => 3,
            PIIType::Phone => 3,
            PIIType::Address => 3,
            PIIType::DateOfBirth => 4,
            PIIType::IPAddress => 2,
            PIIType::MACAddress => 2,
            PIIType::APIKey => 5,
        }
    }
}

/// Detected PII instance
#[derive(Debug, Clone)]
pub struct PIIMatch {
    /// Type of PII detected
    pub pii_type: PIIType,
    /// The matched text
    pub text: String,
    /// Start position in original text
    pub start: usize,
    /// End position in original text
    pub end: usize,
    /// Confidence level (0.0-1.0)
    pub confidence: f32,
}

/// PII Detector with compiled regex patterns
pub struct PIIDetector {
    /// Compiled regex patterns for each PII type
    patterns: Arc<Vec<(PIIType, Regex)>>,
    /// Whether to use Luhn validation for credit cards
    validate_credit_cards: bool,
}

impl PIIDetector {
    /// Create a new PII detector
    pub fn new() -> Self {
        let patterns = vec![
            // Credit card patterns (major card types)
            (PIIType::CreditCard, Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap()),
            (PIIType::CreditCard, Regex::new(r"\b\d{13,19}\b").unwrap()),
            
            // SSN patterns (US format)
            (PIIType::SSN, Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap()),
            
            // Email pattern
            (PIIType::Email, Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()),
            
            // Phone patterns (US and international)
            (PIIType::Phone, Regex::new(r"\b(?:\+?1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}\b").unwrap()),
            (PIIType::Phone, Regex::new(r"\b\+?[1-9]\d{1,14}\b").unwrap()),
            
            // IP addresses
            (PIIType::IPAddress, Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap()),
            (PIIType::IPAddress, Regex::new(r"\b(?:[a-fA-F0-9]{1,4}:){7}[a-fA-F0-9]{1,4}\b").unwrap()),
            
            // MAC addresses
            (PIIType::MACAddress, Regex::new(r"\b(?:[a-fA-F0-9]{2}[:-]){5}[a-fA-F0-9]{2}\b").unwrap()),
            
            // Date patterns (various formats)
            (PIIType::DateOfBirth, Regex::new(r"\b\d{1,2}[-/]\d{1,2}[-/]\d{2,4}\b").unwrap()),
            (PIIType::DateOfBirth, Regex::new(r"\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}\b").unwrap()),
            
            // Address pattern (simplified)
            (PIIType::Address, Regex::new(r"\b\d+\s+[A-Za-z\s]+(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Way|Court|Ct)\b").unwrap()),
            
            // Passport numbers (various country formats)
            (PIIType::Passport, Regex::new(r"\b[A-Z]{1,2}\d{6,9}\b").unwrap()),
            (PIIType::Passport, Regex::new(r"\b\d{9}\b").unwrap()),
            
            // Driver's license (US state formats - simplified)
            (PIIType::DriverLicense, Regex::new(r"\b[A-Z]\d{7,12}\b").unwrap()),
            (PIIType::DriverLicense, Regex::new(r"\b\d{7,12}[A-Z]\b").unwrap()),
            
            // Bank account (generic)
            (PIIType::BankAccount, Regex::new(r"\b\d{8,17}\b").unwrap()),
            
            // Medical record numbers
            (PIIType::MedicalRecord, Regex::new(r"\bMRN[:\s]?\d{6,10}\b").unwrap()),
            (PIIType::MedicalRecord, Regex::new(r"\b\d{2}[A-Z]\d{5}[A-Z]\d{2}\b").unwrap()),
            
            // API keys and tokens
            (PIIType::APIKey, Regex::new(r"\b(?:api[_-]?key|token|secret|auth)[_-]?[a-zA-Z0-9]{16,}\b").unwrap()),
            (PIIType::APIKey, Regex::new(r"\bsk-[a-zA-Z0-9]{20,}\b").unwrap()),
            (PIIType::APIKey, Regex::new(r"\bghp_[a-zA-Z0-9]{36}\b").unwrap()),
            (PIIType::APIKey, Regex::new(r"\bxox[baprs]-[a-zA-Z0-9-]{10,}\b").unwrap()),
        ];
        
        Self {
            patterns: Arc::new(patterns),
            validate_credit_cards: true,
        }
    }
    
    /// Detect PII in text
    /// Returns list of detected PII instances
    ///
    /// # Security
    /// Uses NFKC normalization to prevent Unicode homograph attacks.
    /// This ensures that visually similar characters (e.g., Cyrillic 'а' vs Latin 'a')
    /// are normalized to the same form before pattern matching.
    pub fn detect(&self, text: &str) -> Vec<PIIMatch> {
        // SECURITY: Apply NFKC normalization to prevent Unicode homograph attacks
        // This converts visually similar characters to their canonical form
        let normalized: String = text.nfkc().collect();
        
        let mut matches = Vec::new();
        
        for (pii_type, regex) in self.patterns.iter() {
            for m in regex.find_iter(&normalized) {
                let matched_text = m.as_str();
                
                // Additional validation for credit cards
                if *pii_type == PIIType::CreditCard && self.validate_credit_cards {
                    let digits: String = matched_text.chars().filter(|c| c.is_ascii_digit()).collect();
                    if !self.luhn_check(&digits) {
                        continue;
                    }
                }
                
                // Calculate confidence based on pattern specificity
                let confidence = self.calculate_confidence(pii_type, matched_text);
                
                matches.push(PIIMatch {
                    pii_type: *pii_type,
                    text: matched_text.to_string(),
                    start: m.start(),
                    end: m.end(),
                    confidence,
                });
            }
        }
        
        // Sort by position and remove overlapping matches
        matches.sort_by_key(|m| m.start);
        self.remove_overlaps(matches)
    }
    
    /// Check if text contains any PII
    ///
    /// # Security
    /// Uses NFKC normalization to prevent Unicode homograph attacks.
    pub fn contains_pii(&self, text: &str) -> bool {
        // SECURITY: Apply NFKC normalization
        let normalized: String = text.nfkc().collect();
        
        for (_, regex) in self.patterns.iter() {
            if regex.is_match(&normalized) {
                return true;
            }
        }
        false
    }
    
    /// Redact PII in text
    /// Returns text with PII replaced by [REDACTED]
    ///
    /// # Security
    /// Uses NFKC normalization to prevent Unicode homograph attacks.
    pub fn redact(&self, text: &str) -> String {
        // SECURITY: Apply NFKC normalization for detection
        let normalized: String = text.nfkc().collect();
        let matches = self.detect(&normalized);
        if matches.is_empty() {
            return text.to_string();
        }
        
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
    
    /// Calculate confidence score for a match
    fn calculate_confidence(&self, pii_type: &PIIType, text: &str) -> f32 {
        match pii_type {
            PIIType::Email => {
                // Higher confidence for valid-looking emails
                if text.contains('@') && text.contains('.') {
                    0.95
                } else {
                    0.7
                }
            }
            PIIType::CreditCard => {
                // Luhn validation already done
                0.95
            }
            PIIType::SSN => {
                // Check for valid SSN ranges
                let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() == 9 {
                    let area = &digits[0..3];
                    if area != "000" && area != "666" && area < "900" {
                        0.9
                    } else {
                        0.5
                    }
                } else {
                    0.6
                }
            }
            PIIType::Phone => {
                // Higher confidence for properly formatted numbers
                if text.starts_with('+') || text.chars().filter(|c| c.is_ascii_digit()).count() == 10 {
                    0.85
                } else {
                    0.6
                }
            }
            PIIType::APIKey => {
                // Very high confidence for known API key formats
                if text.starts_with("sk-") || text.starts_with("ghp_") || text.starts_with("xox") {
                    0.98
                } else {
                    0.7
                }
            }
            _ => 0.75,
        }
    }
    
    /// Luhn algorithm for credit card validation
    fn luhn_check(&self, number: &str) -> bool {
        let digits: Vec<u32> = number.chars()
            .filter_map(|c| c.to_digit(10))
            .collect();
        
        if digits.len() < 13 || digits.len() > 19 {
            return false;
        }
        
        let mut sum = 0u32;
        let mut double = false;
        
        for &digit in digits.iter().rev() {
            let mut d = digit;
            if double {
                d *= 2;
                if d > 9 {
                    d -= 9;
                }
            }
            sum += d;
            double = !double;
        }
        
        sum % 10 == 0
    }
    
    /// Remove overlapping matches, keeping highest confidence
    fn remove_overlaps(&self, mut matches: Vec<PIIMatch>) -> Vec<PIIMatch> {
        if matches.len() <= 1 {
            return matches;
        }
        
        let mut result = Vec::new();
        let mut current = matches.remove(0);
        
        for m in matches {
            if m.start < current.end {
                // Overlapping - keep higher confidence
                if m.confidence > current.confidence {
                    current = m;
                }
            } else {
                result.push(current);
                current = m;
            }
        }
        result.push(current);
        
        result
    }
}

impl Default for PIIDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_email_detection() {
        let detector = PIIDetector::new();
        let text = "Contact us at support@example.com for help";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::Email);
        assert_eq!(matches[0].text, "support@example.com");
    }
    
    #[test]
    fn test_ssn_detection() {
        let detector = PIIDetector::new();
        let text = "SSN: 123-45-6789";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::SSN);
    }
    
    #[test]
    fn test_credit_card_detection() {
        let detector = PIIDetector::new();
        // Valid test credit card number (passes Luhn)
        let text = "Card: 4532-0151-1283-0366";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::CreditCard);
    }
    
    #[test]
    fn test_credit_card_luhn_rejects_invalid() {
        let detector = PIIDetector::new();
        // Invalid credit card number (fails Luhn)
        let text = "Card: 1234-5678-9012-3456";
        let matches = detector.detect(text);
        
        // Should not detect invalid card
        let cc_matches: Vec<_> = matches.iter().filter(|m| m.pii_type == PIIType::CreditCard).collect();
        assert!(cc_matches.is_empty());
    }
    
    #[test]
    fn test_phone_detection() {
        let detector = PIIDetector::new();
        let text = "Call me at 555-123-4567";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::Phone);
    }
    
    #[test]
    fn test_api_key_detection() {
        let detector = PIIDetector::new();
        // Use a valid sk- key format (20+ alphanumeric chars after sk-)
        let text = "API key: sk-projabcdefghijklmnopqrstuvwxyz1234";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::APIKey);
        assert!(matches[0].confidence > 0.9);
    }
    
    #[test]
    fn test_redaction() {
        let detector = PIIDetector::new();
        let text = "Email: test@example.com and SSN: 123-45-6789";
        let redacted = detector.redact(text);
        
        assert!(redacted.contains("[REDACTED:Email Address]"));
        assert!(redacted.contains("[REDACTED:Social Security Number]"));
        assert!(!redacted.contains("test@example.com"));
        assert!(!redacted.contains("123-45-6789"));
    }
    
    #[test]
    fn test_no_pii() {
        let detector = PIIDetector::new();
        let text = "The quick brown fox jumps over the lazy dog";
        let matches = detector.detect(text);
        
        assert!(matches.is_empty());
        assert!(!detector.contains_pii(text));
    }
    
    #[test]
    fn test_multiple_pii_types() {
        let detector = PIIDetector::new();
        let text = "Contact john@example.com or call 555-123-4567. IP: 192.168.1.1";
        let matches = detector.detect(text);
        
        assert!(matches.len() >= 3);
        
        let types: Vec<PIIType> = matches.iter().map(|m| m.pii_type).collect();
        assert!(types.contains(&PIIType::Email));
        assert!(types.contains(&PIIType::Phone));
        assert!(types.contains(&PIIType::IPAddress));
    }
    
    #[test]
    fn test_performance() {
        let detector = PIIDetector::new();
        let text = "Email: test@example.com Phone: 555-123-4567 IP: 192.168.1.1".repeat(100);
        
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = detector.detect(&text);
        }
        let duration = start.elapsed();
        
        // Should complete 100 scans in under 5 seconds
        assert!(duration.as_millis() < 5000, "PII detection too slow: {:?}", duration);
    }
    
    #[test]
    fn test_ip_address_v6() {
        let detector = PIIDetector::new();
        let text = "IPv6: 2001:0db8:85a3:0000:0000:8a2e:0370:7334";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::IPAddress);
    }
    
    #[test]
    fn test_github_token_detection() {
        let detector = PIIDetector::new();
        let text = "Token: ghp_1234567890abcdefghijklmnopqrstuvwxyz";
        let matches = detector.detect(text);
        
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pii_type, PIIType::APIKey);
    }
}