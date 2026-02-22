//! Enterprise Security Audit Module
//!
//! Provides the audit logger and global instance management.
//! Types are in `audit_types.rs`.

use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

pub use super::audit_types::*;

/// Audit logger for enterprise security compliance
pub struct AuditLogger {
    config: AuditConfig,
    events: Arc<RwLock<Vec<AuditEvent>>>,
}

impl AuditLogger {
    pub fn new(config: AuditConfig) -> Self {
        Self { config, events: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn log(&self, event: AuditEvent) {
        if event.severity < self.config.min_severity {
            return;
        }
        if self.config.log_to_stdout {
            println!("{}", event.to_log_string());
        }
        let mut events = self.events.write().await;
        events.push(event);
        if events.len() > self.config.max_events {
            let excess = events.len() - self.config.max_events;
            events.drain(0..excess);
        }
    }

    pub async fn log_event(
        &self, severity: AuditSeverity, category: AuditCategory,
        event_type: &str, message: &str, source: &str,
    ) {
        if let Ok(event) = AuditEvent::builder()
            .severity(severity).category(category)
            .event_type(event_type).message(message).source(source)
            .build()
        {
            self.log(event).await;
        }
    }

    pub async fn get_events(&self) -> Vec<AuditEvent> {
        self.events.read().await.clone()
    }

    pub async fn get_events_by_category(&self, category: AuditCategory) -> Vec<AuditEvent> {
        self.events.read().await.iter()
            .filter(|e| e.category == category).cloned().collect()
    }

    pub async fn get_events_by_severity(&self, severity: AuditSeverity) -> Vec<AuditEvent> {
        self.events.read().await.iter()
            .filter(|e| e.severity >= severity).cloned().collect()
    }

    pub async fn get_events_by_time(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<AuditEvent> {
        self.events.read().await.iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end).cloned().collect()
    }

    pub async fn clear(&self) { self.events.write().await.clear(); }

    pub async fn export_json(&self) -> Result<String, serde_json::Error> {
        let events = self.events.read().await;
        serde_json::to_string_pretty(&*events)
    }

    pub async fn event_count(&self) -> usize { self.events.read().await.len() }
}

impl Default for AuditLogger {
    fn default() -> Self { Self::new(AuditConfig::default()) }
}

/// Global audit logger instance
static AUDIT_LOGGER: std::sync::OnceLock<Arc<AuditLogger>> = std::sync::OnceLock::new();

pub fn init_audit_logger(config: AuditConfig) {
    let _ = AUDIT_LOGGER.get_or_init(|| Arc::new(AuditLogger::new(config)));
}

pub fn audit_logger() -> Option<Arc<AuditLogger>> {
    AUDIT_LOGGER.get().cloned()
}

/// Convenience macro for audit logging
#[macro_export]
macro_rules! audit_log {
    ($severity:expr, $category:expr, $event_type:expr, $message:expr, $source:expr) => {
        if let Some(logger) = $crate::security::audit::audit_logger() {
            tokio::spawn(async move {
                logger.log_event($severity, $category, $event_type, $message, $source).await;
            });
        }
    };
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
