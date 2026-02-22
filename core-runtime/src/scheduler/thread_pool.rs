//! Configurable Thread Pool for optimized parallel inference.
//!
//! Provides work-stealing thread pool with configurable thread counts,
//! priority queues, and affinity settings for optimal CPU utilization.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub use super::thread_pool_types::*;
use super::thread_pool_types::PrioritizedTask;

/// Worker thread state.
struct Worker {
    queue: Arc<Mutex<VecDeque<PrioritizedTask>>>,
    active: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// Configurable thread pool with work stealing.
pub struct ThreadPool {
    workers: Vec<Worker>,
    config: ThreadPoolConfig,
    stats: Arc<RwLock<ThreadPoolStats>>,
    task_sequence: AtomicU64,
    shutdown: Arc<AtomicBool>,
    condvar: Arc<(Mutex<bool>, Condvar)>,
    global_queue: Arc<Mutex<VecDeque<PrioritizedTask>>>,
    _all_queues: Vec<Arc<Mutex<VecDeque<PrioritizedTask>>>>,
}

impl ThreadPool {
    pub fn new(config: ThreadPoolConfig) -> Self {
        let num_threads = if config.num_threads == 0 { num_cpus::get().max(1) } else { config.num_threads };
        let shutdown = Arc::new(AtomicBool::new(false));
        let condvar = Arc::new((Mutex::new(false), Condvar::new()));
        let global_queue = Arc::new(Mutex::new(VecDeque::with_capacity(config.queue_size)));
        let stats = Arc::new(RwLock::new(ThreadPoolStats::default()));

        let all_queues: Vec<Arc<Mutex<VecDeque<PrioritizedTask>>>> = (0..num_threads)
            .map(|_| Arc::new(Mutex::new(VecDeque::with_capacity(config.queue_size))))
            .collect();

        let mut workers = Vec::with_capacity(num_threads);
        for id in 0..num_threads {
            let queue = all_queues[id].clone();
            let queue_for_worker = queue.clone();
            let steal_queues = all_queues.clone();
            let active = Arc::new(AtomicBool::new(false));
            let args = WorkerArgs {
                id, queue, steal_queues, active: active.clone(),
                shutdown: shutdown.clone(), condvar: condvar.clone(),
                global_queue: global_queue.clone(), stats: stats.clone(),
                config: config.clone(),
            };
            let thread_name = format!("{}-{}", config.thread_name_prefix, id);
            let handle = thread::Builder::new()
                .name(thread_name)
                .stack_size(if config.stack_size > 0 { config.stack_size } else { 0 })
                .spawn(move || worker_loop(args))
                .expect("Failed to spawn worker thread");
            workers.push(Worker { queue: queue_for_worker, active, handle: Some(handle) });
        }

        Self {
            workers, config, stats, task_sequence: AtomicU64::new(0),
            shutdown, condvar, global_queue, _all_queues: all_queues,
        }
    }

    pub fn submit(&self, task: Task) -> Result<(), ThreadPoolError> {
        self.submit_with_priority(task, TaskPriority::Normal)
    }

    pub fn submit_with_priority(&self, task: Task, priority: TaskPriority) -> Result<(), ThreadPoolError> {
        if self.shutdown.load(Ordering::SeqCst) { return Err(ThreadPoolError::PoolShutdown); }
        let prioritized = PrioritizedTask {
            task, priority,
            sequence: self.task_sequence.fetch_add(1, Ordering::SeqCst),
        };
        let min_id = self.find_least_loaded_worker();
        let queue = if let Some(id) = min_id { self.workers[id].queue.clone() } else { self.global_queue.clone() };
        {
            let mut q = lock_or_recover(&queue);
            if q.len() >= self.config.queue_size { return Err(ThreadPoolError::QueueFull); }
            let pos = q.iter().position(|t| {
                t.priority < prioritized.priority
                    || (t.priority == prioritized.priority && t.sequence > prioritized.sequence)
            }).unwrap_or(q.len());
            q.insert(pos, prioritized);
        }
        let (lock, cvar) = &*self.condvar;
        { let _g = lock_or_recover(lock); cvar.notify_one(); }
        Ok(())
    }

    fn find_least_loaded_worker(&self) -> Option<usize> {
        self.workers.iter().enumerate()
            .min_by_key(|(_, w)| lock_or_recover(&w.queue).len())
            .map(|(i, _)| i)
    }

    pub fn stats(&self) -> ThreadPoolStats {
        let mut stats = read_or_recover(&self.stats).clone();
        stats.threads_active = self.workers.iter().filter(|w| w.active.load(Ordering::SeqCst)).count();
        stats.threads_idle = self.workers.len() - stats.threads_active;
        stats
    }

    pub fn num_threads(&self) -> usize { self.workers.len() }
    pub fn is_shutdown(&self) -> bool { self.shutdown.load(Ordering::SeqCst) }

    pub fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*self.condvar;
        { let _g = lock_or_recover(lock); cvar.notify_all(); }
    }

    pub fn join(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*self.condvar;
        { let _g = lock_or_recover(lock); cvar.notify_all(); }
        for worker in self.workers.drain(..) {
            if let Some(handle) = worker.handle { let _ = handle.join(); }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*self.condvar;
        { let _g = lock_or_recover(lock); cvar.notify_all(); }
        for worker in self.workers.drain(..) {
            if let Some(handle) = worker.handle { let _ = handle.join(); }
        }
    }
}

struct WorkerArgs {
    id: usize,
    queue: Arc<Mutex<VecDeque<PrioritizedTask>>>,
    steal_queues: Vec<Arc<Mutex<VecDeque<PrioritizedTask>>>>,
    active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    condvar: Arc<(Mutex<bool>, Condvar)>,
    global_queue: Arc<Mutex<VecDeque<PrioritizedTask>>>,
    stats: Arc<RwLock<ThreadPoolStats>>,
    config: ThreadPoolConfig,
}

fn worker_loop(args: WorkerArgs) {
    let idle_timeout = Duration::from_millis(args.config.idle_timeout_ms);
    while !args.shutdown.load(Ordering::SeqCst) {
        let task = lock_or_recover(&args.queue).pop_front();
        let task = match task {
            Some(t) => Some(t),
            None => {
                if let Some(t) = lock_or_recover(&args.global_queue).pop_front() {
                    Some(t)
                } else if args.config.enable_work_stealing {
                    try_steal(args.id, &args.steal_queues)
                } else { None }
            }
        };
        if let Some(prioritized) = task {
            args.active.store(true, Ordering::SeqCst);
            let start = Instant::now();
            (prioritized.task)();
            let exec_us = start.elapsed().as_micros() as u64;
            if let Ok(mut s) = args.stats.write() {
                s.total_tasks_executed += 1;
                if prioritized.priority >= TaskPriority::High { s.high_priority_tasks += 1; }
                if s.avg_exec_time_us == 0 { s.avg_exec_time_us = exec_us; }
                else { s.avg_exec_time_us = (s.avg_exec_time_us * 9 + exec_us) / 10; }
            }
            args.active.store(false, Ordering::SeqCst);
        } else {
            let (lock, cvar) = &*args.condvar;
            let guard = lock_or_recover(lock);
            let _ = cvar.wait_timeout(guard, idle_timeout);
        }
    }
}

fn try_steal(
    worker_id: usize,
    all_queues: &[Arc<Mutex<VecDeque<PrioritizedTask>>>],
) -> Option<PrioritizedTask> {
    for (id, target) in all_queues.iter().enumerate() {
        if id == worker_id { continue; }
        if let Some(task) = lock_or_recover(target).pop_back() {
            return Some(task);
        }
    }
    None
}

#[cfg(test)]
#[path = "thread_pool_tests.rs"]
mod tests;
