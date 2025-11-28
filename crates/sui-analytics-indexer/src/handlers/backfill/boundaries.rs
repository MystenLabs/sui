// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill support for re-processing checkpoints with matching file boundaries.
//!
//! When `backfill_mode` is enabled, the indexer:
//! 1. Loads all file boundaries upfront at initialization
//! 2. Forces batch boundaries to align with existing file ranges
//! 3. Uses conditional PUT operations to detect concurrent modifications
//!
//! Backfill mode requires `last_checkpoint` to be set, ensuring a bounded range.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::RwLock;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use object_store::ObjectStore;
use object_store::path::Path as ObjectPath;
use tracing::{debug, info};

use crate::config::FileFormat;

/// Cached e_tag/version for conditional updates.
#[derive(Debug, Clone, Default)]
struct ETagInfo {
    e_tag: Option<String>,
    version: Option<String>,
}

/// A single target file for backfill operations.
///
/// Contains the file metadata and e-tag for conditional updates.
/// The e-tag uses interior mutability to allow refresh on conflict.
pub struct TargetFile {
    pub epoch: u64,
    pub checkpoint_range: Range<u64>,
    pub path: ObjectPath,
    /// E-tag can be refreshed on conflict (interior mutability for retry)
    e_tag: RwLock<ETagInfo>,
}

impl TargetFile {
    /// Get the current e_tag and version for conditional update.
    pub fn get_etag(&self) -> (Option<String>, Option<String>) {
        let info = self.e_tag.read().unwrap();
        (info.e_tag.clone(), info.version.clone())
    }

    /// Refresh e_tag by re-fetching from object store.
    pub async fn refresh_etag(&self, object_store: &dyn ObjectStore) -> Result<()> {
        let meta = object_store
            .head(&self.path)
            .await
            .context("Failed to refresh e_tag")?;

        let mut cache = self.e_tag.write().unwrap();
        *cache = ETagInfo {
            e_tag: meta.e_tag,
            version: meta.version,
        };

        info!(
            epoch = self.epoch,
            start = self.checkpoint_range.start,
            path = %self.path,
            "Refreshed e_tag for backfill target"
        );

        Ok(())
    }
}

/// Pre-loaded map of all target files for backfill operations.
///
/// Created at initialization with all boundaries from start to end checkpoint.
/// Immutable after construction (except e-tags for conditional updates).
pub struct BackfillBoundaries {
    /// Object store for e-tag refresh operations
    object_store: std::sync::Arc<dyn ObjectStore>,
    /// All targets indexed by start_checkpoint
    targets: HashMap<u64, TargetFile>,
}

impl BackfillBoundaries {
    /// Load all target files from start to end checkpoint.
    ///
    /// Lists all files under the prefix, filters to those overlapping with
    /// the checkpoint range, and builds a complete map.
    ///
    /// Fails if no files are found for the checkpoint range.
    pub async fn load_all(
        object_store: std::sync::Arc<dyn ObjectStore>,
        dir_prefix: String,
        file_format: FileFormat,
        checkpoint_range: Range<u64>,
    ) -> Result<Self> {
        let prefix = ObjectPath::from(dir_prefix.as_str());

        let mut targets = HashMap::new();

        // Process the entire stream before moving object_store into Self
        {
            let mut list_stream = object_store.list(Some(&prefix));

            while let Some(meta_result) = list_stream.next().await {
                let meta = meta_result.context("Failed to list files")?;

                if let Some((epoch, range)) = parse_file_path(&meta.location, file_format) {
                    // Only include files that overlap with our checkpoint range
                    if range.end <= checkpoint_range.start || range.start >= checkpoint_range.end {
                        continue;
                    }

                    debug!(
                        path = %meta.location,
                        epoch,
                        start = range.start,
                        end = range.end,
                        "Discovered backfill target"
                    );

                    targets.insert(
                        range.start,
                        TargetFile {
                            epoch,
                            checkpoint_range: range,
                            path: meta.location,
                            e_tag: RwLock::new(ETagInfo {
                                e_tag: meta.e_tag,
                                version: meta.version,
                            }),
                        },
                    );
                }
            }
        }

        if targets.is_empty() {
            return Err(anyhow!(
                "No existing files found at prefix '{}' for range {:?}. \
                 Backfill mode requires existing files to update.",
                dir_prefix,
                checkpoint_range
            ));
        }

        info!(
            prefix = %dir_prefix,
            file_count = targets.len(),
            checkpoint_range = ?checkpoint_range,
            "Loaded all backfill targets"
        );

        Ok(Self {
            object_store,
            targets,
        })
    }

    /// Find the target file that contains a given checkpoint.
    pub fn find_target(&self, checkpoint: u64) -> Option<&TargetFile> {
        self.targets.values().find(|t| {
            t.checkpoint_range.start <= checkpoint && checkpoint < t.checkpoint_range.end
        })
    }

    /// Get a target by exact start_checkpoint key.
    pub fn get_target(&self, start_checkpoint: u64) -> Option<&TargetFile> {
        self.targets.get(&start_checkpoint)
    }

    /// Returns a reference to the object store.
    pub fn object_store(&self) -> &dyn ObjectStore {
        self.object_store.as_ref()
    }

    /// Returns the number of targets loaded.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns true if no targets are loaded.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
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
