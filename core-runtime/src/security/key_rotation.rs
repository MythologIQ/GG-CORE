//! Key Rotation Module (SOC2-2 Compliance)
//!
//! Provides cryptographic key rotation with:
//! - Key versioning (key ID in encrypted blob header)
//! - Gradual migration (decrypt with old, encrypt with new)
//! - Audit logging for key rotation events
//!
//! File format with key version:
//! ```text
//! [GGGCM][version:2][key_id:4][nonce:12][len:8][ciphertext+tag]
//! ```

use super::audit::{audit_logger, AuditCategory, AuditEvent, AuditSeverity};
use super::encryption::{EncryptionError, ModelEncryption, KEY_SIZE, NONCE_SIZE};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Key ID type (4 bytes, supports ~4 billion key versions)
pub type KeyId = u32;

/// Magic number for versioned encrypted files
pub const VERSIONED_MAGIC: &[u8; 5] = b"GGGCM";

/// Current file format version
pub const FORMAT_VERSION: u8 = 3;

/// Key rotation error types
#[derive(Debug, Clone)]
pub enum KeyRotationError {
    /// Key not found in registry
    KeyNotFound(KeyId),
    /// Encryption error
    Encryption(EncryptionError),
    /// No active key configured
    NoActiveKey,
    /// IO error
    IoError(String),
    /// Invalid file format
    InvalidFormat(String),
}

impl std::fmt::Display for KeyRotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyNotFound(id) => write!(f, "Key not found: {id}"),
            Self::Encryption(e) => write!(f, "Encryption error: {e}"),
            Self::NoActiveKey => write!(f, "No active encryption key configured"),
            Self::IoError(s) => write!(f, "IO error: {s}"),
            Self::InvalidFormat(s) => write!(f, "Invalid format: {s}"),
        }
    }
}

impl std::error::Error for KeyRotationError {}

impl From<EncryptionError> for KeyRotationError {
    fn from(e: EncryptionError) -> Self {
        Self::Encryption(e)
    }
}

/// Key entry with metadata
#[derive(Clone)]
struct KeyEntry {
    key: [u8; KEY_SIZE],
    /// Timestamp when key was created (for audit/compliance)
    #[allow(dead_code)]
    created_at: std::time::SystemTime,
    is_active: bool,
}

/// Key rotation manager with versioned key storage
pub struct KeyRotationManager {
    keys: Arc<RwLock<HashMap<KeyId, KeyEntry>>>,
    active_key_id: Arc<RwLock<Option<KeyId>>>,
    next_key_id: Arc<RwLock<KeyId>>,
}

impl KeyRotationManager {
    /// Create a new key rotation manager
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            active_key_id: Arc::new(RwLock::new(None)),
            next_key_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Add a new key and optionally make it active
    pub async fn add_key(&self, key: [u8; KEY_SIZE], make_active: bool) -> KeyId {
        let mut keys = self.keys.write().await;
        let mut next_id = self.next_key_id.write().await;

        let key_id = *next_id;
        *next_id += 1;

        keys.insert(
            key_id,
            KeyEntry {
                key,
                created_at: std::time::SystemTime::now(),
                is_active: make_active,
            },
        );

        if make_active {
            *self.active_key_id.write().await = Some(key_id);
            Self::log_key_rotation_event(key_id, "key_added_active").await;
        } else {
            Self::log_key_rotation_event(key_id, "key_added").await;
        }

        key_id
    }

    /// Rotate to a new key (generates new key, makes it active)
    pub async fn rotate_key(&self) -> Result<KeyId, KeyRotationError> {
        use rand::RngCore;
        let mut new_key = [0u8; KEY_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut new_key);

        let old_key_id = *self.active_key_id.read().await;
        let new_key_id = self.add_key(new_key, true).await;

        // Mark old key as inactive
        if let Some(old_id) = old_key_id {
            if let Some(entry) = self.keys.write().await.get_mut(&old_id) {
                entry.is_active = false;
            }
        }

        Self::log_key_rotation_event(new_key_id, "key_rotated").await;
        Ok(new_key_id)
    }

    /// Get the active key ID
    pub async fn active_key_id(&self) -> Option<KeyId> {
        *self.active_key_id.read().await
    }

    /// Encrypt data with the active key (includes key ID in header)
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, KeyRotationError> {
        let key_id = self
            .active_key_id
            .read()
            .await
            .ok_or(KeyRotationError::NoActiveKey)?;

        let keys = self.keys.read().await;
        let entry = keys
            .get(&key_id)
            .ok_or(KeyRotationError::KeyNotFound(key_id))?;

        let encryption = ModelEncryption::new(entry.key);
        let (nonce, ciphertext) = encryption.encrypt(plaintext)?;

        // Build versioned format: magic + version + key_id + nonce + len + ciphertext
        let mut output = Vec::with_capacity(5 + 1 + 4 + NONCE_SIZE + 8 + ciphertext.len());
        output.extend_from_slice(VERSIONED_MAGIC);
        output.push(FORMAT_VERSION);
        output.extend_from_slice(&key_id.to_le_bytes());
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
        output.extend_from_slice(&ciphertext);

        Ok(output)
    }

    /// Decrypt data (auto-detects key version from header)
    pub async fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, KeyRotationError> {
        let (key_id, nonce, ciphertext) = Self::parse_header(data)?;

        let keys = self.keys.read().await;
        let entry = keys
            .get(&key_id)
            .ok_or(KeyRotationError::KeyNotFound(key_id))?;

        let encryption = ModelEncryption::new(entry.key);
        let plaintext = encryption.decrypt(&nonce, &ciphertext)?;

        Ok(plaintext)
    }

    /// Parse encrypted file header to extract key ID
    fn parse_header(data: &[u8]) -> Result<(KeyId, Vec<u8>, Vec<u8>), KeyRotationError> {
        if data.len() < 5 + 1 + 4 + NONCE_SIZE + 8 {
            return Err(KeyRotationError::InvalidFormat("Data too short".into()));
        }

        // Verify magic
        if &data[0..5] != VERSIONED_MAGIC {
            return Err(KeyRotationError::InvalidFormat("Invalid magic".into()));
        }

        let version = data[5];
        if version < 3 {
            return Err(KeyRotationError::InvalidFormat("Version too old".into()));
        }

        // Extract key ID
        let key_id = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);

        // Extract nonce
        let nonce = data[10..10 + NONCE_SIZE].to_vec();

        // Extract ciphertext length and data
        let len_start = 10 + NONCE_SIZE;
        let len_bytes: [u8; 8] = data[len_start..len_start + 8]
            .try_into()
            .map_err(|_| KeyRotationError::InvalidFormat("Invalid length field".into()))?;
        let ct_len = u64::from_le_bytes(len_bytes) as usize;

        let ct_start = len_start + 8;
        if data.len() < ct_start + ct_len {
            return Err(KeyRotationError::InvalidFormat("Truncated data".into()));
        }

        let ciphertext = data[ct_start..ct_start + ct_len].to_vec();
        Ok((key_id, nonce, ciphertext))
    }

    /// Log key rotation event to audit system
    async fn log_key_rotation_event(key_id: KeyId, event_type: &str) {
        if let Some(logger) = audit_logger() {
            if let Ok(event) = AuditEvent::builder()
                .severity(AuditSeverity::Info)
                .category(AuditCategory::Encryption)
                .event_type(event_type)
                .message(format!("Key rotation event for key_id={key_id}"))
                .source("key_rotation")
                .metadata("key_id", key_id.to_string())
                .success(true)
                .build()
            {
                logger.log(event).await;
            }
        }
    }

    /// Re-encrypt a file from old key to active key
    pub async fn migrate_file(&self, input: &Path, output: &Path) -> Result<(), KeyRotationError> {
        let data = std::fs::read(input).map_err(|e| KeyRotationError::IoError(e.to_string()))?;
        let plaintext = self.decrypt(&data).await?;
        let encrypted = self.encrypt(&plaintext).await?;
        std::fs::write(output, encrypted).map_err(|e| KeyRotationError::IoError(e.to_string()))?;
        Ok(())
    }
}

impl Default for KeyRotationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_encrypt() {
        let mgr = KeyRotationManager::new();
        let key = [0x42u8; KEY_SIZE];
        mgr.add_key(key, true).await;

        let plaintext = b"Hello, key rotation!";
        let encrypted = mgr.encrypt(plaintext).await.unwrap();
        let decrypted = mgr.decrypt(&encrypted).await.unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let mgr = KeyRotationManager::new();
        let key1 = [0x11u8; KEY_SIZE];
        let id1 = mgr.add_key(key1, true).await;

        let plaintext = b"Data encrypted with key 1";
        let encrypted_v1 = mgr.encrypt(plaintext).await.unwrap();

        // Rotate to new key
        let id2 = mgr.rotate_key().await.unwrap();
        assert_ne!(id1, id2);

        // Old encrypted data should still decrypt
        let decrypted = mgr.decrypt(&encrypted_v1).await.unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());

        // New data encrypted with new key
        let encrypted_v2 = mgr.encrypt(plaintext).await.unwrap();
        let decrypted_v2 = mgr.decrypt(&encrypted_v2).await.unwrap();
        assert_eq!(plaintext.as_slice(), decrypted_v2.as_slice());
    }

    #[tokio::test]
    async fn test_no_active_key_error() {
        let mgr = KeyRotationManager::new();
        let result = mgr.encrypt(b"test").await;
        assert!(matches!(result, Err(KeyRotationError::NoActiveKey)));
    }

    #[tokio::test]
    async fn test_key_not_found_error() {
        let mgr = KeyRotationManager::new();
        let fake_data = build_fake_encrypted(999);
        let result = mgr.decrypt(&fake_data).await;
        assert!(matches!(result, Err(KeyRotationError::KeyNotFound(999))));
    }

    fn build_fake_encrypted(key_id: KeyId) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(VERSIONED_MAGIC);
        data.push(FORMAT_VERSION);
        data.extend_from_slice(&key_id.to_le_bytes());
        data.extend_from_slice(&[0u8; NONCE_SIZE]);
        data.extend_from_slice(&16u64.to_le_bytes());
        data.extend_from_slice(&[0u8; 16]);
        data
    }
}
