//! Smart loader loading operations.
//!
//! Extracted from `smart_loader.rs` for Section 4 compliance.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use super::registry::ModelHandle;
use super::smart_loader::{SmartLoaderError};
use super::smart_loader_types::*;

/// Execute background preload for a model.
pub(super) fn preload_background(
    models: Arc<RwLock<HashMap<String, ModelEntry>>>,
    model_id: &str,
    callback: Option<Arc<LoadCallback>>,
) {
    let model_id = model_id.to_string();

    tokio::spawn(async move {
        let path = {
            let mut models = models.write().await;
            let entry = match models.get_mut(&model_id) {
                Some(e) if e.state == LoadState::Registered => e,
                _ => return,
            };
            entry.state = LoadState::Loading;
            entry.path.clone()
        };

        let start = Instant::now();
        let result = execute_load(&path, callback.as_deref());
        let load_ms = start.elapsed().as_millis() as u64;

        let mut models = models.write().await;
        if let Some(entry) = models.get_mut(&model_id) {
            match result {
                Ok(handle) => {
                    entry.state = LoadState::Ready;
                    entry.handle = Some(handle);
                    entry.load_time_ms = Some(load_ms);
                }
                Err(_) => {
                    entry.state = LoadState::Failed;
                }
            }
        }
    });
}

/// Synchronous model load (blocks until complete).
pub(super) async fn load_sync(
    models: &RwLock<HashMap<String, ModelEntry>>,
    model_id: &str,
    callback: Option<&Arc<LoadCallback>>,
    active_tier: &RwLock<Option<ModelTier>>,
    semaphore: &tokio::sync::Semaphore,
) -> Result<(), SmartLoaderError> {
    let _permit = semaphore.acquire().await.unwrap();

    let path = {
        let mut models = models.write().await;
        let entry = models
            .get_mut(model_id)
            .ok_or_else(|| SmartLoaderError::NotRegistered(model_id.to_string()))?;

        if entry.state == LoadState::Ready {
            return Ok(());
        }

        entry.state = LoadState::Loading;
        entry.path.clone()
    };

    let start = Instant::now();
    let result = execute_load(&path, callback.map(|c| c.as_ref()));
    let load_ms = start.elapsed().as_millis() as u64;

    let mut models = models.write().await;
    let entry = models.get_mut(model_id).unwrap();

    match result {
        Ok(handle) => {
            entry.state = LoadState::Ready;
            entry.handle = Some(handle);
            entry.load_time_ms = Some(load_ms);
            entry.last_used = Some(Instant::now());
            entry.use_count += 1;
            *active_tier.write().await = Some(entry.tier);
            Ok(())
        }
        Err(e) => {
            entry.state = LoadState::Failed;
            Err(SmartLoaderError::LoadFailed(e))
        }
    }
}

/// Execute a model load via callback or mmap fallback.
fn execute_load(path: &PathBuf, callback: Option<&LoadCallback>) -> Result<ModelHandle, String> {
    if let Some(cb) = callback {
        cb(path)
    } else {
        std::fs::File::open(path)
            .and_then(|f| unsafe { memmap2::Mmap::map(&f) })
            .map(|mmap| {
                let _ = mmap.get(0);
                ModelHandle::new(1)
            })
            .map_err(|e| e.to_string())
    }
}
