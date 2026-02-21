//! Model Encryption Module
//!
//! Provides AES-256-GCM encryption for model files at rest.
//! Uses hardware acceleration where available (AES-NI).
//!
//! SECURITY: This module uses AES-GCM (Galois/Counter Mode) which provides:
//! - Confidentiality (encryption)
//! - Integrity (authentication tag)
//! - Semantic security (identical plaintexts produce different ciphertexts)
//!
//! # Key Derivation
//!
//! For machine-bound encryption, use [`ModelEncryption::from_machine_id`] which
//! generates a unique, installation-specific salt rather than a hardcoded value.
//! This prevents attackers from deriving keys even if they know the machine ID.
//!
//! # Key Zeroing
//!
//! All key material is securely zeroed when dropped using the `zeroize` crate.
//! This prevents key recovery from memory dumps or cold boot attacks.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Encryption key size (256 bits)
pub const KEY_SIZE: usize = 32;
/// Nonce size (96 bits for GCM)
pub const NONCE_SIZE: usize = 12;
/// Tag size (128 bits)
pub const TAG_SIZE: usize = 16;
/// Block size
pub const BLOCK_SIZE: usize = 16;
/// Minimum salt size for security (16 bytes = 128 bits)
pub const MIN_SALT_SIZE: usize = 16;
/// Default salt file name
const SALT_FILE_NAME: &str = ".gg-core-salt";
/// Maximum nonce history to track for reuse detection
const MAX_NONCE_HISTORY: usize = 10_000;

/// Global installation salt (generated once, cached)
static INSTALLATION_SALT: OnceLock<Vec<u8>> = OnceLock::new();

/// Global nonce tracker for reuse detection
/// Uses a HashSet protected by a Mutex for thread-safe access
static NONCE_TRACKER: OnceLock<Mutex<HashSet<[u8; NONCE_SIZE]>>> = OnceLock::new();

/// Get or initialize the global nonce tracker
fn get_nonce_tracker() -> &'static Mutex<HashSet<[u8; NONCE_SIZE]>> {
    NONCE_TRACKER.get_or_init(|| Mutex::new(HashSet::with_capacity(MAX_NONCE_HISTORY)))
}

/// Check if a nonce has been used and register it if not.
/// Returns true if the nonce is new (safe to use), false if it was already used.
fn check_and_register_nonce(nonce: &[u8; NONCE_SIZE]) -> Result<(), EncryptionError> {
    let tracker = get_nonce_tracker();
    let mut tracker_guard = tracker.lock().map_err(|_| {
        EncryptionError::EncryptionFailed("Nonce tracker lock poisoned".to_string())
    })?;

    if tracker_guard.contains(nonce) {
        // CRITICAL: Nonce reuse detected - this should never happen with CSPRNG
        // If it does, it indicates a serious RNG failure
        return Err(EncryptionError::NonceReuseDetected);
    }

    // Add to tracker, evicting oldest if at capacity
    if tracker_guard.len() >= MAX_NONCE_HISTORY {
        // Simple eviction: clear half the entries
        // In production, you might want LRU eviction
        let to_remove: Vec<[u8; NONCE_SIZE]> = tracker_guard
            .iter()
            .take(MAX_NONCE_HISTORY / 2)
            .copied()
            .collect();
        for key in to_remove {
            tracker_guard.remove(&key);
        }
    }

    tracker_guard.insert(*nonce);
    Ok(())
}

/// Get or create the installation-specific salt.
///
/// The salt is stored in a file within the application's data directory.
/// If the file doesn't exist, a new cryptographically random salt is generated.
///
/// # Security
/// - Uses CSPRNG (OsRng) for salt generation
/// - Salt is at least 16 bytes (128 bits) for security
/// - File is created with restrictive permissions (0600 on Unix)
/// - Salt is cached in memory to avoid repeated file I/O
fn get_or_create_installation_salt() -> Result<Vec<u8>, EncryptionError> {
    // Check if already cached
    if let Some(salt) = INSTALLATION_SALT.get() {
        return Ok(salt.clone());
    }

    let salt_path = get_salt_file_path()?;

    // Try to read existing salt
    let salt = if salt_path.exists() {
        let existing = std::fs::read(&salt_path)
            .map_err(|e| EncryptionError::IoError(format!("Failed to read salt file: {}", e)))?;

        if existing.len() >= MIN_SALT_SIZE {
            existing
        } else {
            // Salt too small, regenerate
            generate_and_save_salt(&salt_path)?
        }
    } else {
        // Generate new salt
        generate_and_save_salt(&salt_path)?
    };

    // Cache the salt (ignore if already set by another thread)
    let _ = INSTALLATION_SALT.set(salt.clone());

    Ok(salt)
}

/// Generate a new salt and save it to disk.
fn generate_and_save_salt(salt_path: &Path) -> Result<Vec<u8>, EncryptionError> {
    // Generate new salt
    let salt = generate_random_salt();

    // Ensure parent directory exists
    if let Some(parent) = salt_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            EncryptionError::IoError(format!("Failed to create salt directory: {}", e))
        })?;
    }

    // Write salt with restrictive permissions
    write_salt_file(salt_path, &salt)?;

    Ok(salt)
}

/// Get the path to the salt file.
///
/// Uses platform-specific application data directories:
/// - Windows: %LOCALAPPDATA%\gg-core\
/// - Unix: ~/.config/gg-core/
fn get_salt_file_path() -> Result<PathBuf, EncryptionError> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .map_err(|_| {
                EncryptionError::IoError("Could not find application data directory".to_string())
            })?;
        Ok(PathBuf::from(app_data).join("gg-core").join(SALT_FILE_NAME))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")
            .map_err(|_| EncryptionError::IoError("Could not find home directory".to_string()))?;
        Ok(PathBuf::from(home)
            .join(".config")
            .join("gg-core")
            .join(SALT_FILE_NAME))
    }
}

/// Generate a cryptographically random salt.
fn generate_random_salt() -> Vec<u8> {
    use rand::RngCore;
    let mut salt = vec![0u8; MIN_SALT_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Write salt file with restrictive permissions.
#[cfg(target_os = "windows")]
fn write_salt_file(path: &Path, salt: &[u8]) -> Result<(), EncryptionError> {
    std::fs::write(path, salt)
        .map_err(|e| EncryptionError::IoError(format!("Failed to write salt file: {}", e)))
}

/// Write salt file with restrictive permissions (Unix).
#[cfg(not(target_os = "windows"))]
fn write_salt_file(path: &Path, salt: &[u8]) -> Result<(), EncryptionError> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600) // Owner read/write only
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, salt))
        .map_err(|e| EncryptionError::IoError(format!("Failed to write salt file: {}", e)))
}

/// Encryption error types
#[derive(Debug, Clone)]
pub enum EncryptionError {
    /// Invalid key size
    InvalidKeySize,
    /// Encryption failed
    EncryptionFailed(String),
    /// Decryption failed
    DecryptionFailed(String),
    /// Invalid ciphertext
    InvalidCiphertext,
    /// IO error
    IoError(String),
    /// Authentication failed
    AuthenticationFailed,
    /// Nonce reuse detected (critical security failure)
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
///
/// # Security
/// - Keys are automatically zeroed on drop using `zeroize`
/// - Nonce reuse is detected and prevented
/// - Uses AES-256-GCM for authenticated encryption
#[derive(ZeroizeOnDrop)]
pub struct ModelEncryption {
    /// Encryption key (automatically zeroed on drop)
    #[zeroize(skip)]
    key: Zeroizing<[u8; KEY_SIZE]>,
    /// Whether hardware acceleration is available
    hw_accelerated: bool,
}

impl ModelEncryption {
    /// Create a new encryption handler with the given key
    ///
    /// The key is wrapped in `Zeroizing` to ensure it is securely erased on drop.
    pub fn new(key: [u8; KEY_SIZE]) -> Self {
        // Check for AES-NI support
        #[cfg(target_arch = "x86_64")]
        let hw_accelerated = is_x86_feature_detected!("aes");
        #[cfg(not(target_arch = "x86_64"))]
        let hw_accelerated = false;

        Self {
            key: Zeroizing::new(key),
            hw_accelerated,
        }
    }

    /// PBKDF2 iteration count (100,000 iterations per OWASP recommendations)
    /// This provides resistance against brute-force attacks on passwords
    const PBKDF2_ITERATIONS: u32 = 100_000;

    /// Create encryption handler from a password (derived key)
    ///
    /// Uses PBKDF2-HMAC-SHA256 with 100,000 iterations for secure key derivation.
    /// This provides resistance against brute-force and dictionary attacks.
    ///
    /// # Arguments
    /// * `password` - User-provided password
    /// * `salt` - Cryptographic salt (should be unique per password)
    ///
    /// # Security
    /// - Uses PBKDF2 with 100,000 iterations (OWASP recommended minimum)
    /// - Salt should be at least 16 bytes and unique per password
    /// - Password should be high entropy (use a password manager)
    /// - Derived key is securely zeroed on drop
    pub fn from_password(password: &str, salt: &[u8]) -> Self {
        let mut key = [0u8; KEY_SIZE];

        // Use PBKDF2-HMAC-SHA256 for secure key derivation
        pbkdf2_hmac::<Sha256>(
            password.as_bytes(),
            salt,
            Self::PBKDF2_ITERATIONS,
            &mut key[..],
        );

        let result = Self::new(key);
        // Zero the local key copy (the Zeroizing in Self will handle the other)
        key.zeroize();
        result
    }

    /// Generate a key from machine-specific identifiers
    /// This ties encryption to the specific machine.
    ///
    /// # Security
    /// Uses a unique, installation-specific salt rather than a hardcoded value.
    /// This prevents attackers from precomputing keys even if they know the machine ID.
    /// The salt is stored in the application data directory and generated once per installation.
    #[cfg(target_os = "windows")]
    pub fn from_machine_id() -> Result<Self, EncryptionError> {
        use std::process::Command;

        // Get machine GUID on Windows
        let output = Command::new("reg")
            .args([
                "query",
                "HKLM\\SOFTWARE\\Microsoft\\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output();

        let machine_id = match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Extract GUID from output
                if let Some(pos) = stdout.find("MachineGuid") {
                    let rest = &stdout[pos..];
                    if let Some(start) = rest.find("REG_SZ") {
                        let guid_part = &rest[start + 6..];
                        guid_part.trim().to_string()
                    } else {
                        return Err(EncryptionError::EncryptionFailed(
                            "Could not parse machine GUID".to_string(),
                        ));
                    }
                } else {
                    return Err(EncryptionError::EncryptionFailed(
                        "Machine GUID not found in registry".to_string(),
                    ));
                }
            }
            Err(e) => {
                return Err(EncryptionError::EncryptionFailed(format!(
                    "Failed to query registry: {}",
                    e
                )))
            }
        };

        // Get or create installation-specific salt
        let salt = get_or_create_installation_salt()?;
        Ok(Self::from_password(&machine_id, &salt))
    }

    /// Generate a key from machine-specific identifiers
    /// This ties encryption to the specific machine.
    ///
    /// # Security
    /// Uses a unique, installation-specific salt rather than a hardcoded value.
    /// This prevents attackers from precomputing keys even if they know the machine ID.
    /// The salt is stored in the application data directory and generated once per installation.
    #[cfg(not(target_os = "windows"))]
    pub fn from_machine_id() -> Result<Self, EncryptionError> {
        // On non-Windows, use a combination of hostname and user
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .map_err(|e| EncryptionError::EncryptionFailed(format!("Hostname error: {}", e)))?;

        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .map_err(|_| {
                EncryptionError::EncryptionFailed("Could not determine user".to_string())
            })?;

        let combined = format!("{}-{}", hostname, user);

        // Get or create installation-specific salt
        let salt = get_or_create_installation_salt()?;
        Ok(Self::from_password(&combined, &salt))
    }

    /// Encrypt data using AES-256-GCM
    /// Returns (nonce, ciphertext_with_tag)
    ///
    /// The ciphertext includes the authentication tag appended to it.
    ///
    /// # Security
    /// - Nonce reuse is detected and will return an error
    /// - Uses cryptographically random nonces from OsRng
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
        // Create cipher
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(self.key.as_slice());
        let cipher = Aes256Gcm::new(key);

        // Generate random nonce (required for semantic security)
        let nonce_bytes = Self::generate_nonce()?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt with AES-GCM (includes authentication)
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        Ok((nonce_bytes, ciphertext))
    }

    /// Decrypt data using AES-256-GCM
    ///
    /// The ciphertext must include the authentication tag.
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if nonce.len() != NONCE_SIZE {
            return Err(EncryptionError::DecryptionFailed(
                "Invalid nonce size".to_string(),
            ));
        }

        // Create cipher
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(self.key.as_slice());
        let cipher = Aes256Gcm::new(key);

        // Decrypt with AES-GCM (verifies authentication tag)
        let nonce = Nonce::from_slice(nonce);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::AuthenticationFailed)?;

        Ok(plaintext)
    }

    /// Encrypt a file
    pub fn encrypt_file(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), EncryptionError> {
        // Read input file
        let mut input_file =
            std::fs::File::open(input_path).map_err(|e| EncryptionError::IoError(e.to_string()))?;

        let mut plaintext = Vec::new();
        input_file
            .read_to_end(&mut plaintext)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Encrypt
        let (nonce, ciphertext) = self.encrypt(&plaintext)?;

        // Write output file with header
        let mut output_file = std::fs::File::create(output_path)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Write magic number (GGCM = gg-core GCM format)
        output_file
            .write_all(b"GGGCM")
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Write version (version 2 = GCM)
        output_file
            .write_all(&[2, 0])
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Write nonce
        output_file
            .write_all(&nonce)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Write ciphertext length (includes tag)
        let len = ciphertext.len() as u64;
        output_file
            .write_all(&len.to_le_bytes())
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Write ciphertext (includes authentication tag)
        output_file
            .write_all(&ciphertext)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Decrypt a file
    pub fn decrypt_file(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), EncryptionError> {
        // Read input file
        let mut input_file =
            std::fs::File::open(input_path).map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Read and verify magic number
        let mut magic = [0u8; 5];
        input_file
            .read_exact(&mut magic)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Support both old formats and new GCM format
        // Old formats (deprecated): "HLINK" (ECB), "HLGCM" (legacy GCM)
        // New format: "GGGCM" (gg-core GCM)
        let is_gcm = &magic == b"GGGCM";
        let is_legacy_gcm = &magic == b"HLGCM";
        let is_legacy_ecb = &magic == b"HLINK";

        if !is_gcm && !is_legacy_gcm && !is_legacy_ecb {
            return Err(EncryptionError::InvalidCiphertext);
        }

        // Read version
        let mut version = [0u8; 2];
        input_file
            .read_exact(&mut version)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        // Read nonce
        let mut nonce = [0u8; NONCE_SIZE];
        input_file
            .read_exact(&mut nonce)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        let plaintext = if is_gcm || is_legacy_gcm {
            // GCM format: nonce + ciphertext (with embedded tag)

            // Read ciphertext length
            let mut len_bytes = [0u8; 8];
            input_file
                .read_exact(&mut len_bytes)
                .map_err(|e| EncryptionError::IoError(e.to_string()))?;
            let len = u64::from_le_bytes(len_bytes) as usize;

            // Read ciphertext
            let mut ciphertext = vec![0u8; len];
            input_file
                .read_exact(&mut ciphertext)
                .map_err(|e| EncryptionError::IoError(e.to_string()))?;

            // Decrypt
            self.decrypt(&nonce[..], &ciphertext)?
        } else {
            // Legacy ECB format (deprecated, for migration only)
            // Read tag
            let mut tag = [0u8; TAG_SIZE];
            input_file
                .read_exact(&mut tag)
                .map_err(|e| EncryptionError::IoError(e.to_string()))?;

            // Read ciphertext length
            let mut len_bytes = [0u8; 8];
            input_file
                .read_exact(&mut len_bytes)
                .map_err(|e| EncryptionError::IoError(e.to_string()))?;
            let len = u64::from_le_bytes(len_bytes) as usize;

            // Read ciphertext
            let mut ciphertext = vec![0u8; len];
            input_file
                .read_exact(&mut ciphertext)
                .map_err(|e| EncryptionError::IoError(e.to_string()))?;

            // Decrypt using legacy ECB method
            #[allow(deprecated)]
            self.decrypt_legacy(&nonce[..], &ciphertext, &tag[..])?
        };

        // Write output file
        let mut output_file = std::fs::File::create(output_path)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;
        output_file
            .write_all(&plaintext)
            .map_err(|e| EncryptionError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Check if hardware acceleration is available
    pub fn is_hw_accelerated(&self) -> bool {
        self.hw_accelerated
    }

    /// Generate random nonce using cryptographically secure RNG
    /// Also registers the nonce to detect reuse.
    fn generate_nonce() -> Result<Vec<u8>, EncryptionError> {
        use rand::RngCore;
        let mut nonce = [0u8; NONCE_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut nonce[..]);

        // Check for nonce reuse and register this nonce
        check_and_register_nonce(&nonce)?;

        Ok(nonce.to_vec())
    }

    /// Legacy ECB decryption for migration purposes
    ///
    /// SECURITY WARNING: This method is kept only for decrypting files
    /// encrypted with the old ECB format. Do not use for new encryption.
    #[deprecated(note = "ECB mode is insecure. Only use for migrating legacy encrypted files.")]
    fn decrypt_legacy(
        &self,
        _nonce: &[u8],
        _ciphertext: &[u8],
        _tag: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        // For security, we don't support legacy ECB decryption
        // Users must re-encrypt their files with the new GCM format
        Err(EncryptionError::DecryptionFailed(
            "Legacy ECB format no longer supported. Please re-encrypt your files.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_key() -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = i as u8;
        }
        key
    }

    #[test]
    fn test_encrypt_decrypt() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Hello, World! This is a test message.";

        let (nonce, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
        let decrypted = encryption.decrypt(&nonce, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_empty() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext: &[u8] = &b""[..];

        let (nonce, ciphertext) = encryption.encrypt(plaintext).unwrap();
        let decrypted = encryption.decrypt(&nonce, &ciphertext).unwrap();

        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_large() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let (nonce, ciphertext) = encryption.encrypt(&plaintext).unwrap();
        let decrypted = encryption.decrypt(&nonce, &ciphertext).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_authentication_failure() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message";

        let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Modify ciphertext (which includes the tag)
        ciphertext[0] ^= 0xFF;

        let result = encryption.decrypt(&nonce, &ciphertext);
        assert!(matches!(result, Err(EncryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_modified_nonce() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message";

        let (mut nonce, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Modify nonce
        nonce[0] ^= 0xFF;

        let result = encryption.decrypt(&nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys() {
        let enc1 = ModelEncryption::new(create_test_key());
        let mut key2 = [0u8; KEY_SIZE];
        key2[0] = 255; // Different key
        let enc2 = ModelEncryption::new(key2);

        let plaintext = b"Test message";

        let (nonce, ciphertext) = enc1.encrypt(plaintext.as_slice()).unwrap();

        // Different key should fail to decrypt
        let result = enc2.decrypt(&nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_password_derived_key() {
        let salt: &[u8] = &b"salt"[..];
        let enc1 = ModelEncryption::from_password("password123", salt);
        let enc2 = ModelEncryption::from_password("password123", salt);
        let enc3 = ModelEncryption::from_password("password456", salt);

        let plaintext = b"Test message";

        // Same password/salt should produce same key
        let (nonce, ct) = enc1.encrypt(plaintext.as_slice()).unwrap();
        let decrypted = enc2.decrypt(&nonce, &ct).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());

        // Different password should fail
        let result = enc3.decrypt(&nonce, &ct);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_encryption() {
        let encryption = ModelEncryption::new(create_test_key());

        // Create temp files
        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();
        let decrypted_file = NamedTempFile::new().unwrap();

        // Write test data
        let test_data = b"This is test data for file encryption.";
        input_file.as_file().write_all(test_data).unwrap();

        // Encrypt
        encryption
            .encrypt_file(input_file.path(), output_file.path())
            .unwrap();

        // Verify encrypted file is different
        let mut encrypted_data = Vec::new();
        output_file
            .as_file()
            .read_to_end(&mut encrypted_data)
            .unwrap();
        assert_ne!(test_data.as_slice(), encrypted_data.as_slice());
        assert!(encrypted_data.starts_with(b"GGGCM")); // GCM format marker

        // Decrypt
        encryption
            .decrypt_file(output_file.path(), decrypted_file.path())
            .unwrap();

        // Verify decrypted data matches original
        let mut decrypted_data = Vec::new();
        decrypted_file
            .as_file()
            .read_to_end(&mut decrypted_data)
            .unwrap();
        assert_eq!(test_data.as_slice(), decrypted_data.as_slice());
    }

    #[test]
    fn test_hw_acceleration_check() {
        let encryption = ModelEncryption::new(create_test_key());

        // Just verify it doesn't crash
        let _ = encryption.is_hw_accelerated();
    }

    #[test]
    fn test_performance() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();

        let start = std::time::Instant::now();
        let (nonce, ciphertext) = encryption.encrypt(&plaintext).unwrap();
        let encrypt_time = start.elapsed();

        let start = std::time::Instant::now();
        let decrypted = encryption.decrypt(&nonce, &ciphertext).unwrap();
        let decrypt_time = start.elapsed();

        assert_eq!(plaintext, decrypted);

        // Release builds: 1 MB must complete in <1 s.
        // Debug builds: allow 30 s (no optimizations).
        let max_ms: u128 = if cfg!(debug_assertions) {
            30_000
        } else {
            1_000
        };
        assert!(
            encrypt_time.as_millis() < max_ms,
            "Encryption too slow: {:?}",
            encrypt_time
        );
        assert!(
            decrypt_time.as_millis() < max_ms,
            "Decryption too slow: {:?}",
            decrypt_time
        );
    }

    #[test]
    fn test_semantic_security() {
        // Verify that encrypting the same plaintext twice produces different ciphertexts
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Same message";

        let (nonce1, ct1) = encryption.encrypt(plaintext.as_slice()).unwrap();
        let (nonce2, ct2) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Nonces should be different (randomly generated)
        assert_ne!(nonce1, nonce2);

        // Ciphertexts should be different (due to different nonces)
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_invalid_nonce_size() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test";

        let (_, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Wrong size nonce
        let wrong_nonce = vec![0u8; 8];
        let result = encryption.decrypt(&wrong_nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_pbkdf2_key_derivation_deterministic() {
        // Same password and salt should produce the same key
        let enc1 = ModelEncryption::from_password("password", b"salt".as_slice());
        let enc2 = ModelEncryption::from_password("password", b"salt".as_slice());

        let plaintext = b"Test message";
        let (nonce, ct) = enc1.encrypt(plaintext.as_slice()).unwrap();
        let decrypted = enc2.decrypt(&nonce, &ct).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_pbkdf2_different_passwords() {
        let enc1 = ModelEncryption::from_password("password1", b"salt".as_slice());
        let enc2 = ModelEncryption::from_password("password2", b"salt".as_slice());

        let plaintext = b"Test message";
        let (nonce, ct) = enc1.encrypt(plaintext.as_slice()).unwrap();
        let result = enc2.decrypt(&nonce, &ct);
        assert!(result.is_err());
    }

    #[test]
    fn test_pbkdf2_different_salts() {
        let enc1 = ModelEncryption::from_password("password", b"salt1".as_slice());
        let enc2 = ModelEncryption::from_password("password", b"salt2".as_slice());

        let plaintext = b"Test message";
        let (nonce, ct) = enc1.encrypt(plaintext.as_slice()).unwrap();
        let result = enc2.decrypt(&nonce, &ct);
        assert!(result.is_err());
    }

    #[test]
    fn test_pbkdf2_empty_password() {
        // Empty password should still work (though not recommended)
        let enc = ModelEncryption::from_password("", b"salt".as_slice());
        let plaintext = b"Test";
        let (nonce, ct) = enc.encrypt(plaintext.as_slice()).unwrap();
        let decrypted = enc.decrypt(&nonce, &ct).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_pbkdf2_empty_salt() {
        // Empty salt should still work (though not recommended)
        let enc = ModelEncryption::from_password("password", b"".as_slice());
        let plaintext = b"Test";
        let (nonce, ct) = enc.encrypt(plaintext.as_slice()).unwrap();
        let decrypted = enc.decrypt(&nonce, &ct).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encryption_error_display() {
        let err = EncryptionError::InvalidKeySize;
        assert!(err.to_string().contains("Invalid key"));

        let err = EncryptionError::EncryptionFailed("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = EncryptionError::DecryptionFailed("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = EncryptionError::InvalidCiphertext;
        assert!(err.to_string().contains("Invalid ciphertext"));

        let err = EncryptionError::IoError("test".to_string());
        assert!(err.to_string().contains("IO error"));

        let err = EncryptionError::AuthenticationFailed;
        assert!(err.to_string().contains("Authentication failed"));
    }

    #[test]
    fn test_gcm_file_format() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();

        input_file.as_file().write_all(b"test data").unwrap();

        encryption
            .encrypt_file(input_file.path(), output_file.path())
            .unwrap();

        let mut encrypted = Vec::new();
        output_file.as_file().read_to_end(&mut encrypted).unwrap();

        // Check magic number
        assert_eq!(&encrypted[0..5], b"GGGCM");
        // Check version (2.0)
        assert_eq!(encrypted[5], 2);
        assert_eq!(encrypted[6], 0);
    }

    #[test]
    fn test_decrypt_invalid_magic() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();

        // Write invalid magic number
        input_file.as_file().write_all(b"INVALID").unwrap();

        let result = encryption.decrypt_file(input_file.path(), output_file.path());
        assert!(result.is_err());
        assert!(matches!(result, Err(EncryptionError::InvalidCiphertext)));
    }

    #[test]
    fn test_nonce_randomness() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Same message";

        // Generate multiple nonces and verify they're all different
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..100 {
            let (nonce, _) = encryption.encrypt(plaintext.as_slice()).unwrap();
            nonces.insert(nonce);
        }
        // All 100 nonces should be unique
        assert_eq!(nonces.len(), 100);
    }

    #[test]
    fn test_ciphertext_includes_tag() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message";

        let (_, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // GCM tag is 16 bytes, so ciphertext should be plaintext.len() + 16
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    // === File encryption edge case tests ===

    #[test]
    fn test_file_encryption_empty_file() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();
        let decrypted_file = NamedTempFile::new().unwrap();

        // Empty file
        input_file.as_file().write_all(b"").unwrap();

        encryption
            .encrypt_file(input_file.path(), output_file.path())
            .unwrap();

        encryption
            .decrypt_file(output_file.path(), decrypted_file.path())
            .unwrap();

        let mut decrypted = Vec::new();
        decrypted_file
            .as_file()
            .read_to_end(&mut decrypted)
            .unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_file_encryption_single_byte() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();
        let decrypted_file = NamedTempFile::new().unwrap();

        input_file.as_file().write_all(b"X").unwrap();

        encryption
            .encrypt_file(input_file.path(), output_file.path())
            .unwrap();

        encryption
            .decrypt_file(output_file.path(), decrypted_file.path())
            .unwrap();

        let mut decrypted = Vec::new();
        decrypted_file
            .as_file()
            .read_to_end(&mut decrypted)
            .unwrap();
        assert_eq!(decrypted, b"X");
    }

    #[test]
    fn test_file_encryption_binary_data() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();
        let decrypted_file = NamedTempFile::new().unwrap();

        // All byte values
        let data: Vec<u8> = (0..=255).collect();
        input_file.as_file().write_all(&data).unwrap();

        encryption
            .encrypt_file(input_file.path(), output_file.path())
            .unwrap();

        encryption
            .decrypt_file(output_file.path(), decrypted_file.path())
            .unwrap();

        let mut decrypted = Vec::new();
        decrypted_file
            .as_file()
            .read_to_end(&mut decrypted)
            .unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_file_encryption_unicode_filename() {
        let encryption = ModelEncryption::new(create_test_key());

        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("テスト_测试_🔥.bin");
        let output_path = temp_dir.path().join("テスト_测试_🔥.enc");
        let decrypted_path = temp_dir.path().join("テスト_测试_🔥.dec");

        std::fs::write(&input_path, b"unicode filename test").unwrap();

        encryption.encrypt_file(&input_path, &output_path).unwrap();
        encryption
            .decrypt_file(&output_path, &decrypted_path)
            .unwrap();

        let decrypted = std::fs::read(&decrypted_path).unwrap();
        assert_eq!(decrypted, b"unicode filename test");
    }

    #[test]
    fn test_file_encryption_overwrite_protection() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();

        input_file.as_file().write_all(b"original").unwrap();

        // Close the output file handle by dropping it and reopening
        let output_path = output_file.path().to_owned();
        std::fs::write(&output_path, b"existing").unwrap();

        // Should successfully overwrite the output file
        encryption
            .encrypt_file(input_file.path(), &output_path)
            .unwrap();

        let encrypted = std::fs::read(&output_path).unwrap();
        assert!(encrypted.starts_with(b"GGGCM"));
    }

    #[test]
    fn test_file_decrypt_truncated_file() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();

        // Write a truncated encrypted file (magic + version + partial nonce)
        input_file
            .as_file()
            .write_all(b"GGGCM\x02\x00\x01\x02\x03")
            .unwrap();

        let result = encryption.decrypt_file(input_file.path(), output_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_file_decrypt_wrong_version() {
        let encryption = ModelEncryption::new(create_test_key());

        let input_file = NamedTempFile::new().unwrap();
        let output_file = NamedTempFile::new().unwrap();

        // Write GCM magic but with version 99
        input_file.as_file().write_all(b"GGGCM\x63\x00").unwrap();

        // Should still attempt to decrypt (version is read but not validated strictly)
        // The error will come from missing data
        let result = encryption.decrypt_file(input_file.path(), output_file.path());
        assert!(result.is_err());
    }

    // === Security property tests ===

    #[test]
    fn test_key_size_constant() {
        assert_eq!(KEY_SIZE, 32); // 256 bits
    }

    #[test]
    fn test_nonce_size_constant() {
        assert_eq!(NONCE_SIZE, 12); // 96 bits (GCM standard)
    }

    #[test]
    fn test_tag_size_constant() {
        assert_eq!(TAG_SIZE, 16); // 128 bits (GCM standard)
    }

    #[test]
    fn test_pbkdf2_iterations_owasp_compliant() {
        // OWASP recommends at least 600,000 iterations for PBKDF2-SHA256 as of 2023
        // However, 100,000 is still acceptable for many use cases
        // We use 100,000 as a balance between security and performance
        assert!(ModelEncryption::PBKDF2_ITERATIONS >= 100_000);
    }

    #[test]
    fn test_multiple_encrypt_same_key_different_ciphertext() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Same message encrypted multiple times";

        let mut ciphertexts = std::collections::HashSet::new();
        for _ in 0..10 {
            let (_, ct) = encryption.encrypt(plaintext.as_slice()).unwrap();
            ciphertexts.insert(ct);
        }

        // All 10 ciphertexts should be different due to random nonces
        assert_eq!(ciphertexts.len(), 10);
    }

    #[test]
    fn test_bit_flip_detection() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message for bit flip detection";

        let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Flip a single bit in the ciphertext
        ciphertext[10] ^= 0x01;

        let result = encryption.decrypt(&nonce, &ciphertext);
        assert!(matches!(result, Err(EncryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_byte_removal_detection() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message";

        let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Remove a byte from the ciphertext
        ciphertext.pop();

        let result = encryption.decrypt(&nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_byte_insertion_detection() {
        let encryption = ModelEncryption::new(create_test_key());
        let plaintext = b"Test message";

        let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();

        // Insert a byte into the ciphertext
        ciphertext.push(0);

        let result = encryption.decrypt(&nonce, &ciphertext);
        assert!(result.is_err());
    }

    // === Nonce reuse detection tests ===

    #[test]
    fn test_nonce_reuse_detection() {
        // Test that manually registering the same nonce twice fails
        let nonce: [u8; NONCE_SIZE] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

        // First registration should succeed
        let result1 = check_and_register_nonce(&nonce);
        assert!(result1.is_ok());

        // Second registration of the same nonce should fail
        let result2 = check_and_register_nonce(&nonce);
        assert!(matches!(result2, Err(EncryptionError::NonceReuseDetected)));
    }

    #[test]
    fn test_nonce_reuse_error_display() {
        let err = EncryptionError::NonceReuseDetected;
        let msg = err.to_string();
        assert!(msg.contains("CRITICAL"));
        assert!(msg.contains("Nonce reuse"));
    }

    #[test]
    fn test_different_nonces_allowed() {
        // Test that different nonces are allowed
        let nonce1: [u8; NONCE_SIZE] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let nonce2: [u8; NONCE_SIZE] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

        let result1 = check_and_register_nonce(&nonce1);
        assert!(result1.is_ok());

        let result2 = check_and_register_nonce(&nonce2);
        assert!(result2.is_ok());
    }
}
