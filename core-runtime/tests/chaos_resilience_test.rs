// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Chaos & Resilience - IPC Protocol Fault Injection
//!
//! Malformed IPC messages, type confusion, extreme payloads,
//! binary protocol chaos, and token encoding edge cases.

use veritas_sdr::ipc::{
    decode_message, encode_message, InferenceRequest, IpcMessage, RequestId,
    TokenEncoder, V1Encoder, V2Encoder,
};
use veritas_sdr::ipc::{decode_message_binary, encode_message_binary};
use veritas_sdr::engine::InferenceParams;

#[test]
fn chaos_ipc_random_bytes() {
    let patterns: Vec<&[u8]> = vec![
        &[0xFF, 0xFE, 0xFD],
        &[0x00, 0x00, 0x00, 0x00],
        &[0x7B],
        &[0x7D],
        b"\xff\xff\xff\xff\xff\xff\xff\xff",
        b"\x00",
        b"\r\n\r\n",
        b"\t\t\t\t",
    ];
    for (i, pattern) in patterns.iter().enumerate() {
        let result = decode_message(pattern);
        assert!(result.is_err(), "Pattern {} should be rejected", i);
    }
}

#[test]
fn chaos_ipc_truncated_json() {
    let truncated = vec![
        br#"{"type":"handshake","tok"#.to_vec(),
        br#"{"type":"inference_request","request_id":1"#.to_vec(),
        br#"{"type":"health_check","check_ty"#.to_vec(),
    ];
    for (i, msg) in truncated.iter().enumerate() {
        assert!(decode_message(msg).is_err(), "Truncated {} should fail", i);
    }
}

#[test]
fn chaos_ipc_type_confusion() {
    let confused = vec![
        br#"{"type":"inference_request","request_id":"not_a_number","model_id":"test","prompt_tokens":[1],"parameters":{}}"#.to_vec(),
        br#"{"type":"inference_request","request_id":1,"model_id":"test","prompt_tokens":"not_array","parameters":{}}"#.to_vec(),
        br#"{"type":"health_check","check_type":42}"#.to_vec(),
    ];
    for msg in &confused {
        let result = decode_message(msg);
        if let Ok(parsed) = result {
            let _ = encode_message(&parsed);
        }
    }
}

#[test]
fn chaos_ipc_repeated_fields() {
    let repeated = br#"{"type":"health_check","check_type":"Liveness","check_type":"Full"}"#;
    let _ = decode_message(repeated);
}

#[test]
fn chaos_ipc_extreme_string_lengths() {
    let long_id = "x".repeat(1_000_000);
    let msg = format!(
        r#"{{"type":"inference_request","request_id":1,"model_id":"{}","prompt_tokens":[1],"parameters":{{}}}}"#,
        long_id
    );
    if let Ok(IpcMessage::InferenceRequest(req)) = decode_message(msg.as_bytes()) {
        let _ = req.validate();
    }
}

#[test]
fn chaos_ipc_massive_token_array() {
    let tokens: Vec<u32> = (0..500_000).collect();
    let request = InferenceRequest {
        request_id: RequestId(1),
        model_id: "test".to_string(),
        prompt_tokens: tokens,
        parameters: InferenceParams::default(),
    };
    let msg = IpcMessage::InferenceRequest(request);
    if let Ok(bytes) = encode_message(&msg) {
        assert!(decode_message(&bytes).is_ok());
    }
}

#[test]
fn chaos_ipc_invalid_inference_params() {
    let bad = vec![
        InferenceParams { max_tokens: 0, ..Default::default() },
        InferenceParams { temperature: -1.0, ..Default::default() },
        InferenceParams { top_p: 0.0, ..Default::default() },
        InferenceParams { top_p: 1.5, ..Default::default() },
    ];
    for (i, p) in bad.iter().enumerate() {
        assert!(p.validate().is_err(), "Bad params {} should be rejected", i);
    }
}

#[test]
fn chaos_binary_random_bytes() {
    let patterns: Vec<&[u8]> = vec![
        &[0xFF, 0xFE], &[0x00],
        b"\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        b"not bincode at all", &[],
    ];
    for p in &patterns {
        assert!(decode_message_binary(p).is_err());
    }
}

#[test]
fn chaos_binary_encode_tagged_enums() {
    use veritas_sdr::ipc::HealthCheckType;
    let msg = IpcMessage::HealthCheck { check_type: HealthCheckType::Liveness };
    if let Ok(bytes) = encode_message_binary(&msg) {
        let _ = decode_message_binary(&bytes);
    }
}

#[test]
fn chaos_v1_encoder_malformed_input() {
    let encoder = V1Encoder;
    let bad: Vec<&[u8]> = vec![
        b"not json", b"[1, 2, \"x\"]", b"[1.5]", b"{}", b"null", b"",
    ];
    for input in bad {
        assert!(encoder.decode(input).is_err());
    }
}

#[test]
fn chaos_v2_encoder_malformed_input() {
    let enc = V2Encoder;
    assert!(enc.decode(&[0x01]).is_err());
    assert!(enc.decode(&[0x01, 0x02]).is_err());
    assert!(enc.decode(&[]).is_err());
    assert!(enc.decode(&[0x01, 0x00, 0x00, 0x00]).is_err());
    assert!(enc.decode(&[0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF]).is_err());
}

#[test]
fn chaos_v2_encoder_count_overflow() {
    let bytes = [0xFF, 0xFF, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00];
    assert!(V2Encoder.decode(&bytes).is_err());
}

#[test]
fn chaos_encoder_roundtrip_stress() {
    let v1 = V1Encoder;
    let v2 = V2Encoder;
    let cases: Vec<Vec<u32>> = vec![
        vec![], vec![0], vec![u32::MAX],
        vec![0, u32::MAX, 42, 1000],
        (0..1000).collect(),
    ];
    for tokens in &cases {
        assert_eq!(tokens, &v1.decode(&v1.encode(tokens)).unwrap());
        assert_eq!(tokens, &v2.decode(&v2.encode(tokens)).unwrap());
    }
}
