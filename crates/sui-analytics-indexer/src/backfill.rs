// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill support for re-processing checkpoints with matching file boundaries.
//!
//! When `backfill_mode` is enabled, the indexer:
//! 1. Lazily loads file boundaries per-epoch on first access
//! 2. Forces batch boundaries to align with existing file ranges
//! 3. Uses conditional PUT operations to detect concurrent modifications
//! 4. Prunes old epochs from memory after processing moves forward

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, anyhow};
use dashmap::DashMap;
use futures::StreamExt;
use object_store::ObjectStore;
use object_store::path::Path as ObjectPath;
use tracing::{debug, info};

use crate::config::FileFormat;

/// Target file metadata for backfill operations.
#[derive(Debug, Clone)]
pub struct TargetRange {
    pub epoch: u64,
    pub checkpoint_range: Range<u64>,
    pub path: ObjectPath,
}

/// Cached e_tag/version for conditional updates.
#[derive(Debug, Clone, Default)]
struct ETagInfo {
    e_tag: Option<String>,
    version: Option<String>,
}

/// Boundaries for a single epoch, loaded on-demand.
///
/// Each batch holds an `Arc<EpochBoundaries>` to keep the data alive
/// while the batch is in-flight, even after the epoch is pruned from cache.
pub struct EpochBoundaries {
    epoch: u64,
    /// Targets indexed by start_checkpoint for this epoch
    targets: HashMap<u64, TargetRange>,
    /// E-tags can be refreshed on conflict (interior mutability for retry)
    e_tags: RwLock<HashMap<u64, ETagInfo>>,
}

impl EpochBoundaries {
    /// Find the target that contains a given checkpoint in this epoch.
    pub fn find_target(&self, checkpoint: u64) -> Option<&TargetRange> {
        self.targets
            .values()
            .find(|t| t.checkpoint_range.start <= checkpoint && checkpoint < t.checkpoint_range.end)
    }

    /// Get a target by exact start_checkpoint key.
    pub fn get(&self, start: u64) -> Option<&TargetRange> {
        self.targets.get(&start)
    }

    /// Get the current e_tag and version for a target.
    pub fn get_etag(&self, start: u64) -> (Option<String>, Option<String>) {
        let cache = self.e_tags.read().unwrap();
        if let Some(info) = cache.get(&start) {
            (info.e_tag.clone(), info.version.clone())
        } else {
            (None, None)
        }
    }

    /// Refresh e_tag for a target by re-fetching from object store.
    pub async fn refresh_etag(&self, object_store: &dyn ObjectStore, start: u64) -> Result<()> {
        let target = self
            .get(start)
            .ok_or_else(|| anyhow!("No target for epoch {} start {}", self.epoch, start))?;

        let meta = object_store
            .head(&target.path)
            .await
            .context("Failed to refresh e_tag")?;

        let mut cache = self.e_tags.write().unwrap();
        cache.insert(
            start,
            ETagInfo {
                e_tag: meta.e_tag,
                version: meta.version,
            },
        );

        info!(
            epoch = self.epoch,
            start,
            path = %target.path,
            "Refreshed e_tag for backfill target"
        );

        Ok(())
    }

    /// Returns the number of targets in this epoch.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns true if there are no targets.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

/// Lazy-loading cache for backfill boundaries.
///
/// Loads epoch data on-demand and caches it in a DashMap.
/// Each batch clones an `Arc<EpochBoundaries>` to keep data alive.
/// Old epochs can be pruned when processing moves forward.
pub struct BackfillBoundaries {
    /// Object store for loading boundaries
    object_store: Arc<dyn ObjectStore>,
    /// Base prefix for this pipeline
    dir_prefix: String,
    /// File format for parsing
    file_format: FileFormat,
    /// Per-epoch cache (loaded on demand)
    epochs: DashMap<u64, Arc<EpochBoundaries>>,
}

impl BackfillBoundaries {
    /// Create a new lazy-loading BackfillBoundaries.
    ///
    /// Validates that the prefix exists but does not list all files upfront.
    pub async fn new(
        object_store: Arc<dyn ObjectStore>,
        dir_prefix: String,
        file_format: FileFormat,
    ) -> Result<Self> {
        // Validate prefix exists with a lightweight check
        {
            let prefix = ObjectPath::from(dir_prefix.as_str());
            let mut list_stream = object_store.list(Some(&prefix));

            // Check if at least one file exists
            let first = list_stream.next().await;
            if first.is_none() {
                return Err(anyhow!(
                    "No existing files found at prefix '{}'. \
                     Backfill mode requires existing files to update.",
                    dir_prefix
                ));
            }
        }

        info!(
            prefix = %dir_prefix,
            "Initialized lazy backfill boundaries"
        );

        Ok(Self {
            object_store,
            dir_prefix,
            file_format,
            epochs: DashMap::new(),
        })
    }

    /// Load boundaries for a specific epoch from object store.
    async fn load_epoch(&self, epoch: u64) -> Result<Arc<EpochBoundaries>> {
        let epoch_prefix = ObjectPath::from(format!("{}/epoch_{}/", self.dir_prefix, epoch));
        let mut list_stream = self.object_store.list(Some(&epoch_prefix));

        let mut targets = HashMap::new();
        let mut e_tags = HashMap::new();

        while let Some(meta_result) = list_stream.next().await {
            let meta = meta_result.context("Failed to list epoch files")?;

            if let Some((parsed_epoch, range)) = parse_file_path(&meta.location, self.file_format) {
                // Sanity check: epoch should match
                if parsed_epoch != epoch {
                    continue;
                }

                debug!(
                    path = %meta.location,
                    epoch,
                    start = range.start,
                    end = range.end,
                    "Discovered backfill target"
                );

                let start = range.start;
                targets.insert(
                    start,
                    TargetRange {
                        epoch,
                        checkpoint_range: range,
                        path: meta.location,
                    },
                );
                e_tags.insert(
                    start,
                    ETagInfo {
                        e_tag: meta.e_tag,
                        version: meta.version,
                    },
                );
            }
        }

        info!(
            epoch,
            count = targets.len(),
            "Loaded backfill boundaries for epoch"
        );

        Ok(Arc::new(EpochBoundaries {
            epoch,
            targets,
            e_tags: RwLock::new(e_tags),
        }))
    }

    /// Ensure an epoch is loaded and return an Arc to its boundaries.
    ///
    /// This is idempotent - if the epoch is already loaded, returns the cached Arc.
    /// The returned Arc keeps the epoch data alive even if pruned from cache.
    pub async fn ensure_epoch_loaded(&self, epoch: u64) -> Result<Arc<EpochBoundaries>> {
        // Check cache first
        if let Some(entry) = self.epochs.get(&epoch) {
            return Ok(entry.clone());
        }

        // Not in cache - load from object store
        let epoch_data = self.load_epoch(epoch).await?;

        // Insert into cache (handles race condition - another thread may have loaded it)
        self.epochs.entry(epoch).or_insert(epoch_data.clone());

        // Return the version in the cache (might be from another thread)
        Ok(self.epochs.get(&epoch).unwrap().clone())
    }

    /// Get an Arc to an epoch's boundaries.
    ///
    /// Panics if the epoch is not loaded. Always call `ensure_epoch_loaded()` first.
    pub fn get_epoch(&self, epoch: u64) -> Option<Arc<EpochBoundaries>> {
        self.epochs.get(&epoch).map(|entry| entry.clone())
    }

    /// Prune epochs older than the given epoch from cache.
    ///
    /// In-flight batches that hold Arcs to pruned epochs will keep the data alive.
    /// Once all batches for an epoch are complete, the memory is freed.
    pub fn prune_epochs_before(&self, epoch: u64) {
        let before_count = self.epochs.len();
        self.epochs.retain(|&e, _| e >= epoch);
        let after_count = self.epochs.len();

        if before_count != after_count {
            info!(
                pruned = before_count - after_count,
                remaining = after_count,
                current_epoch = epoch,
                "Pruned old epochs from backfill cache"
            );
        }
    }

    /// Returns a reference to the object store.
    pub fn object_store(&self) -> &dyn ObjectStore {
        self.object_store.as_ref()
    }
}

/// Parse a file path to extract epoch and checkpoint range.
///
/// Expected format: `{prefix}/epoch_{N}/{start}_{end}.{csv|parquet}`
///
/// Returns `None` if the path doesn't match the expected format.
fn parse_file_path(path: &ObjectPath, file_format: FileFormat) -> Option<(u64, Range<u64>)> {
    let path_str = path.as_ref();

    let extension = match file_format {
        FileFormat::Csv => ".csv",
        FileFormat::Parquet => ".parquet",
    };

    // Check extension
    if !path_str.ends_with(extension) {
        return None;
    }

    // Split path into parts
    let parts: Vec<&str> = path_str.split('/').collect();
    if parts.len() < 2 {
        return None;
    }

    // Extract filename (last part): "100_200.parquet"
    let filename = parts.last()?;
    let name = filename.strip_suffix(extension)?;

    // Parse checkpoint range from filename
    let mut range_parts = name.split('_');
    let start: u64 = range_parts.next()?.parse().ok()?;
    let end: u64 = range_parts.next()?.parse().ok()?;

    // Ensure no extra parts in filename
    if range_parts.next().is_some() {
        return None;
    }

    // Extract epoch from parent directory: "epoch_5"
    let epoch_dir = parts.get(parts.len() - 2)?;
    let epoch: u64 = epoch_dir.strip_prefix("epoch_")?.parse().ok()?;

    Some((epoch, start..end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_path_parquet() {
        let path = ObjectPath::from("transactions/epoch_5/1000_2000.parquet");
        let result = parse_file_path(&path, FileFormat::Parquet);
        assert_eq!(result, Some((5, 1000..2000)));
    }

    #[test]
    fn test_parse_file_path_csv() {
        let path = ObjectPath::from("events/epoch_10/500_600.csv");
        let result = parse_file_path(&path, FileFormat::Csv);
        assert_eq!(result, Some((10, 500..600)));
    }

    #[test]
    fn test_parse_file_path_wrong_extension() {
        let path = ObjectPath::from("transactions/epoch_5/1000_2000.csv");
        let result = parse_file_path(&path, FileFormat::Parquet);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_file_path_invalid_format() {
        // Missing epoch prefix
        let path = ObjectPath::from("transactions/5/1000_2000.parquet");
        assert_eq!(parse_file_path(&path, FileFormat::Parquet), None);

        // Invalid checkpoint format
        let path = ObjectPath::from("transactions/epoch_5/invalid.parquet");
        assert_eq!(parse_file_path(&path, FileFormat::Parquet), None);

        // Extra parts in filename
        let path = ObjectPath::from("transactions/epoch_5/1000_2000_extra.parquet");
        assert_eq!(parse_file_path(&path, FileFormat::Parquet), None);
    }

    #[test]
    fn test_parse_file_path_nested_prefix() {
        let path = ObjectPath::from("analytics/prod/transactions/epoch_5/1000_2000.parquet");
        let result = parse_file_path(&path, FileFormat::Parquet);
        assert_eq!(result, Some((5, 1000..2000)));
    }
}
