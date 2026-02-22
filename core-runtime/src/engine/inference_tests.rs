//! Tests for the core inference engine.
//!
//! Extracted from `inference.rs` for Section 4 compliance.

use super::*;
use std::sync::Arc as StdArc;
use crate::engine::gguf::GgufModel;
use crate::engine::{
    FinishReason, GenerationResult, InferenceCapability, InferenceConfig,
    InferenceError as EngineError, InferenceInput, InferenceOutput,
};

#[test]
fn inference_params_default_is_valid() {
    let params = InferenceParams::default();
    assert!(params.validate().is_ok());
}

#[test]
fn inference_params_rejects_zero_max_tokens() {
    let params = InferenceParams {
        max_tokens: 0,
        ..Default::default()
    };
    assert!(params.validate().is_err());
}

#[test]
fn inference_params_rejects_negative_temperature() {
    let params = InferenceParams {
        temperature: -0.1,
        ..Default::default()
    };
    assert!(params.validate().is_err());
}

#[test]
fn inference_params_rejects_invalid_top_p() {
    let params = InferenceParams {
        top_p: 0.0,
        ..Default::default()
    };
    assert!(params.validate().is_err());

    let params = InferenceParams {
        top_p: 1.5,
        ..Default::default()
    };
    assert!(params.validate().is_err());
}

#[tokio::test]
async fn engine_new_creates_empty_engine() {
    let engine = InferenceEngine::new(4096);
    assert_eq!(engine.max_context_length(), 4096);
    assert!(!engine.has_model("any-model").await);
}

#[tokio::test]
async fn engine_run_fails_for_unloaded_model() {
    let engine = InferenceEngine::new(4096);
    let params = InferenceParams::default();
    let result = engine.run("missing-model", "test prompt", &params).await;
    assert!(matches!(result, Err(InferenceError::ModelNotLoaded(_))));
}

// ---- Memory budget enforcement tests ----

struct BudgetModel {
    reported_memory: usize,
}

#[async_trait::async_trait]
impl GgufModel for BudgetModel {
    fn model_id(&self) -> &str { "budget-model" }
    fn capabilities(&self) -> &[InferenceCapability] {
        &[InferenceCapability::TextGeneration]
    }
    fn memory_usage(&self) -> usize { self.reported_memory }
    async fn infer(
        &self, _: &InferenceInput, _: &InferenceConfig,
    ) -> Result<InferenceOutput, EngineError> {
        Ok(InferenceOutput::Generation(GenerationResult {
            text: "ok".into(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
        }))
    }
    async fn unload(&mut self) -> Result<(), EngineError> { Ok(()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

async fn engine_with_budget_model(memory_usage: usize) -> InferenceEngine {
    let engine = InferenceEngine::new(4096);
    let handle = ModelHandle::new(1);
    let model: StdArc<dyn GgufModel> = StdArc::new(BudgetModel { reported_memory: memory_usage });
    engine.register_model("budget-model".into(), handle, model).await;
    engine
}

#[tokio::test]
async fn memory_budget_allows_inference_when_model_fits() {
    let engine = engine_with_budget_model(512).await;
    let params = InferenceParams::default();
    let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
    let result = engine
        .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 1024)
        .await;
    assert!(result.is_ok(), "expected success, got {result:?}");
}

#[tokio::test]
async fn memory_budget_rejects_when_model_exceeds_budget() {
    let engine = engine_with_budget_model(2048).await;
    let params = InferenceParams::default();
    let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
    let result = engine
        .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 1024)
        .await;
    assert!(
        matches!(result, Err(InferenceError::MemoryExceeded { used: 2048, limit: 1024 })),
        "expected MemoryExceeded, got {result:?}"
    );
}

#[tokio::test]
async fn zero_byte_budget_always_rejects() {
    let engine = engine_with_budget_model(1).await;
    let params = InferenceParams::default();
    let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
    let result = engine
        .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 0)
        .await;
    assert!(
        matches!(result, Err(InferenceError::MemoryExceeded { .. })),
        "zero budget must reject every model"
    );
}

#[tokio::test]
async fn to_config_sets_max_memory_bytes_from_budget() {
    let engine = engine_with_budget_model(100).await;
    let params = InferenceParams::default();
    let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));

    let ok = engine
        .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled.clone(), 200)
        .await;
    assert!(ok.is_ok());

    let err = engine
        .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 50)
        .await;
    assert!(matches!(err, Err(InferenceError::MemoryExceeded { .. })));
}
