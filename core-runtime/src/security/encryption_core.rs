//! Model Encryption Core
//!
//! Provides AES-256-GCM encryption for model files at rest.
//! Uses hardware acceleration where available (AES-NI).
//!
//! # Security
//! - AES-GCM provides confidentiality, integrity, and semantic security
//! - Nonce reuse is detected and prevented
//! - Key material is securely zeroed on drop via `zeroize`

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;
use zeroize::{ZeroizeOnDrop, Zeroizing};

use super::encryption_io;

/// Encryption key size (256 bits)
pub const KEY_SIZE: usize = 32;
/// Nonce size (96 bits for GCM)
pub const NONCE_SIZE: usize = 12;
/// Tag size (128 bits)
pub const TAG_SIZE: usize = 16;
/// Block size
pub const BLOCK_SIZE: usize = 16;
/// Maximum nonce history to track for reuse detection
const MAX_NONCE_HISTORY: usize = 10_000;

/// Global nonce tracker for reuse detection
static NONCE_TRACKER: OnceLock<Mutex<HashSet<[u8; NONCE_SIZE]>>> = OnceLock::new();

/// Get or initialize the global nonce tracker
fn get_nonce_tracker() -> &'static Mutex<HashSet<[u8; NONCE_SIZE]>> {
    NONCE_TRACKER.get_or_init(|| Mutex::new(HashSet::with_capacity(MAX_NONCE_HISTORY)))
}

/// Check if a nonce has been used and register it if not.
pub fn check_and_register_nonce(nonce: &[u8; NONCE_SIZE]) -> Result<(), EncryptionError> {
    let tracker = get_nonce_tracker();
    let mut guard = tracker.lock().map_err(|_| {
        EncryptionError::EncryptionFailed("Nonce tracker lock poisoned".to_string())
    })?;

    if guard.contains(nonce) {
        return Err(EncryptionError::NonceReuseDetected);
    }

    if guard.len() >= MAX_NONCE_HISTORY {
        let to_remove: Vec<[u8; NONCE_SIZE]> = guard.iter().take(MAX_NONCE_HISTORY / 2).copied().collect();
        for key in to_remove {
            guard.remove(&key);
        }
    }

    guard.insert(*nonce);
    Ok(())
}

/// Encryption error types
#[derive(Debug, Clone)]
pub enum EncryptionError {
    InvalidKeySize,
    EncryptionFailed(String),
    DecryptionFailed(String),
    InvalidCiphertext,
    IoError(String),
    AuthenticationFailed,
    NonceReuseDetected,
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionError::InvalidKeySize => write!(f, "Invalid key size"),
            EncryptionError::EncryptionFailed(s) => write!(f, "Encryption failed: {}", s),
            EncryptionError::DecryptionFailed(s) => write!(f, "Decryption failed: {}", s),
            EncryptionError::InvalidCiphertext => write!(f, "Invalid ciphertext"),
            EncryptionError::IoError(s) => write!(f, "IO error: {}", s),
            EncryptionError::AuthenticationFailed => write!(f, "Authentication failed"),
            EncryptionError::NonceReuseDetected => {
                write!(f, "CRITICAL: Nonce reuse detected - possible RNG failure")
            }
        }
    }
}

impl std::error::Error for EncryptionError {}

/// Model encryption handler using AES-256-GCM
#[derive(ZeroizeOnDrop)]
pub struct ModelEncryption {
    #[zeroize(skip)]
    key: Zeroizing<[u8; KEY_SIZE]>,
    hw_accelerated: bool,
}

impl ModelEncryption {
    /// Create a new encryption handler with the given key
    pub fn new(key: [u8; KEY_SIZE]) -> Self {
        #[cfg(target_arch = "x86_64")]
        let hw_accelerated = is_x86_feature_detected!("aes");
        #[cfg(not(target_arch = "x86_64"))]
        let hw_accelerated = false;

        Self {
            key: Zeroizing::new(key),
            hw_accelerated,
        }
    }

    /// PBKDF2 iteration count (600,000 iterations per OWASP 2023 recommendations)
    pub const PBKDF2_ITERATIONS: u32 = 600_000;

    /// Create encryption handler from a password (derived key).
    pub fn from_password(password: &str, salt: &[u8]) -> Self {
        super::encryption_key::derive_key_from_password(password, salt, Self::PBKDF2_ITERATIONS)
    }

    /// Generate a key from machine-specific identifiers.
    pub fn from_machine_id() -> Result<Self, EncryptionError> {
        super::encryption_key::from_machine_id(Self::PBKDF2_ITERATIONS)
    }

    /// Encrypt data using AES-256-GCM. Returns (nonce, ciphertext_with_tag).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(self.key.as_slice());
        let cipher = Aes256Gcm::new(key);

        let nonce_bytes = Self::generate_nonce()?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        Ok((nonce_bytes, ciphertext))
    }

    /// Decrypt data using AES-256-GCM.
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if nonce.len() != NONCE_SIZE {
            return Err(EncryptionError::DecryptionFailed(
                "Invalid nonce size".to_string(),
            ));
        }

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(self.key.as_slice());
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::AuthenticationFailed)
    }

    /// Encrypt a file
    pub fn encrypt_file(&self, input_path: &Path, output_path: &Path) -> Result<(), EncryptionError> {
        let plaintext = encryption_io::read_file_bytes(input_path)?;
        let (nonce, ciphertext) = self.encrypt(&plaintext)?;
        encryption_io::write_encrypted_file(output_path, &nonce, &ciphertext)
    }

    /// Decrypt a file
    pub fn decrypt_file(&self, input_path: &Path, output_path: &Path) -> Result<(), EncryptionError> {
        let plaintext = encryption_io::read_and_decrypt_file(self, input_path)?;
        std::fs::write(output_path, &plaintext).map_err(|e| EncryptionError::IoError(e.to_string()))
    }

    /// Check if hardware acceleration is available
    pub fn is_hw_accelerated(&self) -> bool {
        self.hw_accelerated
    }

    /// Generate random nonce using CSPRNG with reuse detection.
    fn generate_nonce() -> Result<Vec<u8>, EncryptionError> {
        use rand::RngCore;
        let mut nonce = [0u8; NONCE_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut nonce[..]);
        check_and_register_nonce(&nonce)?;
        Ok(nonce.to_vec())
    }

    /// Legacy ECB decryption (deprecated, migration only)
    #[deprecated(note = "ECB mode is insecure. Only use for migrating legacy encrypted files.")]
    pub(crate) fn decrypt_legacy(&self, _nonce: &[u8], _ct: &[u8], _tag: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        Err(EncryptionError::DecryptionFailed(
            "Legacy ECB format no longer supported. Please re-encrypt your files.".to_string(),
        ))
    }
}

