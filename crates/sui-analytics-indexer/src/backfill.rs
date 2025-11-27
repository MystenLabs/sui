// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill support for re-processing checkpoints with matching file boundaries.
//!
//! When `backfill_mode` is enabled, the indexer:
//! 1. Lists existing files to discover their checkpoint ranges
//! 2. Forces batch boundaries to align with existing file ranges
//! 3. Uses conditional PUT operations to detect concurrent modifications

use std::collections::HashMap;
use std::ops::Range;
use std::sync::RwLock;

use anyhow::{Context, Result, anyhow};
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

/// Index of existing files discovered from object store.
///
/// Used in backfill mode to:
/// - Force batch boundaries to match existing file ranges
/// - Enable conditional PUT operations for conflict detection
pub struct BackfillBoundaries {
    /// Targets indexed by (epoch, start_checkpoint) for efficient lookup
    boundaries: HashMap<(u64, u64), TargetRange>,
    /// E-tags can be refreshed on conflict (interior mutability for retry)
    e_tag_cache: RwLock<HashMap<(u64, u64), ETagInfo>>,
}

impl BackfillBoundaries {
    /// Discover all existing files for a pipeline's output prefix.
    ///
    /// Lists the object store and parses filenames to build an index of
    /// existing checkpoint ranges that backfill must match.
    pub async fn discover(
        object_store: &dyn ObjectStore,
        dir_prefix: &str,
        file_format: FileFormat,
    ) -> Result<Self> {
        let prefix = ObjectPath::from(dir_prefix);
        let mut list_stream = object_store.list(Some(&prefix));

        let mut boundaries = HashMap::new();
        let mut e_tag_cache = HashMap::new();

        while let Some(meta_result) = list_stream.next().await {
            let meta = meta_result.context("Failed to list object store")?;

            if let Some((epoch, range)) = parse_file_path(&meta.location, file_format) {
                let key = (epoch, range.start);

                debug!(
                    path = %meta.location,
                    epoch,
                    start = range.start,
                    end = range.end,
                    "Discovered backfill target"
                );

                boundaries.insert(
                    key,
                    TargetRange {
                        epoch,
                        checkpoint_range: range,
                        path: meta.location,
                    },
                );

                e_tag_cache.insert(
                    key,
                    ETagInfo {
                        e_tag: meta.e_tag,
                        version: meta.version,
                    },
                );
            }
        }

        info!(
            prefix = dir_prefix,
            count = boundaries.len(),
            "Discovered backfill boundaries"
        );

        Ok(Self {
            boundaries,
            e_tag_cache: RwLock::new(e_tag_cache),
        })
    }

    /// Returns the number of target files.
    pub fn len(&self) -> usize {
        self.boundaries.len()
    }

    /// Returns true if there are no boundaries.
    pub fn is_empty(&self) -> bool {
        self.boundaries.is_empty()
    }

    /// Returns the total checkpoint range covered by all boundaries.
    pub fn total_range(&self) -> Option<Range<u64>> {
        let min = self
            .boundaries
            .values()
            .map(|t| t.checkpoint_range.start)
            .min()?;
        let max = self
            .boundaries
            .values()
            .map(|t| t.checkpoint_range.end)
            .max()?;
        Some(min..max)
    }

    /// Find the target that contains a given checkpoint in a specific epoch.
    ///
    /// Used during batching to determine when to trigger a commit.
    pub fn find_target(&self, epoch: u64, checkpoint: u64) -> Option<&TargetRange> {
        // Find the target whose range contains this checkpoint
        // Since boundaries are keyed by (epoch, start), we need to search
        self.boundaries.values().find(|t| {
            t.epoch == epoch
                && t.checkpoint_range.start <= checkpoint
                && checkpoint < t.checkpoint_range.end
        })
    }

    /// Get a target by exact (epoch, start_checkpoint) key.
    ///
    /// Used during commit to validate range alignment.
    pub fn get(&self, epoch: u64, start: u64) -> Option<&TargetRange> {
        self.boundaries.get(&(epoch, start))
    }

    /// Get the current e_tag and version for a target.
    ///
    /// Returns values that may have been refreshed by a previous retry.
    pub fn get_etag(&self, epoch: u64, start: u64) -> (Option<String>, Option<String>) {
        let cache = self.e_tag_cache.read().unwrap();
        if let Some(info) = cache.get(&(epoch, start)) {
            (info.e_tag.clone(), info.version.clone())
        } else {
            (None, None)
        }
    }

    /// Refresh e_tag for a target by re-fetching from object store.
    ///
    /// Called when a conditional PUT fails due to e_tag mismatch.
    /// The next retry will use the updated e_tag.
    pub async fn refresh_etag(
        &self,
        object_store: &dyn ObjectStore,
        epoch: u64,
        start: u64,
    ) -> Result<()> {
        let target = self
            .get(epoch, start)
            .ok_or_else(|| anyhow!("No target for epoch {} start {}", epoch, start))?;

        let meta = object_store
            .head(&target.path)
            .await
            .context("Failed to refresh e_tag")?;

        let mut cache = self.e_tag_cache.write().unwrap();
        cache.insert(
            (epoch, start),
            ETagInfo {
                e_tag: meta.e_tag,
                version: meta.version,
            },
        );

        info!(
            epoch,
            start,
            path = %target.path,
            "Refreshed e_tag for backfill target"
        );

        Ok(())
    }

    /// Returns an iterator over all boundaries.
    pub fn iter(&self) -> impl Iterator<Item = &TargetRange> {
        self.boundaries.values()
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
