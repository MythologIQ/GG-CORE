// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for K8s CRD types and validation.

use super::*;

#[test]
fn test_serialize_runtime() {
    let runtime = GgCoreRuntime {
        api_version: "gg-core.io/v1".to_string(),
        kind: "GgCoreRuntime".to_string(),
        metadata: CrdMetadata {
            name: "gg-core-prod".to_string(),
            namespace: Some("default".to_string()),
            labels: None,
        },
        spec: GgCoreRuntimeSpec {
            replicas: 3,
            image: "gg-core:0.5.0".to_string(),
            memory: "4Gi".to_string(),
            cpu: "2".to_string(),
            gpu: None,
            model_pvc: "models-pvc".to_string(),
            socket_path: None,
        },
        status: None,
    };

    let json = serde_json::to_string_pretty(&runtime).unwrap();
    assert!(json.contains("gg-core.io/v1"));
    assert!(json.contains("GgCoreRuntime"));
}

#[test]
fn test_runtime_spec_with_gpu() {
    let spec = GgCoreRuntimeSpec {
        replicas: 2,
        image: "gg-core:0.5.0".to_string(),
        memory: "8Gi".to_string(),
        cpu: "4".to_string(),
        gpu: Some(GpuSpec {
            count: 2,
            resource_type: "nvidia.com/gpu".to_string(),
        }),
        model_pvc: "models-pvc".to_string(),
        socket_path: Some("/var/run/gg-core.sock".to_string()),
    };

    let json = serde_json::to_string(&spec).unwrap();
    assert!(json.contains("nvidia.com/gpu"));
    assert!(json.contains("\"count\":2"));
}

#[test]
fn test_gpu_spec_serialization() {
    let gpu = GpuSpec {
        count: 4,
        resource_type: "amd.com/gpu".to_string(),
    };

    let json = serde_json::to_string(&gpu).unwrap();
    let deserialized: GpuSpec = serde_json::from_str(&json).unwrap();

    assert_eq!(gpu.count, deserialized.count);
    assert_eq!(gpu.resource_type, deserialized.resource_type);
}

#[test]
fn test_runtime_status_serialization() {
    let status = GgCoreRuntimeStatus {
        ready_replicas: 3,
        phase: "Running".to_string(),
        conditions: vec![Condition {
            condition_type: "Ready".to_string(),
            status: "True".to_string(),
            reason: Some("AllReplicasReady".to_string()),
            message: Some("All replicas are ready".to_string()),
        }],
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: GgCoreRuntimeStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(status.ready_replicas, deserialized.ready_replicas);
    assert_eq!(status.phase, deserialized.phase);
    assert_eq!(status.conditions.len(), deserialized.conditions.len());
}

#[test]
fn test_model_spec_serialization() {
    let spec = GgCoreModelSpec {
        model_id: "llama-7b".to_string(),
        version: "1.0.0".to_string(),
        source: ModelSource {
            pvc: "models-pvc".to_string(),
            path: "/models/llama-7b.gguf".to_string(),
        },
        variant: Some("control".to_string()),
        auto_load: true,
    };

    let json = serde_json::to_string(&spec).unwrap();
    let deserialized: GgCoreModelSpec = serde_json::from_str(&json).unwrap();

    assert_eq!(spec.model_id, deserialized.model_id);
    assert_eq!(spec.version, deserialized.version);
    assert_eq!(spec.auto_load, deserialized.auto_load);
    assert_eq!(spec.variant, deserialized.variant);
}

#[test]
fn test_model_source_serialization() {
    let source = ModelSource {
        pvc: "my-pvc".to_string(),
        path: "/data/model.bin".to_string(),
    };

    let json = serde_json::to_string(&source).unwrap();
    let deserialized: ModelSource = serde_json::from_str(&json).unwrap();

    assert_eq!(source.pvc, deserialized.pvc);
    assert_eq!(source.path, deserialized.path);
}

#[test]
fn test_gg_core_model_full() {
    let model = GgCoreModel {
        api_version: "gg-core.io/v1".to_string(),
        kind: "GgCoreModel".to_string(),
        metadata: CrdMetadata {
            name: "llama-model".to_string(),
            namespace: Some("ml-models".to_string()),
            labels: Some({
                let mut map = std::collections::HashMap::new();
                map.insert("app".to_string(), "gg-core".to_string());
                map
            }),
        },
        spec: GgCoreModelSpec {
            model_id: "llama-7b".to_string(),
            version: "1.0.0".to_string(),
            source: ModelSource {
                pvc: "models-pvc".to_string(),
                path: "/models/llama.gguf".to_string(),
            },
            variant: None,
            auto_load: false,
        },
        status: Some(GgCoreModelStatus {
            loaded: true,
            phase: "Loaded".to_string(),
            conditions: vec![],
        }),
    };

    let json = serde_json::to_string_pretty(&model).unwrap();
    assert!(json.contains("GgCoreModel"));
    assert!(json.contains("llama-7b"));
    assert!(json.contains("ml-models"));
}

#[test]
fn test_model_status_serialization() {
    let status = GgCoreModelStatus {
        loaded: false,
        phase: "Loading".to_string(),
        conditions: vec![Condition {
            condition_type: "Loading".to_string(),
            status: "True".to_string(),
            reason: None,
            message: None,
        }],
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: GgCoreModelStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(status.loaded, deserialized.loaded);
    assert_eq!(status.phase, deserialized.phase);
}

#[test]
fn test_crd_metadata_minimal() {
    let meta = CrdMetadata {
        name: "test".to_string(),
        namespace: None,
        labels: None,
    };

    let json = serde_json::to_string(&meta).unwrap();
    let deserialized: CrdMetadata = serde_json::from_str(&json).unwrap();

    assert_eq!(meta.name, deserialized.name);
    assert!(deserialized.namespace.is_none());
    assert!(deserialized.labels.is_none());
}

#[test]
fn test_crd_metadata_with_labels() {
    let mut labels = std::collections::HashMap::new();
    labels.insert("env".to_string(), "prod".to_string());
    labels.insert("team".to_string(), "ml".to_string());

    let meta = CrdMetadata {
        name: "test".to_string(),
        namespace: Some("default".to_string()),
        labels: Some(labels),
    };

    let json = serde_json::to_string(&meta).unwrap();
    assert!(json.contains("env"));
    assert!(json.contains("prod"));
}

#[test]
fn test_condition_full() {
    let condition = Condition {
        condition_type: "Available".to_string(),
        status: "True".to_string(),
        reason: Some("MinimumReplicasAvailable".to_string()),
        message: Some("Deployment has minimum availability".to_string()),
    };

    let json = serde_json::to_string(&condition).unwrap();
    let deserialized: Condition = serde_json::from_str(&json).unwrap();

    assert_eq!(condition.condition_type, deserialized.condition_type);
    assert_eq!(condition.status, deserialized.status);
    assert_eq!(condition.reason, deserialized.reason);
    assert_eq!(condition.message, deserialized.message);
}

#[test]
fn test_condition_minimal() {
    let condition = Condition {
        condition_type: "Ready".to_string(),
        status: "False".to_string(),
        reason: None,
        message: None,
    };

    let json = serde_json::to_string(&condition).unwrap();
    let deserialized: Condition = serde_json::from_str(&json).unwrap();

    assert!(deserialized.reason.is_none());
    assert!(deserialized.message.is_none());
}

#[test]
fn test_camel_case_serialization() {
    let spec = GgCoreRuntimeSpec {
        replicas: 1,
        image: "test:latest".to_string(),
        memory: "1Gi".to_string(),
        cpu: "1".to_string(),
        gpu: None,
        model_pvc: "pvc-1".to_string(),
        socket_path: None,
    };

    let json = serde_json::to_string(&spec).unwrap();
    assert!(json.contains("modelPvc"));
    assert!(json.contains("socketPath"));
    assert!(!json.contains("model_pvc"));
    assert!(!json.contains("socket_path"));
}

#[test]
fn test_runtime_deserialization() {
    let json = r#"{
        "apiVersion": "gg-core.io/v1",
        "kind": "GgCoreRuntime",
        "metadata": {
            "name": "test-runtime",
            "namespace": "default"
        },
        "spec": {
            "replicas": 2,
            "image": "gg-core:latest",
            "memory": "2Gi",
            "cpu": "1",
            "modelPvc": "models"
        }
    }"#;

    let runtime: GgCoreRuntime = serde_json::from_str(json).unwrap();
    assert_eq!(runtime.metadata.name, "test-runtime");
    assert_eq!(runtime.spec.replicas, 2);
    assert_eq!(runtime.spec.model_pvc, "models");
}

#[test]
fn test_model_deserialization() {
    let json = r#"{
        "apiVersion": "gg-core.io/v1",
        "kind": "GgCoreModel",
        "metadata": {
            "name": "test-model"
        },
        "spec": {
            "modelId": "test",
            "version": "1.0.0",
            "source": {
                "pvc": "data-pvc",
                "path": "/models/test.bin"
            },
            "autoLoad": true
        }
    }"#;

    let model: GgCoreModel = serde_json::from_str(json).unwrap();
    assert_eq!(model.spec.model_id, "test");
    assert!(model.spec.auto_load);
}

#[test]
fn test_skip_serializing_none_status() {
    let runtime = GgCoreRuntime {
        api_version: "gg-core.io/v1".to_string(),
        kind: "GgCoreRuntime".to_string(),
        metadata: CrdMetadata {
            name: "test".to_string(),
            namespace: None,
            labels: None,
        },
        spec: GgCoreRuntimeSpec {
            replicas: 1,
            image: "test".to_string(),
            memory: "1Gi".to_string(),
            cpu: "1".to_string(),
            gpu: None,
            model_pvc: "pvc".to_string(),
            socket_path: None,
        },
        status: None,
    };

    let json = serde_json::to_string(&runtime).unwrap();
    assert!(!json.contains("status"));
}

#[test]
fn test_clone_traits() {
    let runtime = GgCoreRuntime {
        api_version: "gg-core.io/v1".to_string(),
        kind: "GgCoreRuntime".to_string(),
        metadata: CrdMetadata {
            name: "test".to_string(),
            namespace: None,
            labels: None,
        },
        spec: GgCoreRuntimeSpec {
            replicas: 1,
            image: "test".to_string(),
            memory: "1Gi".to_string(),
            cpu: "1".to_string(),
            gpu: None,
            model_pvc: "pvc".to_string(),
            socket_path: None,
        },
        status: None,
    };

    let cloned = runtime.clone();
    assert_eq!(runtime.metadata.name, cloned.metadata.name);
}

#[test]
fn test_debug_traits() {
    let gpu = GpuSpec {
        count: 1,
        resource_type: "nvidia.com/gpu".to_string(),
    };

    let debug_str = format!("{:?}", gpu);
    assert!(debug_str.contains("GpuSpec"));
    assert!(debug_str.contains("nvidia.com/gpu"));
}

// --- Validation tests ---

#[test]
fn test_validate_path_traversal() {
    assert!(validate_path("/models/test.gguf", "path").is_ok());
    assert!(validate_path("models/test.gguf", "path").is_ok());

    assert!(matches!(
        validate_path("../../../etc/passwd", "path"),
        Err(ValidationError::PathTraversal(_))
    ));
    assert!(matches!(
        validate_path("models/../../secret", "path"),
        Err(ValidationError::PathTraversal(_))
    ));
}

#[test]
fn test_validate_path_null_byte() {
    assert!(matches!(
        validate_path("models/test\0.gguf", "path"),
        Err(ValidationError::InvalidPath(_))
    ));
}

#[test]
fn test_validate_path_empty() {
    assert!(matches!(
        validate_path("", "path"),
        Err(ValidationError::EmptyField(_))
    ));
}

#[test]
fn test_validate_image_valid() {
    assert!(validate_image("gg-core:0.5.0").is_ok());
    assert!(validate_image("registry.io/gg-core:latest").is_ok());
    assert!(validate_image("gg-core").is_ok());
}

#[test]
fn test_validate_image_injection() {
    assert!(matches!(
        validate_image("gg-core; rm -rf /"),
        Err(ValidationError::InvalidImage(_))
    ));
    assert!(matches!(
        validate_image("gg-core && cat /etc/passwd"),
        Err(ValidationError::InvalidImage(_))
    ));
    assert!(matches!(
        validate_image("gg-core`whoami`"),
        Err(ValidationError::InvalidImage(_))
    ));
    assert!(matches!(
        validate_image("$(cat /etc/passwd)"),
        Err(ValidationError::InvalidImage(_))
    ));
}

#[test]
fn test_validate_image_empty() {
    assert!(matches!(
        validate_image(""),
        Err(ValidationError::EmptyField(_))
    ));
}

#[test]
fn test_validate_model_id_valid() {
    assert!(validate_model_id("llama-7b").is_ok());
    assert!(validate_model_id("model_v2.0").is_ok());
    assert!(validate_model_id("my-model-123").is_ok());
}

#[test]
fn test_validate_model_id_invalid() {
    assert!(matches!(
        validate_model_id("models/llama"),
        Err(ValidationError::InvalidModelId(_))
    ));
    assert!(matches!(
        validate_model_id("model;drop"),
        Err(ValidationError::InvalidModelId(_))
    ));
    assert!(matches!(
        validate_model_id(""),
        Err(ValidationError::EmptyField(_))
    ));
}

#[test]
fn test_validate_socket_path_valid() {
    assert!(validate_socket_path("/var/run/gg-core.sock").is_ok());
    assert!(validate_socket_path("/tmp/socket").is_ok());
}

#[test]
fn test_validate_socket_path_invalid() {
    assert!(matches!(
        validate_socket_path("var/run/gg-core.sock"),
        Err(ValidationError::InvalidSocketPath(_))
    ));
    assert!(matches!(
        validate_socket_path("/var/../etc/passwd"),
        Err(ValidationError::PathTraversal(_))
    ));
    assert!(matches!(
        validate_socket_path("/var/run\0/gg-core.sock"),
        Err(ValidationError::InvalidSocketPath(_))
    ));
}

#[test]
fn test_runtime_spec_validate() {
    let valid_spec = GgCoreRuntimeSpec {
        replicas: 2,
        image: "gg-core:0.5.0".to_string(),
        memory: "4Gi".to_string(),
        cpu: "2".to_string(),
        gpu: None,
        model_pvc: "models-pvc".to_string(),
        socket_path: Some("/var/run/gg-core.sock".to_string()),
    };
    assert!(valid_spec.validate().is_ok());

    let invalid_image = GgCoreRuntimeSpec {
        replicas: 2,
        image: "gg-core; rm -rf /".to_string(),
        memory: "4Gi".to_string(),
        cpu: "2".to_string(),
        gpu: None,
        model_pvc: "models-pvc".to_string(),
        socket_path: None,
    };
    assert!(invalid_image.validate().is_err());
}

#[test]
fn test_model_spec_validate() {
    let valid_spec = GgCoreModelSpec {
        model_id: "llama-7b".to_string(),
        version: "1.0.0".to_string(),
        source: ModelSource {
            pvc: "models-pvc".to_string(),
            path: "/models/llama.gguf".to_string(),
        },
        variant: Some("control".to_string()),
        auto_load: true,
    };
    assert!(valid_spec.validate().is_ok());

    let invalid_model_id = GgCoreModelSpec {
        model_id: "llama/../../../etc/passwd".to_string(),
        version: "1.0.0".to_string(),
        source: ModelSource {
            pvc: "models-pvc".to_string(),
            path: "/models/llama.gguf".to_string(),
        },
        variant: None,
        auto_load: true,
    };
    assert!(invalid_model_id.validate().is_err());
}

#[test]
fn test_model_source_validate() {
    let valid_source = ModelSource {
        pvc: "models-pvc".to_string(),
        path: "/models/test.gguf".to_string(),
    };
    assert!(valid_source.validate().is_ok());

    let traversal_source = ModelSource {
        pvc: "models-pvc".to_string(),
        path: "../../../etc/passwd".to_string(),
    };
    assert!(traversal_source.validate().is_err());
}
