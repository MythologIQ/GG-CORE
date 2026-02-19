// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Chaos & Resilience - Streams, Models, Tokenizer
//!
//! Token stream interruptions, model loading failures,
//! and tokenizer boundary condition tests.

use std::time::Duration;

use veritas_sdr::engine::{TokenStream, TokenizerWrapper};
use veritas_sdr::models::{LoadError, ModelLoader};

// ============================================================================
// Token Stream Interruptions
// ============================================================================

#[tokio::test]
async fn chaos_stream_sender_dropped_mid_stream() {
    let (sender, mut stream) = TokenStream::new(16);
    sender.send(1, false).await.unwrap();
    sender.send(2, false).await.unwrap();
    sender.close();
    assert_eq!(stream.next().await.unwrap().token, 1);
    assert_eq!(stream.next().await.unwrap().token, 2);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn chaos_stream_receiver_dropped_mid_stream() {
    let (sender, stream) = TokenStream::new(4);
    sender.send(1, false).await.unwrap();
    drop(stream);
    assert!(sender.send(2, false).await.is_err());
}

#[tokio::test]
async fn chaos_stream_buffer_overflow() {
    let (sender, mut stream) = TokenStream::new(2);
    sender.send(1, false).await.unwrap();
    sender.send(2, false).await.unwrap();
    let handle = tokio::spawn(async move { sender.send(3, false).await });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(stream.next().await.is_some());
    let result = tokio::time::timeout(Duration::from_millis(100), handle).await;
    assert!(result.is_ok(), "Send should complete after drain");
}

#[tokio::test]
async fn chaos_stream_collect_with_no_final_token() {
    let (sender, stream) = TokenStream::new(16);
    tokio::spawn(async move {
        sender.send(1, false).await.unwrap();
        sender.send(2, false).await.unwrap();
    });
    assert_eq!(stream.collect().await, vec![1, 2]);
}

#[tokio::test]
async fn chaos_stream_rapid_send_close() {
    for _ in 0..100 {
        let (sender, stream) = TokenStream::new(4);
        sender.send(42, true).await.unwrap();
        assert_eq!(stream.collect().await, vec![42]);
    }
}

// ============================================================================
// Model Loading Failures
// ============================================================================

#[test]
fn chaos_model_nonexistent_path() {
    let loader = ModelLoader::new(std::path::PathBuf::from("/nonexistent/base"));
    let result = loader.validate_path("models/ghost.gguf");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), LoadError::NotFound(_)));
}

#[test]
fn chaos_model_path_traversal_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let loader = ModelLoader::new(temp.path().to_path_buf());
    let attacks = vec![
        "../../../etc/passwd",
        "models/../../etc/shadow",
        "models/../../../windows/system32/config/sam",
        "..\\..\\..\\etc\\passwd",
        "models/./../../secret",
    ];
    for path in attacks {
        assert!(loader.validate_path(path).is_err(), "'{}' should be blocked", path);
    }
}

#[test]
fn chaos_model_empty_path() {
    let temp = tempfile::tempdir().unwrap();
    let loader = ModelLoader::new(temp.path().to_path_buf());
    assert!(loader.validate_path("").is_err());
}

#[test]
fn chaos_model_outside_allowed_dirs() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("secrets.txt"), "secret").unwrap();
    let loader = ModelLoader::new(temp.path().to_path_buf());
    assert!(loader.validate_path("secrets.txt").is_err());
}

#[test]
fn chaos_model_valid_path_in_models_dir() {
    let temp = tempfile::tempdir().unwrap();
    let models_dir = temp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    std::fs::write(models_dir.join("test.gguf"), "fake").unwrap();
    let loader = ModelLoader::new(temp.path().to_path_buf());
    assert!(loader.validate_path("models/test.gguf").is_ok());
}

#[test]
fn chaos_model_metadata_empty_file() {
    let temp = tempfile::tempdir().unwrap();
    let models_dir = temp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    std::fs::write(models_dir.join("empty.gguf"), "").unwrap();
    let loader = ModelLoader::new(temp.path().to_path_buf());
    let path = loader.validate_path("models/empty.gguf").unwrap();
    let meta = loader.load_metadata(&path).unwrap();
    assert_eq!(meta.size_bytes, 0);
    assert_eq!(meta.name, "empty");
}

// ============================================================================
// Tokenizer Boundary Conditions
// ============================================================================

#[test]
fn chaos_tokenizer_vocab_boundary() {
    let tw = TokenizerWrapper::new(100, 2, 1);
    assert!(tw.decode(&[99]).is_ok());
    assert!(tw.decode(&[100]).is_err());
    assert!(tw.decode(&[u32::MAX]).is_err());
}

#[test]
fn chaos_tokenizer_empty_operations() {
    let tw = TokenizerWrapper::new(32000, 2, 1);
    assert!(tw.encode("").unwrap().is_empty());
    assert!(tw.decode(&[]).unwrap().is_empty());
}

#[test]
fn chaos_tokenizer_special_tokens() {
    let tw = TokenizerWrapper::new(32000, 2, 1);
    assert!(tw.is_eos(2));
    assert!(!tw.is_eos(1));
    assert!(!tw.is_eos(0));
    assert_eq!(tw.bos_token(), 1);
    assert_eq!(tw.eos_token(), 2);
}

#[test]
fn chaos_tokenizer_all_invalid_sequence() {
    let tw = TokenizerWrapper::new(10, 2, 1);
    assert!(tw.decode(&[5, 11, 3]).is_err());
}
