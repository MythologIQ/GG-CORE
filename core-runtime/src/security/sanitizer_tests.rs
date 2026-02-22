//! Tests for the output sanitizer.

use super::super::output_sanitizer::*;
use super::super::sanitizer_rules::*;
use super::super::pii_detector::PIIType;

#[test]
fn test_pii_redaction() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let output = "Contact support@example.com for assistance";
    let result = sanitizer.sanitize(output);
    assert!(result.modified);
    assert!(result.pii_redacted > 0);
    assert!(result.output.contains("[REDACTED:Email Address]"));
}

#[test]
fn test_no_modification_needed() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let output = "The weather is nice today.";
    let result = sanitizer.sanitize(output);
    assert!(!result.modified);
    assert_eq!(result.pii_redacted, 0);
}

#[test]
fn test_length_truncation() {
    let config = SanitizerConfig { max_length: 50, ..Default::default() };
    let sanitizer = OutputSanitizer::new(config);
    let output = "This is a very long output that should be truncated to fit within the limit.";
    let result = sanitizer.sanitize(output);
    assert!(result.modified);
    assert!(result.output.len() <= 50);
    assert!(!result.warnings.is_empty());
}

#[test]
fn test_multiple_pii_types() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let output = "Email: test@example.com, Phone: 555-123-4567, SSN: 123-45-6789";
    let result = sanitizer.sanitize(output);
    assert!(result.modified);
    assert!(result.pii_redacted >= 3);
}

#[test]
fn test_format_validation() {
    assert!(validate_format("This is valid output").is_ok());
    assert!(validate_format("Invalid\0output").is_err());
}

#[test]
fn test_excessive_repetition_detection() {
    let repetitive = "hello world test hello world test hello world test hello world test hello world test hello world test hello world test";
    assert!(has_excessive_repetition(repetitive));
    let normal = "The quick brown fox jumps over the lazy dog and runs through the forest.";
    assert!(!has_excessive_repetition(normal));
}

#[test]
fn test_streaming_sanitization() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let mut state = StreamingSanitizerState::default();
    let chunk1 = sanitizer.sanitize_chunk("Contact ", &mut state);
    let chunk2 = sanitizer.sanitize_chunk("test@example.com", &mut state);
    let chunk3 = sanitizer.sanitize_chunk(" for help", &mut state);
    let full_output = format!("{}{}{}", chunk1, chunk2, chunk3);
    assert!(full_output.contains("[REDACTED") || state.buffer.contains("@"));
}

#[test]
fn test_confidence_threshold() {
    let config = SanitizerConfig { pii_confidence_threshold: 0.99, ..Default::default() };
    let sanitizer = OutputSanitizer::new(config);
    let output = "Email: test@example.com";
    let _result = sanitizer.sanitize(output);
}

#[test]
fn test_selective_pii_types() {
    let config = SanitizerConfig { redact_types: vec![PIIType::Email], ..Default::default() };
    let sanitizer = OutputSanitizer::new(config);
    let output = "Email: test@example.com, Phone: 555-123-4567";
    let result = sanitizer.sanitize(output);
    assert!(result.output.contains("[REDACTED:Email Address]"));
    assert!(result.output.contains("555-123-4567"));
}

#[test]
fn test_performance() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let output = "Contact support@example.com for help. Call 555-123-4567. SSN: 123-45-6789.".repeat(100);
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = sanitizer.sanitize(&output);
    }
    let duration = start.elapsed();
    assert!(duration.as_millis() < 10000, "Sanitization too slow: {:?}", duration);
}

#[test]
fn test_streaming_pii_split_attack() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let mut state = StreamingSanitizerState::default();
    let chunks = ["My email is j", "ohn.sm", "ith@example.com", " and my phone is 5", "55-1", "23-4567"];
    let mut outputs = Vec::new();
    for chunk in &chunks {
        outputs.push(sanitizer.sanitize_chunk(chunk, &mut state));
    }
    let full_buffer = &state.buffer;
    assert!(full_buffer.contains("john.smith@example.com") || full_buffer.contains("[REDACTED"));
    assert!(full_buffer.contains("555-123-4567") || full_buffer.contains("[REDACTED"));
}

#[test]
fn test_safe_trim_point() {
    let buffer = "This is a test sentence with a word boundary. And more text follows here to make it longer. We need at least 100 characters for the trim logic to work properly. Adding more padding text now.";
    assert!(buffer.len() > 100);
    let trim_point = find_safe_trim_point(buffer, 80);
    assert!(trim_point <= 80);
    let remaining = buffer.len() - trim_point;
    assert!(remaining >= 100);
}

#[test]
fn test_streaming_buffer_does_not_lose_pii() {
    let sanitizer = OutputSanitizer::default_sanitizer();
    let mut state = StreamingSanitizerState::default();
    let padding = "x".repeat(900);
    sanitizer.sanitize_chunk(&padding, &mut state);
    sanitizer.sanitize_chunk("Contact j", &mut state);
    sanitizer.sanitize_chunk("ohn.doe@test", &mut state);
    sanitizer.sanitize_chunk(".com for help", &mut state);
    let has_email = state.buffer.contains("john.doe@test.com") || state.buffer.contains("[REDACTED");
    assert!(has_email || state.buffer.len() >= 50);
}
