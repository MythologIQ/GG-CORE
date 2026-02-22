//! Model management module for CORE Runtime.
//!
//! Handles model loading, registry tracking, manifest parsing, and hot-swap operations.

pub mod manifest;
pub mod pool;
pub mod smart_loader;
mod smart_loader_ops;
pub mod smart_loader_types;
pub mod tier_synergy;

mod drain;
pub mod lifecycle;
mod loader;
mod preload;
pub mod registry;
mod router;
mod swap;

// v0.5.0: Model registry enhancements
pub mod history;
pub mod persistence;
pub mod search;
pub mod version;

pub use drain::{DrainError, FlightGuard, FlightTracker};
pub use lifecycle::{LifecycleError, ModelLifecycle};
pub use history::{VersionHistory, VersionHistoryEntry, VersionSource};
pub use loader::{LoadError, MappedModel, ModelLoader, ModelMetadata, ModelPath};
pub use manifest::{ModelArchitecture, ModelCapability, ModelManifest};
pub use persistence::{PersistenceError, PersistedModel, RegistryPersistence, RegistryState};
pub use pool::{ModelPool, PoolConfig, PoolError, PoolMetrics, PoolStatus, SwitchResult};
pub use pool::ModelTier as PoolModelTier;
pub use preload::{ModelPreloader, PreloadError, PreloadedModel};
pub use registry::{LoadedModelInfo, LoadedModelState, ModelHandle, ModelRegistry};
pub use router::{ModelRouter, RouterError};
pub use search::{ModelQuery, ModelQueryBuilder, ModelSearchResult};
pub use smart_loader::{LoadHint, SmartLoader, SmartLoaderConfig, SmartLoaderError, SmartLoaderMetrics, SmartLoaderStatus};
pub use smart_loader::ModelTier as SmartModelTier;
pub use swap::{SwapError, SwapManager, SwapResult};
pub use tier_synergy::{SynergyMode, SynergyResult, SynergyStatus, TierSynergy};
pub use version::{ModelVersion, VersionRange};
