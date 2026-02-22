//! Core inference execution with real model delegation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engine::gguf::GgufModel;
use crate::engine::{InferenceConfig, InferenceInput, InferenceOutput};
use crate::models::ModelHandle;

pub use super::inference_types::{InferenceError, InferenceParams, InferenceResult};

/// Executes model inference by delegating to registered models.
pub struct InferenceEngine {
    max_context_length: usize,
    /// Models indexed by model_id for lookup.
    models: Arc<RwLock<HashMap<String, Arc<dyn GgufModel>>>>,
}

impl InferenceEngine {
    pub fn new(max_context_length: usize) -> Self {
        Self {
            max_context_length,
            models: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a model for inference.
    pub async fn register_model(
        &self,
        model_id: String,
        _handle: ModelHandle,
        model: Arc<dyn GgufModel>,
    ) {
        self.models.write().await.insert(model_id, model);
    }

    /// Unregister a model.
    pub async fn unregister_model(&self, model_id: &str) {
        self.models.write().await.remove(model_id);
    }

    /// Run inference on text prompt using the specified model.
    ///
    /// NOTE: The read lock on `self.models` is held for the entire
    /// inference call. This is a P2 optimization target — clone the
    /// Arc<dyn GgufModel> and drop the lock before calling infer().
    pub async fn run(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        params.validate()?;
        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;
        Self::infer_with_model(&model, prompt, params).await
    }

    /// Run inference with cooperative cancellation.
    ///
    /// Checks `is_cancelled` before inference. Per-token cancellation
    /// is threaded through the GGUF backend in a future PR (P0.3).
    pub async fn run_cancellable(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
        is_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<InferenceResult, InferenceError> {
        use std::sync::atomic::Ordering;

        params.validate()?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;

        let result = Self::infer_with_model(&model, prompt, params).await?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        Ok(result)
    }

    /// Run inference with cooperative cancellation and a per-call memory budget.
    ///
    /// The `max_memory_bytes` is enforced before calling into the model: if the
    /// model's reported memory exceeds the budget, `MemoryExceeded` is returned
    /// without starting inference (OOM prevention).
    pub async fn run_cancellable_with_memory_limit(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
        is_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        max_memory_bytes: usize,
    ) -> Result<InferenceResult, InferenceError> {
        use std::sync::atomic::Ordering;

        params.validate()?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;

        let result = Self::infer_with_model_budget(
            &model, prompt, params, Some(max_memory_bytes),
        ).await?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        Ok(result)
    }

    /// Look up a model by ID, cloning the Arc (drops the read lock).
    async fn get_model(
        &self,
        model_id: &str,
    ) -> Result<Arc<dyn GgufModel>, InferenceError> {
        let models = self.models.read().await;
        models.get(model_id).cloned().ok_or_else(|| {
            InferenceError::ModelNotLoaded(model_id.to_string())
        })
    }

    fn check_context(&self, prompt: &str) -> Result<(), InferenceError> {
        if prompt.len() > self.max_context_length {
            return Err(InferenceError::ContextExceeded {
                max: self.max_context_length,
                got: prompt.len(),
            });
        }
        Ok(())
    }

    async fn infer_with_model(
        model: &Arc<dyn GgufModel>,
        prompt: &str,
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        Self::infer_with_model_budget(model, prompt, params, None).await
    }

    async fn infer_with_model_budget(
        model: &Arc<dyn GgufModel>,
        prompt: &str,
        params: &InferenceParams,
        max_memory_bytes: Option<usize>,
    ) -> Result<InferenceResult, InferenceError> {
        // Enforce per-call memory budget: reject before inference if model
        // already reports more memory than the budget allows.
        if let Some(budget) = max_memory_bytes {
            let model_mem = model.memory_usage();
            if model_mem > budget {
                return Err(InferenceError::MemoryExceeded {
                    used: model_mem,
                    limit: budget,
                });
            }
        }

        let mut config = params.to_config();
        config.max_memory_bytes = max_memory_bytes;

        let input = InferenceInput::Text(prompt.to_string());
        let output = model.infer(&input, &config).await.map_err(|e| {
            InferenceError::ExecutionFailed(e.to_string())
        })?;

        match output {
            InferenceOutput::Generation(gen) => Ok(InferenceResult {
                output: gen.text,
                tokens_generated: gen.tokens_generated as usize,
                finished: true,
            }),
            _ => Err(InferenceError::ExecutionFailed(
                "Model returned non-generation output".into(),
            )),
        }
    }

    pub fn max_context_length(&self) -> usize {
        self.max_context_length
    }

    /// Check if a model is registered.
    pub async fn has_model(&self, model_id: &str) -> bool {
        self.models.read().await.contains_key(model_id)
    }

    /// Return the memory usage reported by a registered model, or None if not found.
    pub async fn model_memory_usage(&self, model_id: &str) -> Option<usize> {
        self.models.read().await.get(model_id).map(|m| m.memory_usage())
    }

    /// Run streaming inference, sending tokens to the provided sender.
    ///
    /// This method looks up the model, downcasts to GgufGenerator, and calls
    /// generate_stream(). Designed for use with spawn_blocking.
    #[cfg(feature = "gguf")]
    pub fn run_stream_sync(
        &self,
        model_id: &str,
        prompt: &str,
        config: &InferenceConfig,
        sender: crate::engine::TokenStreamSender,
    ) -> Result<(), InferenceError> {
        use crate::engine::gguf::GgufGenerator;

        // Get runtime handle for async model lookup
        let rt = tokio::runtime::Handle::current();
        let models = rt.block_on(self.models.read());
        let model = models.get(model_id).ok_or_else(|| {
            InferenceError::ModelNotLoaded(model_id.to_string())
        })?;

        // Downcast to GgufGenerator for streaming access
        let generator = model.as_any().downcast_ref::<GgufGenerator>().ok_or_else(|| {
            InferenceError::ExecutionFailed("model does not support streaming".into())
        })?;

        generator.generate_stream(prompt, config, sender, None)
            .map_err(|e| InferenceError::ExecutionFailed(e.to_string()))
    }
}

#[cfg(test)]
#[path = "inference_tests.rs"]
mod tests;
