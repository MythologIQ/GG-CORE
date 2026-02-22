//! Tests for IPC protocol types and codec.

use super::super::protocol_types::*;
use super::super::protocol_codec::*;
use crate::engine::InferenceParams;

#[test]
fn test_protocol_version_default() {
    assert_eq!(ProtocolVersion::default(), ProtocolVersion::V1);
}

#[test]
fn test_protocol_version_is_supported() {
    assert!(ProtocolVersion::V1.is_supported());
    assert!(ProtocolVersion::V2.is_supported());
}

#[test]
fn test_protocol_version_negotiate_none() {
    assert_eq!(ProtocolVersion::negotiate(None), ProtocolVersion::V1);
}

#[test]
fn test_protocol_version_negotiate_v1() {
    assert_eq!(ProtocolVersion::negotiate(Some(ProtocolVersion::V1)), ProtocolVersion::V1);
}

#[test]
fn test_protocol_version_negotiate_v2() {
    assert_eq!(ProtocolVersion::negotiate(Some(ProtocolVersion::V2)), ProtocolVersion::V2);
}

#[test]
fn test_encode_decode_message() {
    let msg = IpcMessage::HealthCheck { check_type: HealthCheckType::Liveness };
    let encoded = encode_message(&msg).unwrap();
    let decoded = decode_message(&encoded).unwrap();
    assert!(matches!(decoded, IpcMessage::HealthCheck { check_type: HealthCheckType::Liveness }));
}

#[test]
fn test_encode_response_within_limit() {
    let msg = IpcMessage::Error { code: 500, message: "test".to_string() };
    let result = encode_response(&msg).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_decode_message_too_large() {
    let large_data = vec![0u8; TEST_MAX_MESSAGE_SIZE + 1];
    let result = decode_message(&large_data);
    assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
}

#[test]
fn test_encode_message_binary_roundtrip() {
    let msg = IpcMessage::HealthCheck { check_type: HealthCheckType::Readiness };
    let encoded = encode_message_binary(&msg).unwrap();
    let decoded = decode_message_binary(&encoded).unwrap();
    assert!(matches!(decoded, IpcMessage::HealthCheck { check_type: HealthCheckType::Readiness }));
}

#[test]
fn test_decode_message_binary_too_large() {
    let large_data = vec![0u8; TEST_MAX_MESSAGE_SIZE + 1];
    let result = decode_message_binary(&large_data);
    assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
}

#[test]
fn test_inference_request_validation() {
    let valid = InferenceRequest {
        request_id: RequestId(1),
        model_id: "test-model".to_string(),
        prompt: "Hello, world!".to_string(),
        parameters: InferenceParams::default(),
    };
    assert!(valid.validate().is_ok());

    let invalid_model = InferenceRequest {
        request_id: RequestId(1),
        model_id: "".to_string(),
        prompt: "Hello, world!".to_string(),
        parameters: InferenceParams::default(),
    };
    assert!(invalid_model.validate().is_err());

    let invalid_prompt = InferenceRequest {
        request_id: RequestId(1),
        model_id: "test".to_string(),
        prompt: "".to_string(),
        parameters: InferenceParams::default(),
    };
    assert!(invalid_prompt.validate().is_err());
}

#[test]
fn test_inference_response_success() {
    let response = InferenceResponse::success(RequestId(1), "Generated text".to_string(), 5, true);
    assert_eq!(response.request_id.0, 1);
    assert_eq!(response.output, "Generated text");
    assert_eq!(response.tokens_generated, 5);
    assert!(response.finished);
    assert!(response.error.is_none());
}

#[test]
fn test_inference_response_error() {
    let response = InferenceResponse::error(RequestId(1), "test error".to_string());
    assert!(response.finished);
    assert!(response.error.is_some());
    assert!(response.output.is_empty());
    assert_eq!(response.error_code, Some(InferenceErrorCode::ExecutionFailed));
}

#[test]
fn test_inference_response_success_has_no_error_code() {
    let r = InferenceResponse::success(RequestId(2), "text".into(), 3, true);
    assert!(r.error.is_none());
    assert!(r.error_code.is_none());
}

#[test]
fn test_inference_response_error_coded_admission_rejected() {
    let r = InferenceResponse::error_coded(
        RequestId(3), "Memory limit exceeded".into(), InferenceErrorCode::AdmissionRejected,
    );
    assert_eq!(r.error_code, Some(InferenceErrorCode::AdmissionRejected));
    assert!(r.error.is_some());
}

#[test]
fn test_inference_error_code_serializes() {
    let r = InferenceResponse::error_coded(RequestId(4), "err".into(), InferenceErrorCode::ModelNotLoaded);
    let msg = IpcMessage::InferenceResponse(r);
    let encoded = encode_message(&msg).unwrap();
    let decoded = decode_message(&encoded).unwrap();
    if let IpcMessage::InferenceResponse(resp) = decoded {
        assert_eq!(resp.error_code, Some(InferenceErrorCode::ModelNotLoaded));
    } else {
        panic!("expected InferenceResponse");
    }
}

#[test]
fn test_stream_chunk_token() {
    let chunk = StreamChunk::token(RequestId(1), 42);
    assert_eq!(chunk.token, 42);
    assert!(!chunk.is_final);
    assert!(chunk.error.is_none());
}

#[test]
fn test_stream_chunk_final() {
    let chunk = StreamChunk::final_token(RequestId(1), 42);
    assert!(chunk.is_final);
}

#[test]
fn test_stream_chunk_error() {
    let chunk = StreamChunk::error(RequestId(1), "error".to_string());
    assert!(chunk.is_final);
    assert!(chunk.error.is_some());
}

#[test]
fn test_warmup_response() {
    let success = WarmupResponse::success("model".to_string(), 100);
    assert!(success.success);
    let error = WarmupResponse::error("model".to_string(), "err".to_string(), 50);
    assert!(!error.success);
    assert!(error.error.is_some());
}

#[test]
fn test_handshake_message_encoding() {
    let msg = IpcMessage::Handshake {
        token: "test-token".to_string(),
        protocol_version: Some(ProtocolVersion::V2),
    };
    let encoded = encode_message(&msg).unwrap();
    let decoded: IpcMessage = serde_json::from_slice(&encoded).unwrap();
    assert!(matches!(decoded, IpcMessage::Handshake { protocol_version: Some(ProtocolVersion::V2), .. }));
}

#[test]
fn test_handshake_ack_message() {
    let msg = IpcMessage::HandshakeAck {
        session_id: "session-123".to_string(),
        protocol_version: ProtocolVersion::V1,
    };
    let encoded = encode_message(&msg).unwrap();
    let decoded = decode_message(&encoded).unwrap();
    assert!(matches!(
        decoded,
        IpcMessage::HandshakeAck { session_id, protocol_version: ProtocolVersion::V1 }
        if session_id == "session-123"
    ));
}

#[test]
fn test_protocol_error_display() {
    let err = ProtocolError::MessageTooLarge { size: 100, max: 50 };
    let msg = err.to_string();
    assert!(msg.contains("100"));
    assert!(msg.contains("50"));
    let err = ProtocolError::MissingField("test".to_string());
    assert!(err.to_string().contains("test"));
}
