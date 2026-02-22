//! Inference engine module for CORE Runtime.
//!
//! Handles tokenization, inference execution, and token streaming.
//! Provides the `InferenceModel` trait and supporting types.

pub mod config;
pub mod decode;
pub mod error;
pub mod filter;
pub mod flash_attn;
pub mod flash_attn_gpu;
pub mod gguf;
pub mod gpu;
pub mod gpu_manager;
pub mod gpu_pool;
pub mod input;
pub mod onnx;
pub mod output;
pub mod prefill;
pub mod quantize;
pub mod simd_matmul;
mod simd_neon;
pub mod simd_tokenizer;
pub mod simd_tokenizer_v2;
pub mod speculative;
pub mod speculative_v2;

// GPU backend modules (conditionally compiled)
#[cfg(feature = "cuda")]
pub mod cuda;
#[cfg(all(feature = "metal", target_os = "macos"))]
pub mod metal;
pub mod moe;
pub mod multi_gpu;
pub mod multi_gpu_partition;

pub mod inference;
pub mod inference_types;
mod streaming;
mod tokenizer;

pub use config::InferenceConfig;
pub use decode::{DecodeConfig, DecodeExecutor, DecodeStepResult};
pub use error::InferenceError;
pub use filter::{FilterConfig, OutputFilter};
pub use flash_attn::{FlashAttn, FlashAttnConfig};
pub use flash_attn_gpu::{FlashAttnGpuConfig, FlashAttnGpuError, FlashAttnGpuKernel};
pub use inference::{InferenceEngine, InferenceParams, InferenceResult};
pub use input::{ChatMessage, ChatRole, InferenceInput};
pub use input::{MAX_BATCH_SIZE, MAX_INPUT_TOKENS, MAX_TEXT_BYTES};
pub use output::{ClassificationResult, EmbeddingResult, EntityResult};
pub use output::{FinishReason, GenerationResult, InferenceOutput};
pub use prefill::{PrefillConfig, PrefillExecutor, PrefillResult};
pub use quantize::{QuantFormat, QuantizedTensor, QUANT_BLOCK_SIZE};
pub use simd_matmul::{dot_q4, dot_q8, init_simd};
pub use simd_tokenizer::SimdTokenizer;
pub use simd_tokenizer_v2::{
    SimdTokenizer as SimdTokenizerV2, TokenizerError as TokenizerV2Error, TokenizerStats,
};
pub use speculative::{
    DraftModel, SpeculativeConfig, SpeculativeDecoder, TargetModel, VerifyResult,
};
pub use speculative_v2::{
    SpeculativeConfig as SpeculativeV2Config, SpeculativeDecoder as SpeculativeV2Decoder,
    SpeculativeStats,
};
pub use streaming::{StreamingOutput, TokenStream, TokenStreamSender};
pub use tokenizer::{TokenizerError, TokenizerWrapper};

// Backend re-exports
pub use gguf::{GgufConfig, GgufGenerator, GgufModel};
#[cfg(feature = "gguf")]
pub use gguf::LlamaBackendInner;
pub use gpu::{GpuBackend, GpuConfig, GpuDevice, GpuError, GpuMemory};
pub use gpu_manager::GpuManager;
pub use gpu_pool::GpuMemoryPool;
pub use onnx::{OnnxClassifier, OnnxConfig, OnnxEmbedder, OnnxModel};

// CUDA backend re-exports
#[cfg(feature = "cuda")]
pub use cuda::{
    CudaBackend, CudaDeviceInfo, CudaError, CudaExecutionStream, CudaMemoryBuffer, FlashAttention,
};

// Metal backend re-exports
#[cfg(all(feature = "metal", target_os = "macos"))]
pub use metal::{
    MetalBackend, MetalBuffer, MetalCommandEncoder, MetalComputePipeline, MetalDeviceInfo,
    MetalError, MetalGpuFamily,
};

// Multi-GPU support
pub use multi_gpu::{GpuPartition, MultiGpuConfig, MultiGpuError, MultiGpuManager, MultiGpuStrategy};
pub use multi_gpu_partition::CrossGpuCommunication;

// Mixture of Experts support
pub use moe::{
    ExpertCombiner, ExpertDeviceAssignment, ExpertOutput, LinearRouter, MoeConfig, MoeError,
    MoeExecutor, MoeRouter, RoutingDecision,
};

/// What a model can do — used by the InferenceModel trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceCapability {
    TextClassification,
    TextGeneration,
    Embedding,
    NamedEntityRecognition,
}
