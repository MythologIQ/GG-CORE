//! Inference output types for CORE Runtime.
//!
//! Each output variant maps to a specific inference capability.

/// Output variants for inference operations.
#[derive(Debug, Clone)]
pub enum InferenceOutput {
    Classification(ClassificationResult),
    Generation(GenerationResult),
    Embedding(EmbeddingResult),
    Entities(Vec<EntityResult>),
}

/// Result of text classification inference.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// Predicted label (highest confidence).
    pub label: String,
    /// Confidence score for predicted label (0.0–1.0).
    pub confidence: f32,
    /// All labels with their confidence scores, sorted by confidence descending.
    pub all_labels: Vec<(String, f32)>,
}

/// Result of text generation inference.
#[derive(Debug, Clone)]
pub struct GenerationResult {
    /// Generated text output.
    pub text: String,
    /// Number of tokens generated.
    pub tokens_generated: u32,
    /// Reason generation stopped.
    pub finish_reason: FinishReason,
}

/// Result of embedding generation.
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Dimensionality of the embedding.
    pub dimensions: usize,
}

/// Result of named entity recognition.
#[derive(Debug, Clone)]
pub struct EntityResult {
    /// Extracted entity text.
    pub text: String,
    /// Entity label/type (e.g., "PERSON", "ORG", "DATE").
    pub label: String,
    /// Start byte offset in source text.
    pub start: usize,
    /// End byte offset in source text.
    pub end: usize,
    /// Confidence score (0.0–1.0).
    pub confidence: f32,
}

/// Reason why text generation finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// Model emitted stop token naturally.
    Stop,
    /// Hit max_tokens limit.
    MaxTokens,
    /// Hit timeout_ms limit.
    Timeout,
    /// Content filter triggered.
    ContentFiltered,
    /// Request was cancelled by caller.
    Cancelled,
}

impl InferenceOutput {
    /// Returns true if this is a classification result.
    pub fn is_classification(&self) -> bool {
        matches!(self, Self::Classification(_))
    }

    /// Returns true if this is a generation result.
    pub fn is_generation(&self) -> bool {
        matches!(self, Self::Generation(_))
    }

    /// Returns true if this is an embedding result.
    pub fn is_embedding(&self) -> bool {
        matches!(self, Self::Embedding(_))
    }
}
