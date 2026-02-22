//! Tests for PII detection.

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
    let text = "Card: 4532-0151-1283-0366";
    let matches = detector.detect(text);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].pii_type, PIIType::CreditCard);
}

#[test]
fn test_credit_card_luhn_rejects_invalid() {
    let detector = PIIDetector::new();
    let text = "Card: 1234-5678-9012-3456";
    let matches = detector.detect(text);
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
    for _ in 0..100 { let _ = detector.detect(&text); }
    let duration = start.elapsed();
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
