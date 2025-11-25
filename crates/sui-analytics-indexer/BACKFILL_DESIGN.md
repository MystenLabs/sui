# Analytics Indexer: Deterministic Backfill Design

## Executive Summary

This document describes the design for deterministic backfills in the analytics indexer, solving the long-standing problem of non-reproducible checkpoint ranges when adding new columns to parquet files.

**Status:** Watermark refactor ✅ COMPLETE. Backfill implementation 🔄 READY TO BUILD.

---

## Background

### The Analytics Indexer Architecture

The analytics indexer processes Sui blockchain checkpoints and writes them to parquet files in object storage (S3/GCS). It's built on top of `sui-indexer-alt-framework`, which provides:

1. **Processor**: Converts checkpoint data into typed rows
2. **Collector**: Batches rows for efficient writing
3. **Committer**: Writes batches to storage concurrently
4. **Watermark Tracker**: Tracks progress

### File Naming Convention

Files are named by checkpoint range:
```
events/epoch_42/1000_2437.parquet
events/epoch_42/2437_3891.parquet
events/epoch_42/3891_5000.parquet
```

The ranges are determined by when batches fill up (`max_row_count`), which is non-deterministic across runs.

---

## Problem Statement

### The Checkpoint Gap Issue (SOLVED ✅)

**Original Problem:** When processors returned empty `Vec` for checkpoints with no data, the handler never saw those checkpoints, causing gaps in file ranges:

```
events/epoch_42/1000_2437.parquet  (checkpoints 1000-2437)
events/epoch_42/2450_3891.parquet  (checkpoints 2450-3891)
                  ↑
             Missing 2438-2449!
```

**Solution (COMPLETED):** We refactored the framework to pass `watermarks: &[WatermarkPart]` to `Handler::commit()`. Watermarks are guaranteed contiguous because the collector processes checkpoints in order from a `BTreeMap`. See commit history for full implementation.

### The Backfill Problem (TO BE SOLVED)

**Current Problem:** When adding a new column to the schema, we need to reprocess all historical data. But the batch boundaries are non-deterministic:

```bash
# Original run (schema v1):
events/epoch_42/1000_2437.parquet  # 1437 checkpoints
events/epoch_42/2437_3891.parquet  # 1454 checkpoints

# Backfill run (schema v2):
events/epoch_42/1000_2501.parquet  # 1501 checkpoints ❌ DIFFERENT!
events/epoch_42/2501_3799.parquet  # 1298 checkpoints ❌ DIFFERENT!
```

Now we have **overlapping files with different schemas** - impossible to query!

**What We Need:** Deterministic checkpoint ranges that are reproducible across runs, so backfills can overwrite exact same files.

---

## Solution Design

### Philosophy: The Filesystem IS the Manifest

Instead of maintaining separate manifest metadata, we use the object store listing as our source of truth. File paths encode all necessary information.

### Core Mechanism

**For backfills:**

1. **List existing files** to discover checkpoint ranges
2. **Parse filenames** to extract ranges (e.g., `1000_2437.parquet` → `1000..2437`)
3. **Force batch boundaries** to match existing ranges exactly
4. **Overwrite files** at the same paths

### How It Works With the Concurrent Pipeline

The framework's concurrent pipeline has these concurrency points:

```
CHECKPOINT INGESTION (in order)
         ↓
    PROCESSOR (FANOUT concurrent processing) ⚡
         ↓ (may arrive out of order)
    COLLECTOR (sequential, BTreeMap.first_entry()) 📊 ← ORDERED!
         ↓ (batches sent in order)
    COMMITTER (write_concurrency concurrent writes) ⚡
         ↓
    WATERMARK TRACKER (tracks completion)
```

**Key insight:** Even though processing and committing are concurrent, the **collector batches checkpoints in order**. This means `Handler.batch()` can reliably detect when to stop batching by checking checkpoint numbers.

### Implementation Strategy

#### 1. Add Backfill Mode to Analytics Handler

```rust
pub enum BatchingMode {
    /// Normal mode - batch freely up to max_row_count
    Normal,

    /// Backfill mode - match existing checkpoint ranges
    Backfill {
        target_ranges: Vec<(Range<u64>, String)>,  // (range, file_path)
        current_range_idx: usize,
    },
}

pub struct AnalyticsBatch<T> {
    inner: Mutex<Option<WriterVariant>>,
    pub(crate) dir_prefix: String,
    current_file_bytes: Mutex<Option<Bytes>>,

    // NEW: Track batching mode
    batching_mode: BatchingMode,

    _phantom: PhantomData<T>,
}
```

#### 2. Modify Batching Logic

```rust
fn batch(
    &self,
    batch: &mut Self::Batch,
    values: &mut std::vec::IntoIter<Self::Value>,
) -> BatchStatus {
    match &batch.batching_mode {
        BatchingMode::Normal => {
            // Existing logic - batch up to max_row_count
            // ...
        }

        BatchingMode::Backfill { target_ranges, current_range_idx } => {
            let target = &target_ranges[*current_range_idx];

            for value in values.by_ref() {
                let cp = value.get_checkpoint_sequence_number();

                // Check if we've reached the target boundary
                if cp >= target.0.end {
                    // Stop here - force batch commit
                    return BatchStatus::Ready;
                }

                batch.write_rows(std::iter::once(value), self.config.file_format)?;
            }

            BatchStatus::Pending
        }
    }
}
```

#### 3. Modify Commit Logic

```rust
async fn commit<'a>(
    &self,
    batch: &Self::Batch,
    watermarks: &[WatermarkPart],
    conn: &mut Connection<'a>,
) -> Result<usize> {
    batch.flush()?;

    let Some(file_bytes) = batch.current_file_bytes() else {
        return Ok(0);
    };

    let row_count = batch.row_count()?;

    // Determine file path based on mode
    let file_path = match &batch.batching_mode {
        BatchingMode::Normal => {
            // Use watermarks to determine range
            let checkpoint_range = WatermarkPart::checkpoint_range(watermarks)?;
            let epoch = watermarks.first().unwrap().watermark.epoch_hi_inclusive;

            crate::construct_file_path(
                &batch.dir_prefix,
                epoch,
                checkpoint_range,
                self.config.file_format,
            ).to_string_lossy().to_string()
        }

        BatchingMode::Backfill { target_ranges, current_range_idx } => {
            // Use EXACT path from target ranges
            let (_range, path) = &target_ranges[*current_range_idx];
            path.clone()
        }
    };

    // Write file (overwrites if exists)
    let object_store_path = object_store::path::Path::from(&file_path);
    conn.object_store().put(&object_store_path, file_bytes.into()).await?;

    // Move to next range in backfill mode
    if let BatchingMode::Backfill { current_range_idx, .. } = &mut batch.batching_mode {
        *current_range_idx += 1;
    }

    Ok(row_count)
}
```

#### 4. Add Helper to Discover Existing Ranges

```rust
/// List existing files in object store and extract checkpoint ranges
pub async fn discover_checkpoint_ranges(
    object_store: &ObjectStore,
    pipeline: Pipeline,
    epoch: u64,
) -> Result<Vec<(Range<u64>, String)>> {
    let prefix = format!("{}/epoch_{}/", pipeline.dir_prefix(), epoch);
    let list_result = object_store.list(Some(&object_store::path::Path::from(prefix))).await?;

    let mut ranges = vec![];
    for meta in list_result {
        let path = meta.location.to_string();

        // Parse filename: "events/epoch_42/1000_2437.parquet" → (1000, 2437)
        if let Some(range) = parse_checkpoint_range_from_path(&path) {
            ranges.push((range, path));
        }
    }

    // Sort by checkpoint start
    ranges.sort_by_key(|(range, _)| range.start);

    Ok(ranges)
}

fn parse_checkpoint_range_from_path(path: &str) -> Option<Range<u64>> {
    // Extract filename from path
    let filename = std::path::Path::new(path).file_stem()?.to_str()?;

    // Parse "1000_2437" → 1000..2437
    let parts: Vec<&str> = filename.split('_').collect();
    if parts.len() >= 2 {
        let start: u64 = parts[parts.len() - 2].parse().ok()?;
        let end: u64 = parts[parts.len() - 1].parse().ok()?;
        Some(start..end)
    } else {
        None
    }
}
```

#### 5. Backfill Workflow

```rust
pub async fn run_backfill(
    object_store: Arc<ObjectStore>,
    pipeline: Pipeline,
    epoch: u64,
    config: PipelineConfig,
) -> Result<()> {
    // 1. Discover existing checkpoint ranges
    let target_ranges = discover_checkpoint_ranges(&object_store, pipeline, epoch).await?;

    if target_ranges.is_empty() {
        bail!("No existing files found for {}/epoch_{}", pipeline.dir_prefix(), epoch);
    }

    println!("Found {} existing files to backfill", target_ranges.len());
    for (range, path) in &target_ranges {
        println!("  {} → {:?}", path, range);
    }

    // 2. Create handler in backfill mode
    let processor = match pipeline {
        Pipeline::Event => EventProcessor::new(package_cache),
        Pipeline::Checkpoint => CheckpointProcessor,
        // ... other pipelines
    };

    let handler = AnalyticsHandler::new_backfill(
        processor,
        config,
        target_ranges.clone(),
    );

    // 3. Determine checkpoint range to process
    let start_checkpoint = target_ranges.first().unwrap().0.start;
    let end_checkpoint = target_ranges.last().unwrap().0.end;

    println!("Backfilling checkpoints {}..{}", start_checkpoint, end_checkpoint);

    // 4. Run indexer for this range
    // (Implementation depends on how you start the indexer with custom range)
    indexer::run_with_range(handler, start_checkpoint, end_checkpoint).await?;

    println!("Backfill complete!");
    Ok(())
}
```

---

## Optional Enhancements

### Schema Versioning (Simple)

Add a single metadata file per epoch to track schema version:

```json
// events/epoch_42/_metadata.json
{
  "schema_version": 2,
  "updated_at": "2025-11-22T00:00:00Z",
  "row_count": 1234567,
  "file_count": 42
}
```

Update this file when backfill completes.

### File Compaction

To merge small files into larger ones:

```rust
async fn compact_files(
    store: &ObjectStore,
    pipeline: Pipeline,
    epoch: u64,
    ranges_to_merge: Vec<Range<u64>>,
) -> Result<()> {
    // 1. Read all files in ranges
    let mut all_rows = vec![];
    for range in &ranges_to_merge {
        let path = construct_file_path(pipeline, epoch, range.clone(), FileFormat::Parquet);
        let data = read_parquet_file(store, &path).await?;
        all_rows.extend(data);
    }

    // 2. Write merged file
    let merged_range = ranges_to_merge.first().unwrap().start
        ..ranges_to_merge.last().unwrap().end;
    let merged_path = construct_file_path(pipeline, epoch, merged_range, FileFormat::Parquet);
    write_parquet_file(store, &merged_path, &all_rows).await?;

    // 3. Delete old files
    for range in &ranges_to_merge {
        let path = construct_file_path(pipeline, epoch, range.clone(), FileFormat::Parquet);
        store.delete(&path.into()).await?;
    }

    Ok(())
}
```

After compaction, future backfills will automatically discover and match the new merged ranges.

---

## Implementation Checklist

### Phase 1: Core Backfill Support ✅ READY TO BUILD

- [ ] Add `BatchingMode` enum to `AnalyticsBatch`
- [ ] Modify `batch()` to respect backfill target ranges
- [ ] Modify `commit()` to use target paths in backfill mode
- [ ] Add `discover_checkpoint_ranges()` helper
- [ ] Add `parse_checkpoint_range_from_path()` helper
- [ ] Add `AnalyticsHandler::new_backfill()` constructor
- [ ] Write tests for backfill mode
- [ ] Document backfill CLI usage

### Phase 2: Tooling (Optional)

- [ ] CLI command: `sui-analytics-indexer backfill --pipeline events --epoch 42`
- [ ] CLI command: `sui-analytics-indexer verify-ranges --pipeline events --epoch 42`
- [ ] CLI command: `sui-analytics-indexer compact --pipeline events --epoch 42`
- [ ] Add schema version tracking to metadata file

### Phase 3: Operations (Optional)

- [ ] Monitoring dashboard for backfill progress
- [ ] Alerting for gaps in checkpoint ranges
- [ ] Automated backfill orchestration
- [ ] Compaction scheduler

---

## Testing Strategy

### Unit Tests

1. Test `parse_checkpoint_range_from_path()` with various filename formats
2. Test `discover_checkpoint_ranges()` with mock object store
3. Test `batch()` boundary detection in backfill mode
4. Test `commit()` path selection in normal vs backfill mode

### Integration Tests

1. Write files with normal mode, verify ranges are contiguous
2. List files, parse ranges, verify they match written files
3. Run backfill with discovered ranges, verify exact overwrites
4. Add new column to schema, backfill, verify new schema in files

### Edge Cases

1. Empty checkpoints at batch boundaries
2. Checkpoint with huge number of rows (exceeds max_row_count)
3. Concurrent backfills of same epoch
4. Missing files in sequence (gaps)
5. Corrupted filenames

---

## Migration Path

### For Existing Deployments

1. **No migration needed!** Existing files continue to work
2. Future writes will use the backfill-compatible approach
3. Run initial backfill to establish baseline ranges
4. Future schema changes use deterministic backfills

### For New Deployments

1. Start with backfill mode enabled from day 1
2. Set reasonable `max_row_count` for typical file sizes
3. Monitor file sizes and adjust if needed

---

## Performance Considerations

### Object Store Listing

- Listing files in object store is fast for reasonable file counts (< 10k files/epoch)
- For very large epochs, consider caching discovered ranges
- Use prefix filtering to only list relevant epoch

### Backfill Throughput

- Backfill throughput same as normal indexing (both use same pipeline)
- Concurrency controlled by `FANOUT` and `write_concurrency`
- Can run multiple epoch backfills in parallel (different epochs = different files)

### Storage Costs

- Overwriting files doesn't double storage (old version deleted)
- Object stores typically have versioning - may need lifecycle policies
- Compaction reduces file count, may improve query performance

---

## Open Questions / Future Work

1. Should we support in-place schema evolution (add column without rewrite)?
2. Do we need rollback support if backfill fails halfway?
3. Should we maintain a separate changelog of schema versions?
4. How to handle breaking schema changes (column removal/rename)?

---

## References

- Watermark refactor PR: [Link to PR once created]
- Object store trait: `crates/sui-indexer-alt-object-store/src/lib.rs`
- Analytics handler: `crates/sui-analytics-indexer/src/handlers/analytics_handler.rs`
- Framework collector: `crates/sui-indexer-alt-framework/src/pipeline/concurrent/collector.rs`

---

## Contact

For questions or clarifications, reach out to @nickv or check the analytics indexer channel.
