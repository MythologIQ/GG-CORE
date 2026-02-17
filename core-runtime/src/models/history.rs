// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Version history tracking for models.
//!
//! Maintains a rolling history of model version activations for rollback support.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::version::ModelVersion;

/// Maximum history entries per model (default).
pub const DEFAULT_MAX_HISTORY: usize = 10;

/// How a version was activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionSource {
    /// Initial load at startup.
    Initial,
    /// Hot-swap from previous version.
    HotSwap,
    /// Rollback to previous version.
    Rollback,
    /// Manual load via API.
    Manual,
}

/// A single version history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryEntry {
    /// The version that was activated.
    pub version: ModelVersion,
    /// Unix timestamp when activated.
    pub activated_at: u64,
    /// Unix timestamp when deactivated (None if current).
    pub deactivated_at: Option<u64>,
    /// How this version was activated.
    pub source: VersionSource,
}

impl VersionHistoryEntry {
    /// Create a new history entry for current time.
    pub fn new(version: ModelVersion, source: VersionSource) -> Self {
        Self {
            version,
            activated_at: current_timestamp(),
            deactivated_at: None,
            source,
        }
    }

    /// Check if this entry is currently active.
    pub fn is_active(&self) -> bool {
        self.deactivated_at.is_none()
    }

    /// Mark this entry as deactivated.
    pub fn deactivate(&mut self) {
        self.deactivated_at = Some(current_timestamp());
    }
}

/// Rolling version history for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    entries: VecDeque<VersionHistoryEntry>,
    max_entries: usize,
}

impl VersionHistory {
    /// Create a new history with default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_HISTORY)
    }

    /// Create a history with custom capacity.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Record a new version activation.
    pub fn record(&mut self, version: ModelVersion, source: VersionSource) {
        // Deactivate current entry if exists
        if let Some(current) = self.entries.back_mut() {
            if current.is_active() {
                current.deactivate();
            }
        }

        // Add new entry
        let entry = VersionHistoryEntry::new(version, source);
        self.entries.push_back(entry);

        // Trim if over capacity
        while self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    /// Get the current (most recent active) version.
    pub fn current(&self) -> Option<&VersionHistoryEntry> {
        self.entries.back()
    }

    /// Get the previous version (for rollback).
    pub fn previous(&self) -> Option<&VersionHistoryEntry> {
        if self.entries.len() >= 2 {
            self.entries.get(self.entries.len() - 2)
        } else {
            None
        }
    }

    /// Get all history entries.
    pub fn all(&self) -> &VecDeque<VersionHistoryEntry> {
        &self.entries
    }

    /// Get number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for VersionHistory {
    fn default() -> Self {
        Self::new()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_recording() {
        let mut history = VersionHistory::new();

        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        assert_eq!(history.len(), 1);
        assert!(history.current().unwrap().is_active());

        history.record(ModelVersion::new(1, 1, 0), VersionSource::HotSwap);
        assert_eq!(history.len(), 2);

        let prev = history.previous().unwrap();
        assert!(!prev.is_active()); // Should be deactivated
    }

    #[test]
    fn test_history_capacity() {
        let mut history = VersionHistory::with_capacity(3);

        for i in 0..5 {
            history.record(ModelVersion::new(1, i, 0), VersionSource::Manual);
        }

        assert_eq!(history.len(), 3); // Should be trimmed
    }

    // --- Additional tests ---

    #[test]
    fn test_history_new() {
        let history = VersionHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_history_default() {
        let history = VersionHistory::default();
        assert!(history.is_empty());
    }

    #[test]
    fn test_version_history_entry_new() {
        let entry = VersionHistoryEntry::new(
            ModelVersion::new(1, 0, 0),
            VersionSource::Initial,
        );

        assert!(entry.is_active());
        assert!(entry.deactivated_at.is_none());
        assert!(entry.activated_at > 0);
        assert_eq!(entry.source, VersionSource::Initial);
    }

    #[test]
    fn test_version_history_entry_deactivate() {
        let mut entry = VersionHistoryEntry::new(
            ModelVersion::new(1, 0, 0),
            VersionSource::Initial,
        );

        assert!(entry.is_active());
        entry.deactivate();
        assert!(!entry.is_active());
        assert!(entry.deactivated_at.is_some());
    }

    #[test]
    fn test_current_on_empty_history() {
        let history = VersionHistory::new();
        assert!(history.current().is_none());
    }

    #[test]
    fn test_previous_on_empty_history() {
        let history = VersionHistory::new();
        assert!(history.previous().is_none());
    }

    #[test]
    fn test_previous_with_single_entry() {
        let mut history = VersionHistory::new();
        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        assert!(history.previous().is_none());
    }

    #[test]
    fn test_previous_with_two_entries() {
        let mut history = VersionHistory::new();
        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        history.record(ModelVersion::new(1, 1, 0), VersionSource::HotSwap);

        let prev = history.previous().unwrap();
        assert_eq!(prev.version, ModelVersion::new(1, 0, 0));
    }

    #[test]
    fn test_all_entries() {
        let mut history = VersionHistory::new();
        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        history.record(ModelVersion::new(1, 1, 0), VersionSource::HotSwap);
        history.record(ModelVersion::new(1, 2, 0), VersionSource::Manual);

        let all = history.all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_version_source_variants() {
        // Test all source variants
        let sources = vec![
            VersionSource::Initial,
            VersionSource::HotSwap,
            VersionSource::Rollback,
            VersionSource::Manual,
        ];

        for source in sources {
            let entry = VersionHistoryEntry::new(ModelVersion::new(1, 0, 0), source);
            assert_eq!(entry.source, source);
        }
    }

    #[test]
    fn test_history_capacity_boundary() {
        let mut history = VersionHistory::with_capacity(1);

        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        assert_eq!(history.len(), 1);

        history.record(ModelVersion::new(2, 0, 0), VersionSource::HotSwap);
        assert_eq!(history.len(), 1);

        // Should keep only the latest
        let current = history.current().unwrap();
        assert_eq!(current.version, ModelVersion::new(2, 0, 0));
    }

    #[test]
    fn test_history_serialization() {
        let mut history = VersionHistory::new();
        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        history.record(ModelVersion::new(1, 1, 0), VersionSource::HotSwap);

        let json = serde_json::to_string(&history).unwrap();
        let deserialized: VersionHistory = serde_json::from_str(&json).unwrap();

        assert_eq!(history.len(), deserialized.len());
    }

    #[test]
    fn test_version_history_entry_serialization() {
        let entry = VersionHistoryEntry::new(
            ModelVersion::new(1, 2, 3),
            VersionSource::Rollback,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: VersionHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.version, deserialized.version);
        assert_eq!(entry.source, deserialized.source);
    }

    #[test]
    fn test_default_max_history() {
        assert_eq!(DEFAULT_MAX_HISTORY, 10);
    }

    #[test]
    fn test_history_preserves_order() {
        let mut history = VersionHistory::new();

        for i in 0..5 {
            history.record(ModelVersion::new(1, i, 0), VersionSource::Manual);
        }

        let entries: Vec<_> = history.all().iter().collect();
        for (idx, entry) in entries.iter().enumerate() {
            assert_eq!(entry.version.minor, idx as u32);
        }
    }

    #[test]
    fn test_history_deactivates_on_new_record() {
        let mut history = VersionHistory::new();

        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        assert!(history.current().unwrap().is_active());

        history.record(ModelVersion::new(1, 1, 0), VersionSource::HotSwap);

        // Previous should now be deactivated
        let prev = history.previous().unwrap();
        assert!(!prev.is_active());

        // Current should be active
        assert!(history.current().unwrap().is_active());
    }

    #[test]
    fn test_history_multiple_deactivations() {
        let mut history = VersionHistory::new();

        for i in 0..5 {
            history.record(ModelVersion::new(1, i, 0), VersionSource::Manual);
        }

        // All but last should be deactivated
        let entries: Vec<_> = history.all().iter().collect();
        for (idx, entry) in entries.iter().enumerate() {
            if idx < entries.len() - 1 {
                assert!(!entry.is_active(), "Entry {} should be deactivated", idx);
            } else {
                assert!(entry.is_active(), "Last entry should be active");
            }
        }
    }

    #[test]
    fn test_history_large_capacity() {
        let mut history = VersionHistory::with_capacity(1000);

        for i in 0..100 {
            history.record(ModelVersion::new(1, i, 0), VersionSource::Manual);
        }

        assert_eq!(history.len(), 100);
    }

    #[test]
    fn test_history_zero_capacity() {
        let mut history = VersionHistory::with_capacity(0);
        history.record(ModelVersion::new(1, 0, 0), VersionSource::Initial);
        assert_eq!(history.len(), 0); // All entries trimmed
    }
}
