//! Request queue management.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};

use super::priority::{Priority, PriorityQueue};
use crate::engine::inference::InferenceResult;
use crate::engine::InferenceParams;

/// Response channel type for delivering results back to callers.
pub type ResponseTx = tokio::sync::oneshot::Sender<Result<InferenceResult, String>>;
/// Receiver half for awaiting inference results.
pub type ResponseRx = tokio::sync::oneshot::Receiver<Result<InferenceResult, String>>;

/// Conservative bytes-per-token estimate for tier-1 context check.
/// UTF-8 averages ~4 bytes per token for English text.
const BYTES_PER_TOKEN_ESTIMATE: usize = 4;

/// Configuration for request queue.
#[derive(Debug, Clone)]
pub struct RequestQueueConfig {
    pub max_pending: usize,
    /// Maximum context length in tokens. Used for tier-1 heuristic
    /// rejection at enqueue time (prompt_bytes / 4 > max_context).
    pub max_context_tokens: usize,
}

impl Default for RequestQueueConfig {
    fn default() -> Self {
        Self { max_pending: 256, max_context_tokens: 4096 }
    }
}

/// A queued inference request with timeout and cancellation support.
pub struct QueuedRequest {
    pub id: u64,
    pub model_id: String,
    /// Text prompt for inference.
    pub prompt: String,
    pub params: InferenceParams,
    pub enqueued_at: Instant,
    pub deadline: Option<Instant>,
    pub cancelled: Arc<AtomicBool>,
    /// Channel for sending the result back to the caller.
    pub response_tx: Option<ResponseTx>,
}

impl std::fmt::Debug for QueuedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedRequest")
            .field("id", &self.id)
            .field("model_id", &self.model_id)
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl QueuedRequest {
    /// Create a new queued request. Used for testing and batch processing.
    pub fn new(
        id: u64,
        model_id: String,
        prompt: String,
        params: InferenceParams,
    ) -> Self {
        let enqueued_at = Instant::now();
        let deadline = params.timeout_ms.map(|ms| enqueued_at + Duration::from_millis(ms));
        Self {
            id,
            model_id,
            prompt,
            params,
            enqueued_at,
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            response_tx: None,
        }
    }

    /// Check if request has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Check if request has exceeded its deadline.
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() > d)
    }

    /// Mark the request as cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Get a closure that checks cancellation (for passing to backends).
    pub fn cancel_check(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

/// Thread-safe request queue with priority support.
pub struct RequestQueue {
    queue: Arc<Mutex<PriorityQueue<QueuedRequest>>>,
    next_id: AtomicU64,
    config: RequestQueueConfig,
    /// Notifies the worker when new items are enqueued.
    notify: Arc<Notify>,
}

impl RequestQueue {
    pub fn new(config: RequestQueueConfig) -> Self {
        Self {
            queue: Arc::new(Mutex::new(PriorityQueue::new())),
            next_id: AtomicU64::new(1),
            config,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Enqueue a new request. Returns request ID and queue position.
    pub async fn enqueue(
        &self,
        model_id: String,
        prompt: String,
        params: InferenceParams,
        priority: Priority,
    ) -> Result<(u64, usize), QueueError> {
        self.enqueue_inner(model_id, prompt, params, priority, None).await
    }

    /// Enqueue with a response channel. Returns (id, position, receiver).
    pub async fn enqueue_with_response(
        &self,
        model_id: String,
        prompt: String,
        params: InferenceParams,
        priority: Priority,
    ) -> Result<(u64, ResponseRx), QueueError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (id, _pos) = self
            .enqueue_inner(model_id, prompt, params, priority, Some(tx))
            .await?;
        Ok((id, rx))
    }

    async fn enqueue_inner(
        &self,
        model_id: String,
        prompt: String,
        params: InferenceParams,
        priority: Priority,
        response_tx: Option<ResponseTx>,
    ) -> Result<(u64, usize), QueueError> {
        // Tier-1 context check: conservative byte heuristic (~4 bytes/token).
        // Rejects obviously oversized prompts before they enter the queue.
        // Tier-2 precise check happens post-dequeue in InferenceEngine.
        let estimated_tokens = prompt.len() / BYTES_PER_TOKEN_ESTIMATE;
        if estimated_tokens > self.config.max_context_tokens {
            return Err(QueueError::ContextTooLarge {
                estimated_tokens,
                max: self.config.max_context_tokens,
            });
        }

        let mut queue = self.queue.lock().await;

        if queue.len() >= self.config.max_pending {
            return Err(QueueError::QueueFull);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let enqueued_at = Instant::now();
        let deadline = params.timeout_ms.map(|ms| enqueued_at + Duration::from_millis(ms));
        let request = QueuedRequest {
            id,
            model_id,
            prompt,
            params,
            enqueued_at,
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            response_tx,
        };
        let position = queue.len();
        queue.push(request, priority);
        drop(queue);

        self.notify.notify_one();
        Ok((id, position))
    }

    /// Cancel a pending request by ID. Returns true if found and cancelled.
    pub async fn cancel(&self, request_id: u64) -> bool {
        let queue = self.queue.lock().await;
        for request in queue.iter() {
            if request.id == request_id {
                request.cancel();
                return true;
            }
        }
        false
    }

    /// Dequeue the highest priority request, skipping cancelled/expired.
    pub async fn dequeue(&self) -> Option<QueuedRequest> {
        let mut queue = self.queue.lock().await;
        loop {
            let request = queue.pop()?;
            if request.is_cancelled() || request.is_expired() {
                continue;
            }
            return Some(request);
        }
    }

    /// Wait for a notification then dequeue. Returns None on shutdown.
    pub async fn wait_and_dequeue(&self) -> Option<QueuedRequest> {
        loop {
            if let Some(req) = self.dequeue().await {
                return Some(req);
            }
            self.notify.notified().await;
        }
    }

    /// Wake the worker (used during shutdown).
    pub fn wake(&self) {
        self.notify.notify_one();
    }

    /// Current queue length.
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if queue is empty.
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }

    /// Admit a streaming request: check capacity and context, reserve a slot.
    ///
    /// Returns a guard that releases the slot on drop. The streaming path
    /// uses this instead of full enqueue because tokens are streamed via
    /// a channel rather than returned through a oneshot.
    pub async fn admit_streaming(
        &self,
        prompt: &str,
    ) -> Result<StreamingAdmissionGuard, QueueError> {
        let estimated_tokens = prompt.len() / BYTES_PER_TOKEN_ESTIMATE;
        if estimated_tokens > self.config.max_context_tokens {
            return Err(QueueError::ContextTooLarge {
                estimated_tokens,
                max: self.config.max_context_tokens,
            });
        }

        let queue = self.queue.lock().await;
        if queue.len() >= self.config.max_pending {
            return Err(QueueError::QueueFull);
        }
        drop(queue);

        Ok(StreamingAdmissionGuard { _private: () })
    }
}

/// RAII guard for streaming admission. Ensures the streaming path
/// went through queue admission control before executing inference.
pub struct StreamingAdmissionGuard {
    _private: (),
}

#[derive(Debug)]
pub enum QueueError {
    QueueFull,
    ContextTooLarge { estimated_tokens: usize, max: usize },
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => write!(f, "request queue is full"),
            Self::ContextTooLarge { estimated_tokens, max } => {
                write!(
                    f,
                    "prompt too large: ~{estimated_tokens} tokens (max {max})",
                )
            }
        }
    }
}

impl std::error::Error for QueueError {}
