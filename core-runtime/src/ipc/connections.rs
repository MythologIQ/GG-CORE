//! Connection pool management with limits.
//!
//! Provides global connection limiting with RAII guards.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Configuration for connection pool.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub max_connections: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self { max_connections: 64 }
    }
}

/// Global connection pool with atomic counting.
pub struct ConnectionPool {
    active: AtomicUsize,
    config: ConnectionConfig,
}

impl ConnectionPool {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            active: AtomicUsize::new(0),
            config,
        }
    }

    /// Try to acquire a connection slot. Returns guard if available.
    pub fn try_acquire(&self) -> Option<ConnectionGuard<'_>> {
        loop {
            let current = self.active.load(Ordering::Relaxed);
            if current >= self.config.max_connections {
                return None;
            }

            // CAS to atomically increment
            if self
                .active
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return Some(ConnectionGuard { pool: self });
            }
            // CAS failed, retry
        }
    }

    /// Current number of active connections.
    pub fn active_count(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }

    /// Maximum allowed connections.
    pub fn max_connections(&self) -> usize {
        self.config.max_connections
    }

    /// Try to acquire a connection slot with owned Arc guard.
    /// Suitable for spawned tasks that require `'static` lifetime.
    pub fn try_acquire_owned(self: &Arc<Self>) -> Option<OwnedConnectionGuard> {
        loop {
            let current = self.active.load(Ordering::Relaxed);
            if current >= self.config.max_connections {
                return None;
            }
            if self
                .active
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return Some(OwnedConnectionGuard {
                    pool: Arc::clone(self),
                });
            }
        }
    }

    fn release(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// RAII guard that releases connection on drop.
pub struct ConnectionGuard<'a> {
    pool: &'a ConnectionPool,
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        self.pool.release();
    }
}

/// Owned RAII guard for spawned tasks (`'static` lifetime).
pub struct OwnedConnectionGuard {
    pool: Arc<ConnectionPool>,
}

impl Drop for OwnedConnectionGuard {
    fn drop(&mut self) {
        self.pool.release();
    }
}
