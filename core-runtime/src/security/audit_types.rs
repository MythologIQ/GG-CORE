//! Types and structs for the enterprise security audit module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Audit event severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum AuditSeverity {
    Info = 0,
    Warning = 1,
    Error = 2,
    Critical = 3,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditSeverity::Info => write!(f, "INFO"),
            AuditSeverity::Warning => write!(f, "WARNING"),
            AuditSeverity::Error => write!(f, "ERROR"),
            AuditSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Audit event categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditCategory {
    Authentication,
    Authorization,
    DataAccess,
    Configuration,
    Encryption,
    Network,
    ModelOperation,
    System,
}

impl std::fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditCategory::Authentication => write!(f, "AUTHENTICATION"),
            AuditCategory::Authorization => write!(f, "AUTHORIZATION"),
            AuditCategory::DataAccess => write!(f, "DATA_ACCESS"),
            AuditCategory::Configuration => write!(f, "CONFIGURATION"),
            AuditCategory::Encryption => write!(f, "ENCRYPTION"),
            AuditCategory::Network => write!(f, "NETWORK"),
            AuditCategory::ModelOperation => write!(f, "MODEL_OPERATION"),
            AuditCategory::System => write!(f, "SYSTEM"),
        }
    }
}

/// Audit event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub severity: AuditSeverity,
    pub category: AuditCategory,
    pub event_type: String,
    pub message: String,
    pub source: String,
    pub actor: Option<String>,
    pub resource: Option<String>,
    pub metadata: HashMap<String, String>,
    pub correlation_id: Option<String>,
    pub success: bool,
}

impl AuditEvent {
    pub fn builder() -> AuditEventBuilder {
        AuditEventBuilder::default()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_log_string(&self) -> String {
        format!(
            "[{}] {} [{}] {} - {} (actor={:?}, resource={:?}, success={})",
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            self.severity, self.category, self.event_type,
            self.message, self.actor, self.resource, self.success
        )
    }
}

/// Builder for audit events
#[derive(Debug, Default)]
pub struct AuditEventBuilder {
    severity: Option<AuditSeverity>,
    category: Option<AuditCategory>,
    event_type: Option<String>,
    message: Option<String>,
    source: Option<String>,
    actor: Option<String>,
    resource: Option<String>,
    metadata: HashMap<String, String>,
    correlation_id: Option<String>,
    success: bool,
}

impl AuditEventBuilder {
    pub fn severity(mut self, severity: AuditSeverity) -> Self {
        self.severity = Some(severity); self
    }
    pub fn category(mut self, category: AuditCategory) -> Self {
        self.category = Some(category); self
    }
    pub fn event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into()); self
    }
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into()); self
    }
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into()); self
    }
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into()); self
    }
    pub fn resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into()); self
    }
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into()); self
    }
    pub fn correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into()); self
    }
    pub fn success(mut self, success: bool) -> Self {
        self.success = success; self
    }

    pub fn build(self) -> Result<AuditEvent, &'static str> {
        Ok(AuditEvent {
            id: generate_event_id(),
            timestamp: Utc::now(),
            severity: self.severity.ok_or("severity is required")?,
            category: self.category.ok_or("category is required")?,
            event_type: self.event_type.ok_or("event_type is required")?,
            message: self.message.ok_or("message is required")?,
            source: self.source.ok_or("source is required")?,
            actor: self.actor,
            resource: self.resource,
            metadata: self.metadata,
            correlation_id: self.correlation_id,
            success: self.success,
        })
    }
}

/// Generate a unique event ID
pub fn generate_event_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes[..]);
    hex::encode(bytes)
}

/// Audit log configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub min_severity: AuditSeverity,
    pub max_events: usize,
    pub log_to_stdout: bool,
    pub include_sensitive: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            min_severity: AuditSeverity::Info,
            max_events: 10000,
            log_to_stdout: true,
            include_sensitive: false,
        }
    }
}
