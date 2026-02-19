//! Tokenization wrapper for model-agnostic token handling.
//!
//! Provides encode/decode via the GGUF backend when the `gguf` feature
//! is enabled, falling back to a no-op stub for other builds.

use thiserror::Error;

#[cfg(feature = "gguf")]
use std::sync::Arc;

#[cfg(feature = "gguf")]
use crate::engine::gguf::LlamaBackendInner;

#[derive(Error, Debug)]
pub enum TokenizerError {
    #[error("Tokenizer not loaded")]
    NotLoaded,

    #[error("Encoding failed: {0}")]
    EncodingFailed(String),

    #[error("Decoding failed: {0}")]
    DecodingFailed(String),

    #[error("Invalid token ID: {0}")]
    InvalidToken(u32),
}

/// Wrapper around model-specific tokenizer.
///
/// When built with `gguf`, delegates to `LlamaBackendInner` for
/// real BPE tokenization. Otherwise provides a no-op stub.
pub struct TokenizerWrapper {
    vocab_size: u32,
    eos_token: u32,
    bos_token: u32,
    #[cfg(feature = "gguf")]
    backend: Option<Arc<LlamaBackendInner>>,
}

impl TokenizerWrapper {
    /// Create a stub tokenizer without a backing model.
    pub fn new(vocab_size: u32, eos_token: u32, bos_token: u32) -> Self {
        Self {
            vocab_size,
            eos_token,
            bos_token,
            #[cfg(feature = "gguf")]
            backend: None,
        }
    }

    /// Create a tokenizer backed by a loaded GGUF backend.
    #[cfg(feature = "gguf")]
    pub fn with_backend(
        backend: Arc<LlamaBackendInner>,
        vocab_size: u32,
        eos_token: u32,
        bos_token: u32,
    ) -> Self {
        Self {
            vocab_size,
            eos_token,
            bos_token,
            backend: Some(backend),
        }
    }

    /// Encode text to token IDs.
    ///
    /// When a backend is loaded, uses llama-cpp-2 tokenization
    /// (BOS is prepended by the backend). Returns empty vec otherwise.
    pub fn encode(&self, text: &str) -> Result<Vec<u32>, TokenizerError> {
        #[cfg(feature = "gguf")]
        if let Some(be) = &self.backend {
            return encode_via_backend(be, text);
        }
        let _ = text;
        Ok(Vec::new())
    }

    /// Decode token IDs back to text.
    ///
    /// When a backend is loaded, uses llama-cpp-2 detokenization.
    /// Returns empty string otherwise.
    pub fn decode(&self, tokens: &[u32]) -> Result<String, TokenizerError> {
        self.validate_tokens(tokens)?;

        #[cfg(feature = "gguf")]
        if let Some(be) = &self.backend {
            return decode_via_backend(be, tokens);
        }
        Ok(String::new())
    }

    pub fn eos_token(&self) -> u32 {
        self.eos_token
    }

    pub fn bos_token(&self) -> u32 {
        self.bos_token
    }

    pub fn vocab_size(&self) -> u32 {
        self.vocab_size
    }

    /// Check if token is end-of-sequence.
    pub fn is_eos(&self, token: u32) -> bool {
        token == self.eos_token
    }

    /// Returns true if a real model tokenizer is available.
    pub fn has_model(&self) -> bool {
        #[cfg(feature = "gguf")]
        {
            return self.backend.is_some();
        }
        #[cfg(not(feature = "gguf"))]
        false
    }

    fn validate_tokens(&self, tokens: &[u32]) -> Result<(), TokenizerError> {
        for &token in tokens {
            if token >= self.vocab_size {
                return Err(TokenizerError::InvalidToken(token));
            }
        }
        Ok(())
    }
}

/// Encode text via the GGUF backend, converting LlamaToken to u32.
#[cfg(feature = "gguf")]
fn encode_via_backend(
    be: &LlamaBackendInner,
    text: &str,
) -> Result<Vec<u32>, TokenizerError> {
    let llama_tokens = be
        .tokenize(text)
        .map_err(|e| TokenizerError::EncodingFailed(e.to_string()))?;
    Ok(llama_tokens.iter().map(|t| t.0 as u32).collect())
}

/// Decode u32 token IDs via the GGUF backend.
#[cfg(feature = "gguf")]
fn decode_via_backend(
    be: &LlamaBackendInner,
    tokens: &[u32],
) -> Result<String, TokenizerError> {
    use llama_cpp_2::token::LlamaToken;
    let llama_tokens: Vec<LlamaToken> =
        tokens.iter().map(|&t| LlamaToken(t as i32)).collect();
    be.detokenize(&llama_tokens)
        .map_err(|e| TokenizerError::DecodingFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_encode_returns_empty() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        let tokens = tw.encode("hello world").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn stub_decode_returns_empty() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        let text = tw.decode(&[1, 5, 10]).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn decode_rejects_invalid_token() {
        let tw = TokenizerWrapper::new(100, 2, 1);
        let result = tw.decode(&[50, 200]);
        assert!(result.is_err());
        match result.unwrap_err() {
            TokenizerError::InvalidToken(id) => assert_eq!(id, 200),
            other => panic!("expected InvalidToken, got: {other}"),
        }
    }

    #[test]
    fn is_eos_detects_correctly() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        assert!(tw.is_eos(2));
        assert!(!tw.is_eos(1));
        assert!(!tw.is_eos(0));
    }

    #[test]
    fn special_token_accessors() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        assert_eq!(tw.eos_token(), 2);
        assert_eq!(tw.bos_token(), 1);
        assert_eq!(tw.vocab_size(), 32000);
    }

    #[test]
    fn has_model_false_for_stub() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        assert!(!tw.has_model());
    }

    #[test]
    fn decode_empty_tokens_succeeds() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        let text = tw.decode(&[]).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn encode_empty_string_succeeds() {
        let tw = TokenizerWrapper::new(32000, 2, 1);
        let tokens = tw.encode("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn decode_boundary_token_valid() {
        let tw = TokenizerWrapper::new(100, 2, 1);
        // Token 99 is the last valid ID (vocab_size=100 means 0..99)
        let result = tw.decode(&[99]);
        assert!(result.is_ok());
    }

    #[test]
    fn decode_boundary_token_invalid() {
        let tw = TokenizerWrapper::new(100, 2, 1);
        // Token 100 is out of range
        let result = tw.decode(&[100]);
        assert!(result.is_err());
    }
}
