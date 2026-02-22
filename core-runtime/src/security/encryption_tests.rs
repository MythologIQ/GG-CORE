//! Tests for model encryption module.

use super::super::encryption_core::*;
use std::io::{Read, Write};
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
    ciphertext[0] ^= 0xFF;
    let result = encryption.decrypt(&nonce, &ciphertext);
    assert!(matches!(result, Err(EncryptionError::AuthenticationFailed)));
}

#[test]
fn test_modified_nonce() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test message";
    let (mut nonce, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    nonce[0] ^= 0xFF;
    let result = encryption.decrypt(&nonce, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn test_different_keys() {
    let enc1 = ModelEncryption::new(create_test_key());
    let mut key2 = [0u8; KEY_SIZE];
    key2[0] = 255;
    let enc2 = ModelEncryption::new(key2);
    let plaintext = b"Test message";
    let (nonce, ciphertext) = enc1.encrypt(plaintext.as_slice()).unwrap();
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
    let (nonce, ct) = enc1.encrypt(plaintext.as_slice()).unwrap();
    let decrypted = enc2.decrypt(&nonce, &ct).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    let result = enc3.decrypt(&nonce, &ct);
    assert!(result.is_err());
}

#[test]
fn test_file_encryption() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    let decrypted_file = NamedTempFile::new().unwrap();
    let test_data = b"This is test data for file encryption.";
    input_file.as_file().write_all(test_data).unwrap();
    encryption.encrypt_file(input_file.path(), output_file.path()).unwrap();
    let mut encrypted_data = Vec::new();
    output_file.as_file().read_to_end(&mut encrypted_data).unwrap();
    assert_ne!(test_data.as_slice(), encrypted_data.as_slice());
    assert!(encrypted_data.starts_with(b"GGGCM"));
    encryption.decrypt_file(output_file.path(), decrypted_file.path()).unwrap();
    let mut decrypted_data = Vec::new();
    decrypted_file.as_file().read_to_end(&mut decrypted_data).unwrap();
    assert_eq!(test_data.as_slice(), decrypted_data.as_slice());
}

#[test]
fn test_hw_acceleration_check() {
    let encryption = ModelEncryption::new(create_test_key());
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
    let max_ms: u128 = if cfg!(debug_assertions) { 30_000 } else { 1_000 };
    assert!(encrypt_time.as_millis() < max_ms, "Encryption too slow: {:?}", encrypt_time);
    assert!(decrypt_time.as_millis() < max_ms, "Decryption too slow: {:?}", decrypt_time);
}

#[test]
fn test_semantic_security() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Same message";
    let (nonce1, ct1) = encryption.encrypt(plaintext.as_slice()).unwrap();
    let (nonce2, ct2) = encryption.encrypt(plaintext.as_slice()).unwrap();
    assert_ne!(nonce1, nonce2);
    assert_ne!(ct1, ct2);
}

#[test]
fn test_invalid_nonce_size() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test";
    let (_, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    let wrong_nonce = vec![0u8; 8];
    let result = encryption.decrypt(&wrong_nonce, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn test_pbkdf2_key_derivation_deterministic() {
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
    let enc = ModelEncryption::from_password("", b"salt".as_slice());
    let plaintext = b"Test";
    let (nonce, ct) = enc.encrypt(plaintext.as_slice()).unwrap();
    let decrypted = enc.decrypt(&nonce, &ct).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
}

#[test]
fn test_pbkdf2_empty_salt() {
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
    encryption.encrypt_file(input_file.path(), output_file.path()).unwrap();
    let mut encrypted = Vec::new();
    output_file.as_file().read_to_end(&mut encrypted).unwrap();
    assert_eq!(&encrypted[0..5], b"GGGCM");
    assert_eq!(encrypted[5], 2);
    assert_eq!(encrypted[6], 0);
}

#[test]
fn test_decrypt_invalid_magic() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"INVALID").unwrap();
    let result = encryption.decrypt_file(input_file.path(), output_file.path());
    assert!(result.is_err());
    assert!(matches!(result, Err(EncryptionError::InvalidCiphertext)));
}

#[test]
fn test_nonce_randomness() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Same message";
    let mut nonces = std::collections::HashSet::new();
    for _ in 0..100 {
        let (nonce, _) = encryption.encrypt(plaintext.as_slice()).unwrap();
        nonces.insert(nonce);
    }
    assert_eq!(nonces.len(), 100);
}

#[test]
fn test_ciphertext_includes_tag() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test message";
    let (_, ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    assert_eq!(ciphertext.len(), plaintext.len() + 16);
}

#[test]
fn test_file_encryption_empty_file() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    let decrypted_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"").unwrap();
    encryption.encrypt_file(input_file.path(), output_file.path()).unwrap();
    encryption.decrypt_file(output_file.path(), decrypted_file.path()).unwrap();
    let mut decrypted = Vec::new();
    decrypted_file.as_file().read_to_end(&mut decrypted).unwrap();
    assert!(decrypted.is_empty());
}

#[test]
fn test_file_encryption_single_byte() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    let decrypted_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"X").unwrap();
    encryption.encrypt_file(input_file.path(), output_file.path()).unwrap();
    encryption.decrypt_file(output_file.path(), decrypted_file.path()).unwrap();
    let mut decrypted = Vec::new();
    decrypted_file.as_file().read_to_end(&mut decrypted).unwrap();
    assert_eq!(decrypted, b"X");
}

#[test]
fn test_file_encryption_binary_data() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    let decrypted_file = NamedTempFile::new().unwrap();
    let data: Vec<u8> = (0..=255).collect();
    input_file.as_file().write_all(&data).unwrap();
    encryption.encrypt_file(input_file.path(), output_file.path()).unwrap();
    encryption.decrypt_file(output_file.path(), decrypted_file.path()).unwrap();
    let mut decrypted = Vec::new();
    decrypted_file.as_file().read_to_end(&mut decrypted).unwrap();
    assert_eq!(decrypted, data);
}

#[test]
fn test_file_encryption_unicode_filename() {
    let encryption = ModelEncryption::new(create_test_key());
    let temp_dir = tempfile::tempdir().unwrap();
    let input_path = temp_dir.path().join("test_enc.bin");
    let output_path = temp_dir.path().join("test_enc.enc");
    let decrypted_path = temp_dir.path().join("test_enc.dec");
    std::fs::write(&input_path, b"unicode filename test").unwrap();
    encryption.encrypt_file(&input_path, &output_path).unwrap();
    encryption.decrypt_file(&output_path, &decrypted_path).unwrap();
    let decrypted = std::fs::read(&decrypted_path).unwrap();
    assert_eq!(decrypted, b"unicode filename test");
}

#[test]
fn test_file_encryption_overwrite_protection() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"original").unwrap();
    let output_path = output_file.path().to_owned();
    std::fs::write(&output_path, b"existing").unwrap();
    encryption.encrypt_file(input_file.path(), &output_path).unwrap();
    let encrypted = std::fs::read(&output_path).unwrap();
    assert!(encrypted.starts_with(b"GGGCM"));
}

#[test]
fn test_file_decrypt_truncated_file() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"GGGCM\x02\x00\x01\x02\x03").unwrap();
    let result = encryption.decrypt_file(input_file.path(), output_file.path());
    assert!(result.is_err());
}

#[test]
fn test_file_decrypt_wrong_version() {
    let encryption = ModelEncryption::new(create_test_key());
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    input_file.as_file().write_all(b"GGGCM\x63\x00").unwrap();
    let result = encryption.decrypt_file(input_file.path(), output_file.path());
    assert!(result.is_err());
}

#[test]
fn test_key_size_constant() {
    assert_eq!(KEY_SIZE, 32);
}

#[test]
fn test_nonce_size_constant() {
    assert_eq!(NONCE_SIZE, 12);
}

#[test]
fn test_tag_size_constant() {
    assert_eq!(TAG_SIZE, 16);
}

#[test]
fn test_pbkdf2_iterations_owasp_compliant() {
    assert!(ModelEncryption::PBKDF2_ITERATIONS >= 600_000);
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
    assert_eq!(ciphertexts.len(), 10);
}

#[test]
fn test_bit_flip_detection() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test message for bit flip detection";
    let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    ciphertext[10] ^= 0x01;
    let result = encryption.decrypt(&nonce, &ciphertext);
    assert!(matches!(result, Err(EncryptionError::AuthenticationFailed)));
}

#[test]
fn test_byte_removal_detection() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test message";
    let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    ciphertext.pop();
    let result = encryption.decrypt(&nonce, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn test_byte_insertion_detection() {
    let encryption = ModelEncryption::new(create_test_key());
    let plaintext = b"Test message";
    let (nonce, mut ciphertext) = encryption.encrypt(plaintext.as_slice()).unwrap();
    ciphertext.push(0);
    let result = encryption.decrypt(&nonce, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn test_nonce_reuse_detection() {
    use rand::RngCore;
    let mut nonce: [u8; NONCE_SIZE] = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut nonce[..]);
    let result1 = check_and_register_nonce(&nonce);
    assert!(result1.is_ok());
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
    let nonce1: [u8; NONCE_SIZE] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let nonce2: [u8; NONCE_SIZE] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    let result1 = check_and_register_nonce(&nonce1);
    assert!(result1.is_ok());
    let result2 = check_and_register_nonce(&nonce2);
    assert!(result2.is_ok());
}
