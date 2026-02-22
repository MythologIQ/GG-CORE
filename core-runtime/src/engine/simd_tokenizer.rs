//! SIMD-accelerated tokenizer for fast text processing.
//!
//! Provides AVX2-accelerated whitespace detection and greedy BPE encoding.

use crate::engine::TokenizerError;
use std::collections::HashMap;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Token vocabulary entry.
struct TokenEntry {
    bytes: Vec<u8>,
    #[allow(dead_code)]
    id: u32,
}

/// SIMD-accelerated tokenizer for fast text processing.
pub struct SimdTokenizer {
    vocab: Vec<TokenEntry>,
    vocab_map: HashMap<Vec<u8>, u32>,
    eos_token: u32,
    bos_token: u32,
}

impl SimdTokenizer {
    /// Create tokenizer from vocabulary bytes.
    /// Format: one token per line, tokens are raw bytes.
    pub fn from_vocab(
        vocab_bytes: &[u8],
        eos_token: u32,
        bos_token: u32,
    ) -> Result<Self, TokenizerError> {
        let mut vocab = Vec::new();
        let mut vocab_map = HashMap::new();

        for (id, line) in vocab_bytes.split(|&b| b == b'\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let token_bytes = line.to_vec();
            vocab_map.insert(token_bytes.clone(), id as u32);
            vocab.push(TokenEntry {
                bytes: token_bytes,
                id: id as u32,
            });
        }

        Ok(Self { vocab, vocab_map, eos_token, bos_token })
    }

    /// Find whitespace positions using AVX2 SIMD (x86_64 only).
    ///
    /// # Safety
    /// Caller must verify AVX2 support via `is_x86_feature_detected!("avx2")`.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    unsafe fn find_whitespace_avx2(text: &[u8]) -> Vec<usize> {
        let mut positions = Vec::new();
        let space = _mm256_set1_epi8(b' ' as i8);
        let newline = _mm256_set1_epi8(b'\n' as i8);
        let tab = _mm256_set1_epi8(b'\t' as i8);

        let chunks = text.chunks_exact(32);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            let data = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let is_space = _mm256_cmpeq_epi8(data, space);
            let is_newline = _mm256_cmpeq_epi8(data, newline);
            let is_tab = _mm256_cmpeq_epi8(data, tab);
            let is_ws = _mm256_or_si256(_mm256_or_si256(is_space, is_newline), is_tab);
            let mask = _mm256_movemask_epi8(is_ws) as u32;
            let base = chunk_idx * 32;
            Self::extract_positions(mask, base, &mut positions);
        }

        Self::find_whitespace_remainder(remainder, text.len(), &mut positions);
        positions
    }

    /// Extract bit positions from mask (helper for SIMD).
    #[cfg(target_arch = "x86_64")]
    fn extract_positions(mask: u32, base: usize, positions: &mut Vec<usize>) {
        for bit in 0..32 {
            if (mask >> bit) & 1 == 1 {
                positions.push(base + bit);
            }
        }
    }

    /// Handle remainder bytes after SIMD chunks.
    fn find_whitespace_remainder(remainder: &[u8], total_len: usize, positions: &mut Vec<usize>) {
        let base = total_len - remainder.len();
        for (i, &b) in remainder.iter().enumerate() {
            if b == b' ' || b == b'\n' || b == b'\t' {
                positions.push(base + i);
            }
        }
    }

    /// Find whitespace positions (scalar fallback).
    fn find_whitespace_scalar(text: &[u8]) -> Vec<usize> {
        text.iter()
            .enumerate()
            .filter(|(_, &b)| b == b' ' || b == b'\n' || b == b'\t')
            .map(|(i, _)| i)
            .collect()
    }

    /// Find whitespace with automatic dispatch.
    pub fn find_whitespace(text: &[u8]) -> Vec<usize> {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                // SAFETY: AVX2 feature is detected before calling
                return unsafe { Self::find_whitespace_avx2(text) };
            }
        }
        Self::find_whitespace_scalar(text)
    }

    /// Encode text to token IDs using greedy BPE.
    pub fn encode(&self, text: &str) -> Vec<u32> {
        let bytes = text.as_bytes();
        let mut tokens = Vec::with_capacity(bytes.len() / 4);
        let mut pos = 0;

        while pos < bytes.len() {
            let (best_len, best_id) = self.find_longest_match(bytes, pos);
            tokens.push(best_id);
            pos += best_len;
        }

        tokens
    }

    /// Find longest matching token from position.
    fn find_longest_match(&self, bytes: &[u8], pos: usize) -> (usize, u32) {
        let max_len = (bytes.len() - pos).min(16);

        for len in (1..=max_len).rev() {
            if let Some(&id) = self.vocab_map.get(&bytes[pos..pos + len]) {
                return (len, id);
            }
        }

        // Fallback to single byte
        (1, bytes[pos] as u32)
    }

    /// Decode token IDs to text.
    pub fn decode(&self, tokens: &[u32]) -> Result<String, TokenizerError> {
        let mut bytes = Vec::new();
        for &id in tokens {
            if let Some(entry) = self.vocab.get(id as usize) {
                bytes.extend_from_slice(&entry.bytes);
            } else {
                return Err(TokenizerError::InvalidToken(id));
            }
        }
        String::from_utf8(bytes).map_err(|e| TokenizerError::DecodingFailed(e.to_string()))
    }

    /// Get end-of-sequence token ID.
    pub fn eos_token(&self) -> u32 {
        self.eos_token
    }

    /// Get beginning-of-sequence token ID.
    pub fn bos_token(&self) -> u32 {
        self.bos_token
    }

    /// Get vocabulary size.
    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }
}
