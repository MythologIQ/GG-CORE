// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Chaos & Resilience - Scheduler and Inference Engine
//!
//! Queue flooding, concurrent enqueue/dequeue, expired request
//! handling, and inference engine edge cases.

use std::sync::Arc;
use std::time::Duration;

use veritas_sdr::engine::{InferenceEngine, InferenceParams};
use veritas_sdr::models::ModelHandle;
use veritas_sdr::scheduler::{Priority, RequestQueue, RequestQueueConfig};

// ============================================================================
// Queue Chaos
// ============================================================================

#[tokio::test]
async fn chaos_queue_flood() {
    let queue = RequestQueue::new(RequestQueueConfig { max_pending: 5 });
    for i in 0..5 {
        let r = queue.enqueue(
            "model".into(), vec![1, 2, 3],
            InferenceParams::default(), Priority::Normal,
        ).await;
        assert!(r.is_ok(), "Request {} should enqueue", i);
    }
    let overflow = queue.enqueue(
        "model".into(), vec![1],
        InferenceParams::default(), Priority::Normal,
    ).await;
    assert!(overflow.is_err(), "Queue should reject when full");
}

#[tokio::test]
async fn chaos_queue_cancel_then_dequeue() {
    let queue = RequestQueue::new(RequestQueueConfig { max_pending: 10 });
    let (id1, _) = queue.enqueue(
        "model".into(), vec![1], InferenceParams::default(), Priority::Normal,
    ).await.unwrap();
    let (id2, _) = queue.enqueue(
        "model".into(), vec![2], InferenceParams::default(), Priority::Normal,
    ).await.unwrap();
    assert!(queue.cancel(id1).await);
    let next = queue.dequeue().await.unwrap();
    assert_eq!(next.id, id2);
}

#[tokio::test]
async fn chaos_queue_expired_requests_skipped() {
    let queue = RequestQueue::new(RequestQueueConfig { max_pending: 10 });
    let short = InferenceParams { timeout_ms: Some(1), ..Default::default() };
    queue.enqueue("model".into(), vec![1], short, Priority::Normal).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    queue.enqueue(
        "model".into(), vec![2], InferenceParams::default(), Priority::Normal,
    ).await.unwrap();
    let next = queue.dequeue().await.unwrap();
    assert_eq!(next.prompt_tokens, vec![2]);
}

#[tokio::test]
async fn chaos_concurrent_enqueue_dequeue() {
    let queue = Arc::new(RequestQueue::new(RequestQueueConfig { max_pending: 256 }));
    let mut handles = vec![];
    for pid in 0..4 {
        let q = Arc::clone(&queue);
        handles.push(tokio::spawn(async move {
            let mut n = 0u32;
            for i in 0..25 {
                if q.enqueue(
                    format!("model-{}", pid), vec![i as u32],
                    InferenceParams::default(), Priority::Normal,
                ).await.is_ok() { n += 1; }
            }
            n
        }));
    }
    for _ in 0..2 {
        let q = Arc::clone(&queue);
        handles.push(tokio::spawn(async move {
            let mut n = 0u32;
            for _ in 0..50 {
                if q.dequeue().await.is_some() { n += 1; }
                tokio::task::yield_now().await;
            }
            n
        }));
    }
    let results: Vec<u32> = futures::future::join_all(handles)
        .await.into_iter().map(|r| r.unwrap()).collect();
    assert!(results[..4].iter().sum::<u32>() > 0);
}

// ============================================================================
// Inference Engine Chaos
// ============================================================================

#[tokio::test]
async fn chaos_inference_engine_context_exceeded() {
    let engine = InferenceEngine::new(128);
    let huge: Vec<u32> = (0..200).collect();
    assert!(engine.run(ModelHandle::new(0), &huge, &InferenceParams::default()).await.is_err());
}

#[tokio::test]
async fn chaos_inference_engine_invalid_params() {
    let engine = InferenceEngine::new(4096);
    let bad = InferenceParams { max_tokens: 0, ..Default::default() };
    assert!(engine.run(ModelHandle::new(0), &[1, 2, 3], &bad).await.is_err());
}

#[tokio::test]
async fn chaos_inference_engine_concurrent_requests() {
    let engine = Arc::new(InferenceEngine::new(4096));
    let mut handles = vec![];
    for i in 0..10u32 {
        let e = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            let tokens: Vec<u32> = (0..10).map(|t| t + i * 10).collect();
            e.run(ModelHandle::new(0), &tokens, &InferenceParams::default()).await
        }));
    }
    let results: Vec<_> = futures::future::join_all(handles).await;
    for (i, r) in results.iter().enumerate() {
        assert!(r.as_ref().unwrap().is_ok(), "Request {} should succeed", i);
    }
}
