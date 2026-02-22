//! Types for the configurable thread pool.

use std::sync::Mutex;

/// Acquire a mutex lock, recovering from poison if a thread panicked.
#[inline]
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Mutex was poisoned, recovering to maintain availability");
        poisoned.into_inner()
    })
}

/// Acquire a read lock, recovering from poison if a thread panicked.
#[inline]
pub fn read_or_recover<T>(rwlock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    rwlock.read().unwrap_or_else(|poisoned| {
        tracing::warn!("RwLock was poisoned, recovering to maintain availability");
        poisoned.into_inner()
    })
}

/// Configuration for the thread pool.
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    pub num_threads: usize,
    pub enable_work_stealing: bool,
    pub queue_size: usize,
    pub stack_size: usize,
    pub thread_name_prefix: String,
    pub enable_priority: bool,
    pub idle_timeout_ms: u64,
    pub enable_affinity: bool,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            num_threads: 0,
            enable_work_stealing: true,
            queue_size: 256,
            stack_size: 0,
            thread_name_prefix: "core-worker".to_string(),
            enable_priority: true,
            idle_timeout_ms: 10,
            enable_affinity: false,
        }
    }
}

impl ThreadPoolConfig {
    pub fn inference_optimized() -> Self {
        Self {
            num_threads: 0,
            enable_work_stealing: true,
            queue_size: 512,
            stack_size: 2 * 1024 * 1024,
            thread_name_prefix: "inference".to_string(),
            enable_priority: true,
            idle_timeout_ms: 5,
            enable_affinity: true,
        }
    }

    pub fn batch_optimized() -> Self {
        Self {
            num_threads: 0,
            enable_work_stealing: true,
            queue_size: 1024,
            stack_size: 0,
            thread_name_prefix: "batch".to_string(),
            enable_priority: false,
            idle_timeout_ms: 50,
            enable_affinity: false,
        }
    }
}

/// Task priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// A task to be executed by the thread pool.
pub type Task = Box<dyn FnOnce() + Send + 'static>;

/// Prioritized task wrapper.
pub(super) struct PrioritizedTask {
    pub task: Task,
    pub priority: TaskPriority,
    pub sequence: u64,
}

/// Statistics for thread pool performance.
#[derive(Debug, Default, Clone)]
pub struct ThreadPoolStats {
    pub total_tasks_executed: u64,
    pub high_priority_tasks: u64,
    pub work_steals: u64,
    pub queue_overflows: u64,
    pub avg_wait_time_us: u64,
    pub avg_exec_time_us: u64,
    pub threads_active: usize,
    pub threads_idle: usize,
}

/// Errors for thread pool operations.
#[derive(Debug, thiserror::Error)]
pub enum ThreadPoolError {
    #[error("Thread pool is shut down")]
    PoolShutdown,
    #[error("Task queue is full")]
    QueueFull,
    #[error("Failed to spawn thread: {0}")]
    ThreadSpawnFailed(String),
}
