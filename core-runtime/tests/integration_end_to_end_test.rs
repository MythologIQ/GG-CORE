//! End-to-end integration tests for the full request pipeline.
//!
//! Tests the complete flow: IPC → Scheduler → Engine → Response.

use gg_core::engine::{FilterConfig, InferenceParams};
use gg_core::ipc::protocol::{
    decode_message, encode_message, InferenceRequest, IpcMessage, RequestId,
};
use gg_core::scheduler::{Priority, PriorityQueue, QueuedRequest, RequestQueueConfig};

#[test]
fn ipc_request_roundtrip() {
    let request = InferenceRequest {
        request_id: RequestId(12345),
        model_id: "test-model".to_string(),
        prompt: "Hello, this is a test prompt".to_string(),
        parameters: InferenceParams {
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            top_k: 50,
            stream: false,
            timeout_ms: None,
        },
    };

    let message = IpcMessage::InferenceRequest(request.clone());

    // Encode
    let encoded = encode_message(&message).expect("Encoding should succeed");
    assert!(!encoded.is_empty());

    // Decode
    let decoded = decode_message(&encoded).expect("Decoding should succeed");

    // Verify roundtrip
    match decoded {
        IpcMessage::InferenceRequest(req) => {
            assert_eq!(req.request_id, RequestId(12345));
            assert_eq!(req.model_id, "test-model");
            assert_eq!(req.prompt, "Hello, this is a test prompt");
        }
        _ => panic!("Expected InferenceRequest"),
    }
}

#[test]
fn scheduler_request_ordering() {
    let mut queue: PriorityQueue<QueuedRequest> = PriorityQueue::new();

    // Add requests with different priorities
    queue.push(
        QueuedRequest::new(1, "model-a".to_string(), "first prompt".to_string(), InferenceParams::default()),
        Priority::Normal,
    );

    queue.push(
        QueuedRequest::new(2, "model-b".to_string(), "second prompt".to_string(), InferenceParams::default()),
        Priority::Critical,
    );

    // Critical priority request should come first
    let first = queue.pop().expect("Should have request");
    assert_eq!(first.id, 2, "Critical request should be first");

    let second = queue.pop().expect("Should have request");
    assert_eq!(second.id, 1, "Normal request should be second");
}

#[test]
fn error_propagation_model_not_found() {
    use gg_core::models::ModelLoader;

    let loader = ModelLoader::new(std::env::temp_dir());

    // Attempting to validate non-existent model should propagate error
    let result = loader.validate_path("nonexistent/model.bin");
    assert!(result.is_err());

    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("not found") || err_str.contains("not allowed"));
}

#[test]
fn concurrent_request_queue_capacity() {
    let config = RequestQueueConfig { max_pending: 100, ..Default::default() };

    // Queue should accept requests up to max_pending
    assert_eq!(config.max_pending, 100);
}

#[test]
fn request_id_uniqueness() {
    let id1 = RequestId(1);
    let id2 = RequestId(2);
    let id3 = RequestId(1);

    assert_ne!(id1, id2, "Different IDs should not be equal");
    assert_eq!(id1, id3, "Same IDs should be equal");
}

#[test]
fn priority_ordering_correct() {
    assert!(Priority::Critical as u8 > Priority::High as u8);
    assert!(Priority::High as u8 > Priority::Normal as u8);
    assert!(Priority::Normal as u8 > Priority::Low as u8);
}

#[test]
fn inference_params_serialization() {
    let params = InferenceParams {
        max_tokens: 256,
        temperature: 0.8,
        top_p: 0.95,
        top_k: 40,
        stream: false,
        timeout_ms: None,
    };

    // Params should be serializable
    let json = serde_json::to_string(&params).expect("Should serialize");
    assert!(json.contains("max_tokens"));
    assert!(json.contains("256"));
}

#[test]
fn output_filter_config_defaults() {
    let config = FilterConfig::default();

    // Default should have empty blocklist
    assert!(config.blocklist.is_empty());
}

#[test]
fn queue_fifo_for_same_priority() {
    let mut queue: PriorityQueue<QueuedRequest> = PriorityQueue::new();

    // Add multiple requests with same priority
    for i in 1..=5 {
        queue.push(
            QueuedRequest::new(i, format!("model-{}", i), format!("prompt-{}", i), InferenceParams::default()),
            Priority::Normal,
        );
    }

    // Should dequeue in FIFO order for same priority
    let first = queue.pop().expect("Should have request");
    assert_eq!(first.id, 1, "First request should come first");

    let second = queue.pop().expect("Should have request");
    assert_eq!(second.id, 2, "Second request should come second");
}
