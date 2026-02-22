//! Key derivation and salt management for model encryption.
//!
//! Provides PBKDF2-HMAC-SHA256 key derivation and installation-specific
//! salt generation for machine-bound encryption.

use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use zeroize::Zeroize;

use super::encryption_core::{EncryptionError, ModelEncryption, KEY_SIZE};

/// Minimum salt size for security (16 bytes = 128 bits)
pub const MIN_SALT_SIZE: usize = 16;
/// Default salt file name
const SALT_FILE_NAME: &str = ".gg-core-salt";

/// Global installation salt (generated once, cached)
static INSTALLATION_SALT: OnceLock<Vec<u8>> = OnceLock::new();

/// Get or create the installation-specific salt.
///
/// The salt is stored in a file within the application's data directory.
/// If the file doesn't exist, a new cryptographically random salt is generated.
pub fn get_or_create_installation_salt() -> Result<Vec<u8>, EncryptionError> {
    if let Some(salt) = INSTALLATION_SALT.get() {
        return Ok(salt.clone());
    }

    let salt_path = get_salt_file_path()?;

    let salt = if salt_path.exists() {
        let existing = std::fs::read(&salt_path)
            .map_err(|e| EncryptionError::IoError(format!("Failed to read salt file: {}", e)))?;

        if existing.len() >= MIN_SALT_SIZE {
            existing
        } else {
            generate_and_save_salt(&salt_path)?
        }
    } else {
        generate_and_save_salt(&salt_path)?
    };

    let _ = INSTALLATION_SALT.set(salt.clone());
    Ok(salt)
}

/// Generate a new salt and save it to disk.
fn generate_and_save_salt(salt_path: &Path) -> Result<Vec<u8>, EncryptionError> {
    let salt = generate_random_salt();

    if let Some(parent) = salt_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            EncryptionError::IoError(format!("Failed to create salt directory: {}", e))
        })?;
    }

    write_salt_file(salt_path, &salt)?;
    Ok(salt)
}

/// Get the path to the salt file.
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
        .mode(0o600)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, salt))
        .map_err(|e| EncryptionError::IoError(format!("Failed to write salt file: {}", e)))
}

/// Create encryption handler from a password (derived key).
///
/// Uses PBKDF2-HMAC-SHA256 for secure key derivation.
pub fn derive_key_from_password(password: &str, salt: &[u8], iterations: u32) -> ModelEncryption {
    let mut key = [0u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut key[..]);
    let result = ModelEncryption::new(key);
    key.zeroize();
    result
}

/// Generate a key from machine-specific identifiers (Windows).
#[cfg(target_os = "windows")]
pub fn from_machine_id(iterations: u32) -> Result<ModelEncryption, EncryptionError> {
    use std::process::Command;

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
            extract_machine_guid(&stdout)?
        }
        Err(e) => {
            return Err(EncryptionError::EncryptionFailed(format!(
                "Failed to query registry: {}",
                e
            )))
        }
    };

    let salt = get_or_create_installation_salt()?;
    Ok(derive_key_from_password(&machine_id, &salt, iterations))
}

/// Extract GUID from Windows registry output.
#[cfg(target_os = "windows")]
fn extract_machine_guid(stdout: &str) -> Result<String, EncryptionError> {
    if let Some(pos) = stdout.find("MachineGuid") {
        let rest = &stdout[pos..];
        if let Some(start) = rest.find("REG_SZ") {
            let guid_part = &rest[start + 6..];
            return Ok(guid_part.trim().to_string());
        }
    }
    Err(EncryptionError::EncryptionFailed(
        "Could not parse machine GUID".to_string(),
    ))
}

/// Generate a key from machine-specific identifiers (non-Windows).
#[cfg(not(target_os = "windows"))]
pub fn from_machine_id(iterations: u32) -> Result<ModelEncryption, EncryptionError> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .map_err(|e| EncryptionError::EncryptionFailed(format!("Hostname error: {}", e)))?;

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .map_err(|_| {
            EncryptionError::EncryptionFailed("Could not determine user".to_string())
        })?;

    let combined = format!("{}-{}", hostname, user);
    let salt = get_or_create_installation_salt()?;
    Ok(derive_key_from_password(&combined, &salt, iterations))
}
