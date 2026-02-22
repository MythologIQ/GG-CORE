//! File I/O helpers for model encryption.

use std::io::{Read, Write};
use std::path::Path;

use super::encryption_core::{EncryptionError, ModelEncryption, NONCE_SIZE, TAG_SIZE};

/// Read file bytes for encryption.
pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>, EncryptionError> {
    let mut file = std::fs::File::open(path).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    Ok(data)
}

/// Write encrypted file with GGGCM header.
pub fn write_encrypted_file(path: &Path, nonce: &[u8], ct: &[u8]) -> Result<(), EncryptionError> {
    let mut out = std::fs::File::create(path).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    out.write_all(b"GGGCM").map_err(|e| EncryptionError::IoError(e.to_string()))?;
    out.write_all(&[2, 0]).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    out.write_all(nonce).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    let len = ct.len() as u64;
    out.write_all(&len.to_le_bytes()).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    out.write_all(ct).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    Ok(())
}

/// Read and decrypt an encrypted file.
pub fn read_and_decrypt_file(enc: &ModelEncryption, path: &Path) -> Result<Vec<u8>, EncryptionError> {
    let mut file = std::fs::File::open(path).map_err(|e| EncryptionError::IoError(e.to_string()))?;

    let mut magic = [0u8; 5];
    file.read_exact(&mut magic).map_err(|e| EncryptionError::IoError(e.to_string()))?;

    let is_gcm = &magic == b"GGGCM";
    let is_legacy_gcm = &magic == b"HLGCM";
    let is_legacy_ecb = &magic == b"HLINK";

    if !is_gcm && !is_legacy_gcm && !is_legacy_ecb {
        return Err(EncryptionError::InvalidCiphertext);
    }

    let mut version = [0u8; 2];
    file.read_exact(&mut version).map_err(|e| EncryptionError::IoError(e.to_string()))?;

    let mut nonce = [0u8; NONCE_SIZE];
    file.read_exact(&mut nonce).map_err(|e| EncryptionError::IoError(e.to_string()))?;

    if is_gcm || is_legacy_gcm {
        read_gcm_payload(enc, &mut file, &nonce)
    } else {
        read_legacy_ecb_payload(enc, &mut file, &nonce)
    }
}

/// Read GCM payload and decrypt.
fn read_gcm_payload(
    enc: &ModelEncryption,
    file: &mut std::fs::File,
    nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>, EncryptionError> {
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    let len = u64::from_le_bytes(len_bytes) as usize;
    let mut ciphertext = vec![0u8; len];
    file.read_exact(&mut ciphertext).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    enc.decrypt(&nonce[..], &ciphertext)
}

/// Read legacy ECB payload (deprecated).
fn read_legacy_ecb_payload(
    enc: &ModelEncryption,
    file: &mut std::fs::File,
    nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>, EncryptionError> {
    let mut tag = [0u8; TAG_SIZE];
    file.read_exact(&mut tag).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    let len = u64::from_le_bytes(len_bytes) as usize;
    let mut ciphertext = vec![0u8; len];
    file.read_exact(&mut ciphertext).map_err(|e| EncryptionError::IoError(e.to_string()))?;
    #[allow(deprecated)]
    enc.decrypt_legacy(&nonce[..], &ciphertext, &tag[..])
}
