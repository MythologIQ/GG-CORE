//! Veritas SDR - Secure Deterministic Runtime
//!
//! **Veritas** (Truth, Integrity, Correctness) + **SDR** (Secure Deterministic Runtime)
//!
//! A sandboxed, offline inference engine that performs model execution only.
//! No authority over data, tools, or system actions.
//!
//! # Design Principles
//!
//! - **Secure**: Sandbox with no ambient privileges, comprehensive input/output validation
//! - **Deterministic**: No GC pauses, predictable latency, reproducible results
//! - **Veritas**: Truth in outputs, integrity in execution, correctness in behavior
//!
//! # Security Boundaries
//!
//! - Process: Separate OS process, restricted user
//! - Filesystem: Read `models/`, `tokenizers/`. Write `temp/`, `cache/`.
//! - Network: Blocked (deny all)
//! - IPC: Named pipes/Unix sockets only. No HTTP/REST/WebSocket.

pub mod engine;
pub mod health;
pub mod ipc;
pub mod memory;
pub mod models;
pub mod sandbox;
pub mod scheduler;
pub mod security;
pub mod shutdown;
pub mod telemetry;

// A/B testing module (v0.5.0)
pub mod ab_testing;

// Kubernetes types (v0.5.0)
pub mod k8s;

// CLI module for health probes (v0.5.0)
pub mod cli;

// Deployment automation (v0.6.0)
pub mod deployment;

// C FFI module (v0.3.1)
#[cfg(feature = "ffi")]
pub mod ffi;

// Python bindings module (v0.3.1)
#[cfg(feature = "python")]
pub mod python;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use engine::InferenceEngine;
use health::{HealthChecker, HealthConfig};
use ipc::{ConnectionConfig, ConnectionPool, IpcHandler, IpcHandlerConfig, SessionAuth};
use memory::{
    ContextCache, ContextCacheConfig, GpuMemory, GpuMemoryConfig, MemoryPool, MemoryPoolConfig,
};
use models::{ModelLoader, ModelRegistry};
use scheduler::{
    BatchConfig, BatchProcessor, OutputCache, OutputCacheConfig, RequestQueue, RequestQueueConfig,
};
use shutdown::ShutdownCoordinator;
use telemetry::MetricsStore;
use tokio::sync::Mutex;

/// Runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub base_path: PathBuf,
    pub auth_token: String,
    pub session_timeout: Duration,
    pub max_context_length: usize,
    pub memory_pool: MemoryPoolConfig,
    pub gpu_memory: GpuMemoryConfig,
    pub context_cache: ContextCacheConfig,
    pub request_queue: RequestQueueConfig,
    pub batch: BatchConfig,
    pub shutdown_timeout: Duration,
    pub output_cache: OutputCacheConfig,
    pub connections: ConnectionConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("."),
            auth_token: String::new(),
            session_timeout: Duration::from_secs(3600),
            max_context_length: 4096,
            memory_pool: MemoryPoolConfig::default(),
            gpu_memory: GpuMemoryConfig::default(),
            context_cache: ContextCacheConfig::default(),
            request_queue: RequestQueueConfig::default(),
            batch: BatchConfig::default(),
            shutdown_timeout: Duration::from_secs(30),
            output_cache: OutputCacheConfig::default(),
            connections: ConnectionConfig::default(),
        }
    }
}

/// The CORE Runtime instance.
pub struct Runtime {
    pub config: RuntimeConfig,
    pub memory_pool: MemoryPool,
    pub gpu_memory: GpuMemory,
    pub context_cache: ContextCache,
    pub model_loader: ModelLoader,
    pub model_registry: Arc<ModelRegistry>,
    pub inference_engine: Arc<InferenceEngine>,
    pub request_queue: Arc<RequestQueue>,
    pub batch_processor: BatchProcessor,
    pub ipc_handler: IpcHandler,
    pub shutdown: Arc<ShutdownCoordinator>,
    pub health: Arc<HealthChecker>,
    pub metrics_store: Arc<MetricsStore>,
    pub output_cache: Arc<Mutex<OutputCache>>,
    pub connections: Arc<ConnectionPool>,
}

impl Runtime {
    /// Create a new runtime instance with the given configuration.
    pub fn new(config: RuntimeConfig) -> Self {
        let memory_pool = MemoryPool::new(config.memory_pool.clone());
        let gpu_memory = GpuMemory::new(config.gpu_memory.clone());
        let context_cache = ContextCache::new(config.context_cache.clone());
        let model_loader = ModelLoader::new(config.base_path.clone());
        let model_registry = Arc::new(ModelRegistry::new());
        let inference_engine = InferenceEngine::new(config.max_context_length);
        let request_queue = Arc::new(RequestQueue::new(config.request_queue.clone()));
        let batch_processor = BatchProcessor::new(config.batch.clone());
        let shutdown = Arc::new(ShutdownCoordinator::new());
        let health = Arc::new(HealthChecker::new(HealthConfig::default()));
        let metrics_store = Arc::new(MetricsStore::new());
        let output_cache = Arc::new(Mutex::new(OutputCache::new(config.output_cache.clone())));
        let connections = Arc::new(ConnectionPool::new(config.connections.clone()));

        let session_auth = Arc::new(SessionAuth::new(&config.auth_token, config.session_timeout));
        let inference_engine = Arc::new(inference_engine);
        let ipc_handler = IpcHandler::new(
            session_auth,
            request_queue.clone(),
            IpcHandlerConfig::default(),
            shutdown.clone(),
            health.clone(),
            model_registry.clone(),
            metrics_store.clone(),
            Arc::clone(&inference_engine),
        );

        Self {
            config,
            memory_pool,
            gpu_memory,
            context_cache,
            model_loader,
            model_registry,
            inference_engine,
            request_queue,
            batch_processor,
            ipc_handler,
            shutdown,
            health,
            metrics_store,
            output_cache,
            connections,
        }
    }
}

