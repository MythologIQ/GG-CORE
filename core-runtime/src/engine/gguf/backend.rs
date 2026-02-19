//! llama-cpp-2 backend for GGUF inference.
//!
//! Model loading, context creation, and token generation
//! via the llama-cpp-2 Rust bindings.

use std::num::NonZeroU32;
use std::path::Path;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::engine::{
    FinishReason, GenerationResult, InferenceConfig, InferenceError,
};

/// Holds the loaded llama-cpp-2 model and backend.
pub struct LlamaBackendInner {
    backend: LlamaBackend,
    model: LlamaModel,
    n_ctx: u32,
    n_threads: i32,
}

// SAFETY: LlamaModel and LlamaBackend are Send+Sync in llama-cpp-2.
unsafe impl Send for LlamaBackendInner {}
unsafe impl Sync for LlamaBackendInner {}

impl LlamaBackendInner {
    /// Load a GGUF model from disk.
    pub fn load(
        path: &Path,
        config: &super::GgufConfig,
    ) -> Result<Self, InferenceError> {
        let backend = LlamaBackend::init().map_err(|e| {
            InferenceError::ModelError(format!("backend init: {e}"))
        })?;
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(config.n_gpu_layers);
        let model = LlamaModel::load_from_file(&backend, path, &model_params)
            .map_err(|e| InferenceError::ModelError(format!("load: {e}")))?;
        let n_threads = resolve_threads(config.n_threads);
        Ok(Self { backend, model, n_ctx: config.n_ctx, n_threads })
    }

    pub fn model_size(&self) -> usize { self.model.size() as usize }

    /// Generate text from a prompt using llama-cpp-2.
    pub fn generate(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<GenerationResult, InferenceError> {
        let tokens = self.tokenize(prompt)?;
        let max_tok = config.max_tokens.unwrap_or(256);
        let mut ctx = self.create_context()?;
        let (out_tokens, reason) =
            self.sample_loop(&mut ctx, &tokens, max_tok, config)?;
        let text = self.detokenize(&out_tokens)?;
        let count = u32::try_from(out_tokens.len()).unwrap_or(u32::MAX);
        Ok(GenerationResult { text, tokens_generated: count, finish_reason: reason })
    }

    /// Stream tokens one at a time through a channel.
    pub fn generate_stream(
        &self,
        prompt: &str,
        config: &InferenceConfig,
        sender: crate::engine::TokenStreamSender,
    ) -> Result<(), InferenceError> {
        let tokens = self.tokenize(prompt)?;
        let max_tok = config.max_tokens.unwrap_or(256);
        let mut ctx = self.create_context()?;
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        add_seq(&mut batch, &tokens)?;
        decode(&mut ctx, &mut batch)?;
        let mut sampler = build_sampler(config);
        sampler.accept_many(tokens.iter().copied());
        let mut pos = tokens.len() as i32;
        let rt = tokio::runtime::Handle::current();
        for i in 0..max_tok {
            let tok = sampler.sample(&ctx, pos - 1);
            sampler.accept(tok);
            let eog = self.model.is_eog_token(tok);
            let is_final = eog || i + 1 == max_tok;
            if rt.block_on(sender.send(tok.0 as u32, is_final)).is_err() {
                break;
            }
            if eog { break; }
            batch.clear();
            add_one(&mut batch, tok, pos)?;
            decode(&mut ctx, &mut batch)?;
            pos += 1;
        }
        Ok(())
    }

    /// Tokenize a prompt string.
    pub fn tokenize(&self, text: &str) -> Result<Vec<LlamaToken>, InferenceError> {
        self.model.str_to_token(text, AddBos::Always).map_err(|e| {
            InferenceError::InputValidation(format!("tokenize: {e}"))
        })
    }

    /// Convert token IDs back to a string.
    pub fn detokenize(&self, tokens: &[LlamaToken]) -> Result<String, InferenceError> {
        let mut dec = encoding_rs::UTF_8.new_decoder();
        let mut out = String::new();
        for &t in tokens {
            let piece = self.model.token_to_piece(t, &mut dec, false, None)
                .map_err(|e| InferenceError::ModelError(format!("detok: {e}")))?;
            out.push_str(&piece);
        }
        Ok(out)
    }

    fn create_context(&self) -> Result<LlamaContext<'_>, InferenceError> {
        let p = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.n_ctx))
            .with_n_threads(self.n_threads)
            .with_n_threads_batch(self.n_threads);
        self.model.new_context(&self.backend, p)
            .map_err(|e| InferenceError::ModelError(format!("ctx: {e}")))
    }

    fn sample_loop(
        &self,
        ctx: &mut LlamaContext<'_>,
        tokens: &[LlamaToken],
        max_tok: u32,
        config: &InferenceConfig,
    ) -> Result<(Vec<LlamaToken>, FinishReason), InferenceError> {
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        add_seq(&mut batch, tokens)?;
        decode(ctx, &mut batch)?;
        let mut sampler = build_sampler(config);
        sampler.accept_many(tokens.iter().copied());
        let mut out = Vec::new();
        let mut pos = tokens.len() as i32;
        for _ in 0..max_tok {
            let tok = sampler.sample(ctx, pos - 1);
            sampler.accept(tok);
            if self.model.is_eog_token(tok) {
                return Ok((out, FinishReason::Stop));
            }
            out.push(tok);
            batch.clear();
            add_one(&mut batch, tok, pos)?;
            decode(ctx, &mut batch)?;
            pos += 1;
        }
        Ok((out, FinishReason::MaxTokens))
    }
}

fn add_seq(batch: &mut LlamaBatch, tokens: &[LlamaToken]) -> Result<(), InferenceError> {
    batch.add_sequence(tokens, 0, false)
        .map_err(|e| InferenceError::ModelError(format!("batch: {e}")))
}

fn add_one(batch: &mut LlamaBatch, tok: LlamaToken, pos: i32) -> Result<(), InferenceError> {
    batch.add(tok, pos, &[0], true)
        .map_err(|e| InferenceError::ModelError(format!("batch: {e}")))
}

fn decode(ctx: &mut LlamaContext<'_>, batch: &mut LlamaBatch) -> Result<(), InferenceError> {
    ctx.decode(batch).map_err(|e| InferenceError::ModelError(format!("decode: {e}")))
}

fn build_sampler(config: &InferenceConfig) -> LlamaSampler {
    let mut s = Vec::new();
    if config.repetition_penalty > 1.0 {
        s.push(LlamaSampler::penalties(64, config.repetition_penalty, 0.0, 0.0));
    }
    if config.top_k > 0 {
        s.push(LlamaSampler::top_k(config.top_k as i32));
    }
    s.push(LlamaSampler::top_p(config.top_p as f32, 1));
    s.push(LlamaSampler::temp(config.temperature));
    s.push(LlamaSampler::dist(42));
    LlamaSampler::chain_simple(s)
}

fn resolve_threads(n: u32) -> i32 {
    if n == 0 {
        i32::try_from(num_cpus::get().max(1).min(8)).unwrap_or(4)
    } else {
        i32::try_from(n).unwrap_or(4)
    }
}
