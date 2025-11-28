// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill support for re-processing checkpoints with matching file boundaries.

use std::collections::HashMap;
use std::ops::Range;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use object_store::ObjectStore;
use object_store::path::Path as ObjectPath;
use tracing::{debug, info};

use crate::config::FileFormat;

/// A single target file for backfill operations.
pub struct TargetFile {
    pub epoch: u64,
    pub checkpoint_range: Range<u64>,
    pub path: ObjectPath,
    pub e_tag: Option<String>,
    pub version: Option<String>,
}

/// Map of target files keyed by start checkpoint.
pub type BackfillTargets = HashMap<u64, TargetFile>;

/// Load all target files from start to end checkpoint.
///
/// Lists all files under the prefix, filters to those overlapping with
/// the checkpoint range, and builds a map keyed by start checkpoint.
///
/// Fails if no files are found for the checkpoint range.
pub async fn load_backfill_targets(
    object_store: std::sync::Arc<dyn ObjectStore>,
    dir_prefix: &str,
    file_format: FileFormat,
    checkpoint_range: Range<u64>,
) -> Result<BackfillTargets> {
    let prefix = ObjectPath::from(dir_prefix);
    let mut targets = HashMap::new();
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
                    e_tag: meta.e_tag,
                    version: meta.version,
                },
            );
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

    Ok(targets)
}

/// Parse a file path to extract epoch and checkpoint range.
///
/// Expected format: `{prefix}/epoch_{N}/{start}_{end}.{csv|parquet}`
fn parse_file_path(path: &ObjectPath, file_format: FileFormat) -> Option<(u64, Range<u64>)> {
    let path_str = path.as_ref();

    let extension = match file_format {
        FileFormat::Csv => ".csv",
        FileFormat::Parquet => ".parquet",
    };

    if !path_str.ends_with(extension) {
        return None;
    }

    let parts: Vec<&str> = path_str.split('/').collect();
    if parts.len() < 2 {
        return None;
    }

    let filename = parts.last()?;
    let name = filename.strip_suffix(extension)?;

    let mut range_parts = name.split('_');
    let start: u64 = range_parts.next()?.parse().ok()?;
    let end: u64 = range_parts.next()?.parse().ok()?;

    if range_parts.next().is_some() {
        return None;
    }

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
        let path = ObjectPath::from("transactions/5/1000_2000.parquet");
        assert_eq!(parse_file_path(&path, FileFormat::Parquet), None);

        let path = ObjectPath::from("transactions/epoch_5/invalid.parquet");
        assert_eq!(parse_file_path(&path, FileFormat::Parquet), None);

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
