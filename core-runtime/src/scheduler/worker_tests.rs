//! Tests for the worker dequeue-execute loop.

use std::sync::Arc;

use super::*;
use crate::engine::inference::InferenceEngine;
use crate::engine::gguf::GgufModel;
use crate::engine::{
    FinishReason, GenerationResult, InferenceCapability, InferenceConfig,
    InferenceError as EngineError, InferenceInput, InferenceOutput, InferenceParams,
};
use crate::scheduler::queue::{RequestQueue, RequestQueueConfig};
use crate::scheduler::Priority;

struct MockModel {
    id: String,
}

impl MockModel {
    fn arc(id: &str) -> Arc<dyn GgufModel> {
        Arc::new(Self { id: id.to_string() })
    }
}

#[async_trait::async_trait]
impl GgufModel for MockModel {
    fn model_id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[InferenceCapability] {
        &[InferenceCapability::TextGeneration]
    }
    fn memory_usage(&self) -> usize { 1024 }
    async fn infer(
        &self, _: &InferenceInput, _: &InferenceConfig,
    ) -> Result<InferenceOutput, EngineError> {
        Ok(InferenceOutput::Generation(GenerationResult {
            text: "hello world".into(),
            tokens_generated: 2,
            finish_reason: FinishReason::Stop,
        }))
    }
    async fn unload(&mut self) -> Result<(), EngineError> { Ok(()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

async fn setup() -> (Arc<RequestQueue>, Arc<InferenceEngine>) {
    let queue = Arc::new(RequestQueue::new(RequestQueueConfig { max_pending: 64, ..Default::default() }));
    let engine = Arc::new(InferenceEngine::new(4096));
    let handle = crate::models::ModelHandle::new(1);
    engine.register_model("test-model".into(), handle, MockModel::arc("test")).await;
    (queue, engine)
}

#[tokio::test]
async fn worker_executes_enqueued_request() {
    let (queue, engine) = setup().await;
    let shutdown = tokio_util::sync::CancellationToken::new();

    let worker = spawn_worker(queue.clone(), engine, shutdown.clone());

    let (_id, rx) = queue
        .enqueue_with_response(
            "test-model".into(), "hi".into(),
            InferenceParams::default(), Priority::Normal,
        )
        .await.unwrap();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2), rx,
    ).await.unwrap().unwrap().unwrap();

    assert_eq!(result.output, "hello world");
    assert_eq!(result.tokens_generated, 2);

    shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), worker).await;
}

#[tokio::test]
async fn worker_skips_cancelled_request() {
    let (queue, engine) = setup().await;
    let shutdown = tokio_util::sync::CancellationToken::new();

    let worker = spawn_worker(queue.clone(), engine, shutdown.clone());

    // Enqueue and immediately cancel
    let (id, rx) = queue
        .enqueue_with_response(
            "test-model".into(), "cancel me".into(),
            InferenceParams::default(), Priority::Normal,
        )
        .await.unwrap();
    queue.cancel(id).await;

    // Enqueue a second request that should succeed
    let (_id2, rx2) = queue
        .enqueue_with_response(
            "test-model".into(), "keep me".into(),
            InferenceParams::default(), Priority::Normal,
        )
        .await.unwrap();

    // First request: cancelled (receiver dropped or error)
    let r1 = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await;
    // Either the receiver gets an error or the channel is dropped
    assert!(r1.is_err() || r1.unwrap().is_err());

    // Second request: succeeds
    let r2 = tokio::time::timeout(std::time::Duration::from_secs(2), rx2)
        .await.unwrap().unwrap().unwrap();
    assert_eq!(r2.output, "hello world");

    shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), worker).await;
}

#[tokio::test]
async fn worker_handles_multiple_concurrent_requests() {
    let (queue, engine) = setup().await;
    let shutdown = tokio_util::sync::CancellationToken::new();

    let worker = spawn_worker(queue.clone(), engine, shutdown.clone());

    let mut receivers = Vec::new();
    for i in 0..5 {
        let (_id, rx) = queue
            .enqueue_with_response(
                "test-model".into(), format!("prompt {i}"),
                InferenceParams::default(), Priority::Normal,
            )
            .await.unwrap();
        receivers.push(rx);
    }

    for rx in receivers {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5), rx,
        ).await.unwrap().unwrap().unwrap();
        assert_eq!(result.output, "hello world");
    }

    shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), worker).await;
}

#[tokio::test]
async fn worker_shuts_down_gracefully() {
    let (queue, engine) = setup().await;
    let shutdown = tokio_util::sync::CancellationToken::new();

    let worker = spawn_worker(queue.clone(), engine, shutdown.clone());

    shutdown.cancel();
    queue.wake();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2), worker,
    ).await;
    assert!(result.is_ok(), "worker should shut down within timeout");
}

#[tokio::test]
async fn queue_full_rejected() {
    let queue = Arc::new(RequestQueue::new(RequestQueueConfig { max_pending: 2, ..Default::default() }));

    queue.enqueue("m".into(), "a".into(), InferenceParams::default(), Priority::Normal)
        .await.unwrap();
    queue.enqueue("m".into(), "b".into(), InferenceParams::default(), Priority::Normal)
        .await.unwrap();

    let err = queue.enqueue("m".into(), "c".into(), InferenceParams::default(), Priority::Normal)
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn tier1_context_check_rejects_oversized_prompt() {
    // max_context_tokens=10 means max ~40 bytes before tier-1 rejects.
    let config = RequestQueueConfig {
        max_pending: 64,
        max_context_tokens: 10,
    };
    let queue = Arc::new(RequestQueue::new(config));

    // 44 bytes / 4 = 11 estimated tokens > 10 max => rejected
    let big_prompt = "a".repeat(44);
    let err = queue
        .enqueue("m".into(), big_prompt, InferenceParams::default(), Priority::Normal)
        .await;
    assert!(err.is_err(), "oversized prompt must be rejected at tier-1");

    // 40 bytes / 4 = 10 estimated tokens == 10 max => accepted
    let ok_prompt = "b".repeat(40);
    let result = queue
        .enqueue("m".into(), ok_prompt, InferenceParams::default(), Priority::Normal)
        .await;
    assert!(result.is_ok(), "prompt at limit should be accepted");
}

#[tokio::test]
async fn tier1_context_check_allows_small_prompt() {
    let config = RequestQueueConfig {
        max_pending: 64,
        max_context_tokens: 4096,
    };
    let queue = Arc::new(RequestQueue::new(config));

    let result = queue
        .enqueue("m".into(), "hello".into(), InferenceParams::default(), Priority::Normal)
        .await;
    assert!(result.is_ok());
}
