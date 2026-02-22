//! GGUF-based text generation model.
//!
//! Wraps llama-cpp-2 for text generation tasks.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::engine::{
    GenerationResult, InferenceCapability, InferenceConfig,
    InferenceError, InferenceInput, InferenceOutput,
};

/// GGUF text generation model using llama-cpp-2.
pub struct GgufGenerator {
    model_id: String,
    memory_bytes: AtomicUsize,
    #[allow(dead_code)]
    context_size: u32,
    #[cfg(feature = "gguf")]
    inner: Option<super::backend::LlamaBackendInner>,
}

impl GgufGenerator {
    /// Create a new generator (no model loaded yet).
    pub fn new(model_id: String, context_size: u32) -> Self {
        Self {
            model_id,
            memory_bytes: AtomicUsize::new(0),
            context_size,
            #[cfg(feature = "gguf")]
            inner: None,
        }
    }

    /// Load a model from a GGUF file path.
    #[cfg(feature = "gguf")]
    pub fn load(
        model_id: String,
        path: &std::path::Path,
        config: &super::GgufConfig,
    ) -> Result<Self, InferenceError> {
        let inner = super::backend::LlamaBackendInner::load(path, config)?;
        let mem = inner.model_size();
        Ok(Self {
            model_id,
            memory_bytes: AtomicUsize::new(mem),
            context_size: config.n_ctx,
            inner: Some(inner),
        })
    }

    /// Generate text from a prompt string.
    fn generate_text(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<GenerationResult, InferenceError> {
        self.generate_text_cancellable(prompt, config, None)
    }

    /// Generate text with optional cooperative cancellation.
    ///
    /// When `is_cancelled` is provided, the backend checks it once
    /// per token and returns `FinishReason::Cancelled` if set.
    fn generate_text_cancellable(
        &self,
        prompt: &str,
        config: &InferenceConfig,
        is_cancelled: Option<&dyn Fn() -> bool>,
    ) -> Result<GenerationResult, InferenceError> {
        if prompt.is_empty() {
            return Err(InferenceError::InputValidation(
                "prompt cannot be empty".into(),
            ));
        }
        #[cfg(feature = "gguf")]
        {
            if let Some(inner) = &self.inner {
                return inner.generate_cancellable(prompt, config, is_cancelled);
            }
        }
        #[cfg(not(feature = "gguf"))]
        {
            let _ = config;
            let _ = is_cancelled;
        }
        Err(InferenceError::ModelError(format!(
            "model '{}' not loaded - cannot generate",
            self.model_id
        )))
    }

    /// Stream tokens for a prompt, sending each to the channel.
    ///
    /// When `is_cancelled` is provided, the backend checks it once
    /// per token and stops streaming if set.
    #[cfg(feature = "gguf")]
    pub fn generate_stream(
        &self,
        prompt: &str,
        config: &InferenceConfig,
        sender: crate::engine::TokenStreamSender,
        is_cancelled: Option<&dyn Fn() -> bool>,
    ) -> Result<(), InferenceError> {
        if let Some(inner) = &self.inner {
            return inner.generate_stream(prompt, config, sender, is_cancelled);
        }
        Err(InferenceError::ModelError("no model loaded".into()))
    }

    /// Generate N tokens from token context (for speculative decoding).
    #[cfg(feature = "gguf")]
    pub async fn generate_tokens(
        &self,
        context: &[u32],
        count: usize,
    ) -> Result<Vec<u32>, InferenceError> {
        if let Some(inner) = &self.inner {
            return inner.generate_from_tokens(context, count);
        }
        Err(InferenceError::ModelError("no model loaded".into()))
    }

    /// Verify draft tokens against model (for speculative decoding).
    #[cfg(feature = "gguf")]
    pub async fn verify_draft_tokens(
        &self,
        context: &[u32],
        draft: &[u32],
    ) -> Result<crate::engine::speculative::VerifyResult, InferenceError> {
        if let Some(inner) = &self.inner {
            return inner.verify_tokens(context, draft);
        }
        Err(InferenceError::ModelError("no model loaded".into()))
    }

    /// Get EOS token ID (for speculative decoding).
    #[cfg(feature = "gguf")]
    pub fn eos_token_id(&self) -> Option<u32> {
        self.inner.as_ref().and_then(|i| i.eos_token())
    }

    /// Format chat messages into a prompt string.
    fn format_chat_prompt(
        &self,
        messages: &[crate::engine::ChatMessage],
    ) -> Result<String, InferenceError> {
        let mut prompt = String::new();
        for msg in messages {
            let tag = match msg.role {
                crate::engine::ChatRole::System => "<|system|>",
                crate::engine::ChatRole::User => "<|user|>",
                crate::engine::ChatRole::Assistant => "<|assistant|>",
            };
            prompt.push_str(tag);
            prompt.push_str(&msg.content);
            prompt.push_str("<|end|>\n");
        }
        prompt.push_str("<|assistant|>");
        Ok(prompt)
    }
}

#[async_trait::async_trait]
impl super::GgufModel for GgufGenerator {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn capabilities(&self) -> &[InferenceCapability] {
        &[InferenceCapability::TextGeneration]
    }

    fn memory_usage(&self) -> usize {
        self.memory_bytes.load(Ordering::SeqCst)
    }

    async fn infer(
        &self,
        input: &InferenceInput,
        config: &InferenceConfig,
    ) -> Result<InferenceOutput, InferenceError> {
        input.validate()?;
        config.validate()?;

        match input {
            InferenceInput::Text(prompt) => {
                let result = self.generate_text(prompt, config)?;
                Ok(InferenceOutput::Generation(result))
            }
            InferenceInput::ChatMessages(messages) => {
                let prompt = self.format_chat_prompt(messages)?;
                let result = self.generate_text(&prompt, config)?;
                Ok(InferenceOutput::Generation(result))
            }
            InferenceInput::TextBatch(_) => {
                Err(InferenceError::CapabilityNotSupported(
                    "batch generation not supported".into(),
                ))
            }
        }
    }

    async fn unload(&mut self) -> Result<(), InferenceError> {
        self.memory_bytes.store(0, Ordering::SeqCst);
        #[cfg(feature = "gguf")]
        {
            self.inner = None;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
