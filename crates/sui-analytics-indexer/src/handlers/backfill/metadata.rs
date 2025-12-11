// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill support for re-processing checkpoints with matching file boundaries.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use object_store::ObjectStore;
use object_store::path::Path as ObjectPath;
use tracing::{debug, info};

use crate::config::{FileFormat, IndexerConfig};

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

/// Validates backfill config and loads targets if backfill mode is enabled.
///
/// Returns `Some((targets, adjusted_first_checkpoint))` in backfill mode,
/// or `None` if backfill mode is disabled.
///
/// Validation rules:
/// - `task_name` must be set for watermark isolation
/// - `last_checkpoint` must be set
/// - Exactly one pipeline must be configured
/// - If `first_checkpoint` is 0, it's auto-adjusted to the first file boundary
/// - If `first_checkpoint` > 0, it must align exactly with a file boundary
/// - `last_checkpoint` must align with a file end boundary
pub async fn load_backfill_metadata(
    config: &IndexerConfig,
    object_store: Arc<dyn ObjectStore>,
) -> Result<Option<(BackfillTargets, u64)>> {
    if !config.backfill_mode {
        return Ok(None);
    }

    if config.task_name.is_none() {
        return Err(anyhow!(
            "backfill_mode requires task_name to be set for watermark isolation"
        ));
    }
    if config.last_checkpoint.is_none() {
        return Err(anyhow!("backfill_mode requires last_checkpoint to be set"));
    }

    let pipeline_configs = config.pipeline_configs();
    if pipeline_configs.len() != 1 {
        return Err(anyhow!(
            "backfill_mode requires exactly one pipeline, found {}. \
             Different pipelines may have different file boundaries.",
            pipeline_configs.len()
        ));
    }

    let first = config.first_checkpoint.unwrap_or(0);
    let last = config.last_checkpoint.expect("validated above");
    let pipeline_config = &pipeline_configs[0];

    let (targets, adjusted) = load_targets(
        object_store,
        pipeline_config.pipeline.name(),
        pipeline_config.file_format,
        first,
        last,
    )
    .await?;

    info!(
        pipeline = %pipeline_config.pipeline,
        file_count = targets.len(),
        first_checkpoint = adjusted,
        "Loaded backfill targets"
    );

    Ok(Some((targets, adjusted)))
}

/// Load target files from the object store for the given checkpoint range.
async fn load_targets(
    object_store: Arc<dyn ObjectStore>,
    pipeline_name: &str,
    file_format: FileFormat,
    first_checkpoint: u64,
    last_checkpoint: u64,
) -> Result<(BackfillTargets, u64)> {
    let prefix = ObjectPath::from(pipeline_name);
    let mut targets = HashMap::new();
    let mut min_start: Option<u64> = None;
    let mut max_end: Option<u64> = None;
    let mut list_stream = object_store.list(Some(&prefix));

    while let Some(meta_result) = list_stream.next().await {
        let meta = meta_result.context("Failed to list files")?;

        if let Some((epoch, range)) = parse_file_path(&meta.location, file_format) {
            // Only include files that overlap with our checkpoint range
            // File range is [start, end), checkpoint range is [first, last]
            if range.end <= first_checkpoint || range.start > last_checkpoint {
                continue;
            }

            debug!(
                path = %meta.location,
                epoch,
                start = range.start,
                end = range.end,
                "Discovered backfill target"
            );

            min_start = Some(min_start.map_or(range.start, |m| m.min(range.start)));
            max_end = Some(max_end.map_or(range.end, |m| m.max(range.end)));

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
            "No existing files found at prefix '{}' for range {}..={}. \
             Backfill mode requires existing files to update.",
            pipeline_name,
            first_checkpoint,
            last_checkpoint
        ));
    }

    // Determine the actual first checkpoint to use
    let adjusted_first = if first_checkpoint == 0 {
        // Auto-adjust to first file boundary when starting from 0
        let first = min_start.expect("min_start set when targets is non-empty");
        if first != 0 {
            info!(
                adjusted = first,
                "Auto-adjusted first_checkpoint to first file boundary"
            );
        }
        first
    } else {
        // Explicit start checkpoint must align with a file boundary
        if !targets.contains_key(&first_checkpoint) {
            let mut available: Vec<_> = targets.keys().copied().collect();
            available.sort();
            return Err(anyhow!(
                "first_checkpoint {} does not align with a file boundary. \
                 Available file starts: {:?}",
                first_checkpoint,
                available
            ));
        }
        first_checkpoint
    };

    // Validate last_checkpoint aligns with a file end boundary
    // last_checkpoint is inclusive, so we need a file ending at last_checkpoint + 1
    let max_end = max_end.expect("max_end set when targets is non-empty");
    if max_end != last_checkpoint + 1 {
        return Err(anyhow!(
            "last_checkpoint {} does not align with a file end boundary. \
             last_checkpoint must be one less than a file's end checkpoint. \
             Max file end found: {}",
            last_checkpoint,
            max_end
        ));
    }

    info!(
        pipeline = %pipeline_name,
        file_count = targets.len(),
        first_checkpoint = adjusted_first,
        last_checkpoint,
        "Loaded backfill targets"
    );

    Ok((targets, adjusted_first))
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
