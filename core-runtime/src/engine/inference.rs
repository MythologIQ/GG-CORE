//! Core inference execution.

use thiserror::Error;

use crate::models::ModelHandle;

#[derive(Error, Debug)]
pub enum InferenceError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(u64),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Inference failed: {0}")]
    ExecutionFailed(String),

    #[error("Context length exceeded: max {max}, got {got}")]
    ContextExceeded { max: usize, got: usize },
}

/// Parameters controlling inference behavior.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceParams {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: usize,
    /// Enable token-by-token streaming response.
    #[serde(default)]
    pub stream: bool,
    /// Request timeout in milliseconds. None = no timeout.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            stream: false,
            timeout_ms: None,
        }
    }
}

impl InferenceParams {
    pub fn validate(&self) -> Result<(), InferenceError> {
        if self.max_tokens == 0 {
            return Err(InferenceError::InvalidParams("max_tokens must be > 0".into()));
        }
        if self.temperature < 0.0 {
            return Err(InferenceError::InvalidParams("temperature must be >= 0".into()));
        }
        if self.top_p <= 0.0 || self.top_p > 1.0 {
            return Err(InferenceError::InvalidParams("top_p must be in (0, 1]".into()));
        }
        Ok(())
    }
}

/// Result of inference execution.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub output_tokens: Vec<u32>,
    pub tokens_generated: usize,
    pub finished: bool,
}

/// Executes model inference.
pub struct InferenceEngine {
    max_context_length: usize,
}

impl InferenceEngine {
    pub fn new(max_context_length: usize) -> Self {
        Self { max_context_length }
    }

    /// Run inference on input tokens.
    pub async fn run(
        &self,
        _model: ModelHandle,
        input_tokens: &[u32],
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        params.validate()?;

        if input_tokens.len() > self.max_context_length {
            return Err(InferenceError::ContextExceeded {
                max: self.max_context_length,
                got: input_tokens.len(),
            });
        }

        // Development mode: generate mock tokens proportional to input
        // TODO: Replace with actual inference via candle/llama-cpp when model integration is complete
        let generated_count = params.max_tokens.min(input_tokens.len().saturating_add(10));
        let output_tokens: Vec<u32> = (0..generated_count as u32).collect();

        Ok(InferenceResult {
            output_tokens,
            tokens_generated: generated_count,
            finished: true,
        })
    }

    pub fn max_context_length(&self) -> usize {
        self.max_context_length
    }
}
