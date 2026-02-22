//! Tests for the enterprise security audit module.

use super::super::audit_types::*;
use super::*;

#[test]
fn test_audit_severity_ordering() {
    assert!(AuditSeverity::Critical > AuditSeverity::Error);
    assert!(AuditSeverity::Error > AuditSeverity::Warning);
    assert!(AuditSeverity::Warning > AuditSeverity::Info);
}

#[test]
fn test_audit_event_builder() {
    let event = AuditEvent::builder()
        .severity(AuditSeverity::Info)
        .category(AuditCategory::Authentication)
        .event_type("login")
        .message("User logged in")
        .source("auth_module")
        .actor("user123")
        .success(true)
        .build()
        .unwrap();

    assert_eq!(event.severity, AuditSeverity::Info);
    assert_eq!(event.category, AuditCategory::Authentication);
    assert_eq!(event.event_type, "login");
    assert!(event.success);
}

#[test]
fn test_audit_event_builder_missing_fields() {
    let result = AuditEvent::builder().severity(AuditSeverity::Info).build();
    assert!(result.is_err());
}

#[test]
fn test_audit_event_to_json() {
    let event = AuditEvent::builder()
        .severity(AuditSeverity::Warning)
        .category(AuditCategory::DataAccess)
        .event_type("read")
        .message("Data accessed")
        .source("api")
        .build()
        .unwrap();

    let json = event.to_json().unwrap();
    assert!(json.contains("Warning"));
    assert!(json.contains("DataAccess"));
}

#[test]
fn test_audit_event_to_log_string() {
    let event = AuditEvent::builder()
        .severity(AuditSeverity::Error)
        .category(AuditCategory::Encryption)
        .event_type("decrypt_failed")
        .message("Decryption failed")
        .source("encryption")
        .actor("session123")
        .resource("model.gguf")
        .success(false)
        .build()
        .unwrap();

    let log = event.to_log_string();
    assert!(log.contains("ERROR"));
    assert!(log.contains("ENCRYPTION"));
    assert!(log.contains("Decryption failed"));
    assert!(log.contains("session123"));
    assert!(log.contains("model.gguf"));
    assert!(log.contains("success=false"));
}

#[tokio::test]
async fn test_audit_logger() {
    let logger = AuditLogger::new(AuditConfig::default());
    let event = AuditEvent::builder()
        .severity(AuditSeverity::Info)
        .category(AuditCategory::System)
        .event_type("startup")
        .message("System started")
        .source("main")
        .build()
        .unwrap();

    logger.log(event).await;
    assert_eq!(logger.event_count().await, 1);
}

#[tokio::test]
async fn test_audit_logger_severity_filter() {
    let config = AuditConfig { min_severity: AuditSeverity::Warning, ..Default::default() };
    let logger = AuditLogger::new(config);

    let info_event = AuditEvent::builder()
        .severity(AuditSeverity::Info)
        .category(AuditCategory::System)
        .event_type("test").message("Info event").source("test")
        .build().unwrap();
    logger.log(info_event).await;
    assert_eq!(logger.event_count().await, 0);

    let warning_event = AuditEvent::builder()
        .severity(AuditSeverity::Warning)
        .category(AuditCategory::System)
        .event_type("test").message("Warning event").source("test")
        .build().unwrap();
    logger.log(warning_event).await;
    assert_eq!(logger.event_count().await, 1);
}

#[tokio::test]
async fn test_audit_logger_max_events() {
    let config = AuditConfig { max_events: 5, ..Default::default() };
    let logger = AuditLogger::new(config);

    for i in 0..10 {
        let event = AuditEvent::builder()
            .severity(AuditSeverity::Info)
            .category(AuditCategory::System)
            .event_type("test").message(format!("Event {}", i)).source("test")
            .build().unwrap();
        logger.log(event).await;
    }
    assert_eq!(logger.event_count().await, 5);
}

#[tokio::test]
async fn test_get_events_by_category() {
    let logger = AuditLogger::new(AuditConfig::default());
    for i in 0..5 {
        let event = AuditEvent::builder()
            .severity(AuditSeverity::Info)
            .category(if i % 2 == 0 { AuditCategory::Authentication } else { AuditCategory::DataAccess })
            .event_type("test").message(format!("Event {}", i)).source("test")
            .build().unwrap();
        logger.log(event).await;
    }
    let auth_events = logger.get_events_by_category(AuditCategory::Authentication).await;
    assert_eq!(auth_events.len(), 3);
    let data_events = logger.get_events_by_category(AuditCategory::DataAccess).await;
    assert_eq!(data_events.len(), 2);
}

#[tokio::test]
async fn test_export_json() {
    let logger = AuditLogger::new(AuditConfig::default());
    let event = AuditEvent::builder()
        .severity(AuditSeverity::Info)
        .category(AuditCategory::System)
        .event_type("test").message("Test event").source("test")
        .build().unwrap();
    logger.log(event).await;
    let json = logger.export_json().await.unwrap();
    assert!(json.starts_with("["));
    assert!(json.contains("Test event"));
}

#[test]
fn test_generate_event_id() {
    let id1 = generate_event_id();
    let id2 = generate_event_id();
    assert_eq!(id1.len(), 32);
    assert_eq!(id2.len(), 32);
    assert_ne!(id1, id2);
    assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
}
