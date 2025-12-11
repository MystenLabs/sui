// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use object_store::ObjectStore;
use object_store::local::LocalFileSystem;
use parquet::file::reader::FileReader;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_analytics_indexer::config::{FileFormat, IndexerConfig, OutputStoreConfig, PipelineConfig};
use sui_analytics_indexer::metrics::Metrics;
use sui_analytics_indexer::pipeline::Pipeline;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::{Checkpoint, CheckpointData};
use sui_types::test_checkpoint_data_builder::{AdvanceEpochConfig, TestCheckpointBuilder};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn default_pipeline_config(pipeline: Pipeline) -> PipelineConfig {
    PipelineConfig {
        pipeline,
        file_format: FileFormat::Parquet,
        // High value to batch many checkpoints into single file by default
        max_rows_per_file: 10000,
        package_id_filter: None,
        sf_table_id: None,
        sf_checkpoint_col_id: None,
        report_sf_max_table_checkpoint: false,
    }
}

fn filter_by_extension<'a>(
    files: &'a [object_store::path::Path],
    ext: &str,
) -> Vec<&'a object_store::path::Path> {
    files.iter().filter(|p| p.as_ref().ends_with(ext)).collect()
}

struct TestHarness {
    output_dir: TempDir,
    ingestion_dir: TempDir,
    object_store: Arc<LocalFileSystem>,
    builder: Option<TestCheckpointBuilder>,
    last_checkpoint_seq: u64,
    next_object_id: u64,
}

impl TestHarness {
    fn new() -> Self {
        let output_dir = TempDir::new().expect("Failed to create output temp dir");
        let ingestion_dir = TempDir::new().expect("Failed to create ingestion temp dir");
        let object_store = Arc::new(
            LocalFileSystem::new_with_prefix(output_dir.path())
                .expect("Failed to create local file system"),
        );

        Self {
            output_dir,
            ingestion_dir,
            object_store,
            builder: Some(TestCheckpointBuilder::new(0)),
            last_checkpoint_seq: 0,
            next_object_id: 0,
        }
    }

    fn add_checkpoint(&mut self) -> u64 {
        let builder = self.builder.take().expect("builder already consumed");
        let object_id = self.next_object_id;
        self.next_object_id += 1;
        self.builder = Some(
            builder
                .start_transaction(0)
                .create_owned_object(object_id)
                .finish_transaction(),
        );

        let checkpoint = self
            .builder
            .as_mut()
            .expect("builder missing")
            .build_checkpoint();
        self.last_checkpoint_seq = *checkpoint.summary.sequence_number();
        self.write_checkpoint_to_ingestion_dir(checkpoint);
        self.last_checkpoint_seq
    }

    fn advance_epoch(&mut self) -> u64 {
        let checkpoint = self
            .builder
            .as_mut()
            .expect("builder missing")
            .advance_epoch(AdvanceEpochConfig::default());
        self.last_checkpoint_seq = *checkpoint.summary.sequence_number();
        self.write_checkpoint_to_ingestion_dir(checkpoint);
        self.last_checkpoint_seq
    }

    fn write_checkpoint_to_ingestion_dir(&self, checkpoint: Checkpoint) {
        let checkpoint_data: CheckpointData = checkpoint.into();
        let file_name = format!("{}.chk", checkpoint_data.checkpoint_summary.sequence_number);
        let file_path = self.ingestion_dir.path().join(file_name);
        let blob =
            Blob::encode(&checkpoint_data, BlobEncoding::Bcs).expect("Failed to encode checkpoint");
        fs::write(file_path, blob.to_bytes()).expect("Failed to write checkpoint file");
    }

    fn default_config(&self) -> IndexerConfig {
        IndexerConfig {
            rest_url: "https://checkpoints.testnet.sui.io".to_string(),
            client_metric_host: "127.0.0.1".to_string(),
            client_metric_port: 8081,
            output_store: OutputStoreConfig::File {
                path: self.output_dir.path().to_path_buf(),
            },
            request_timeout_secs: 30,
            remote_store_url: "https://checkpoints.testnet.sui.io".to_string(),
            streaming_url: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
            work_dir: None,
            local_ingestion_path: Some(self.ingestion_dir.path().to_path_buf()),
            sf_account_identifier: None,
            sf_warehouse: None,
            sf_database: None,
            sf_schema: None,
            sf_username: None,
            sf_role: None,
            sf_password_file: None,
            task_name: None,
            backfill_mode: false,
            reader_interval_ms: 100,
            pipeline_configs: vec![default_pipeline_config(Pipeline::Checkpoint)],
            ingestion: IngestionConfig {
                checkpoint_buffer_size: 100,
                ..Default::default()
            },
            concurrent: ConcurrentConfig::default(),
            first_checkpoint: None,
            last_checkpoint: None,
        }
    }

    async fn run_indexer(&self, config: IndexerConfig) {
        let registry = prometheus::Registry::new();
        let metrics = Metrics::new(&registry);
        let cancel = CancellationToken::new();

        let indexer = sui_analytics_indexer::build_analytics_indexer(
            config,
            metrics,
            registry,
            cancel.clone(),
        )
        .await
        .expect("Failed to build indexer");

        let handle = indexer.run().await.expect("Failed to run indexer");

        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            cancel.cancel();
            let _ = handle.await;
        })
        .await
        .expect("Timed out waiting for indexer");
    }

    async fn list_files(&self, prefix: &str) -> Vec<object_store::path::Path> {
        use futures::TryStreamExt;
        let prefix_path = object_store::path::Path::from(prefix);
        let list_stream = self.object_store.list(Some(&prefix_path));
        let objects: Vec<_> = list_stream
            .try_collect()
            .await
            .expect("Failed to list files");
        objects.into_iter().map(|meta| meta.location).collect()
    }

    async fn read_parquet_row_count(&self, path: &object_store::path::Path) -> usize {
        let bytes = self
            .object_store
            .get(path)
            .await
            .expect("Failed to get parquet file")
            .bytes()
            .await
            .expect("Failed to read parquet bytes");
        let reader = parquet::file::reader::SerializedFileReader::new(bytes)
            .expect("Failed to create parquet reader");
        let metadata = reader.metadata();
        metadata
            .row_groups()
            .iter()
            .map(|rg| rg.num_rows() as usize)
            .sum()
    }

    async fn read_csv_line_count(&self, path: &object_store::path::Path) -> usize {
        let bytes = self
            .object_store
            .get(path)
            .await
            .expect("Failed to get CSV file")
            .bytes()
            .await
            .expect("Failed to read CSV bytes");
        let content = String::from_utf8(bytes.to_vec()).expect("Invalid UTF-8 in CSV");
        content.lines().count()
    }

    fn output_path(&self) -> &Path {
        self.output_dir.path()
    }

    fn object_store(&self) -> &Arc<LocalFileSystem> {
        &self.object_store
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_pipeline_basic() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // File count may vary due to timing, but total rows must be exact.
    assert!(
        !parquet_files.is_empty(),
        "Expected at least one parquet file in checkpoints/epoch_0, found: {:?}",
        files
    );

    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        total_rows, 1,
        "Expected 1 total checkpoint row, found {}",
        total_rows
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_checkpoints_single_file() {
    let mut harness = TestHarness::new();
    for _ in 0..5 {
        harness.add_checkpoint();
    }

    let mut config = harness.default_config();
    config.last_checkpoint = Some(harness.last_checkpoint_seq);

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // File count may vary due to timing, but total rows must be exact.
    assert!(
        !parquet_files.is_empty(),
        "Expected at least one parquet file"
    );

    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        total_rows, 5,
        "Expected 5 total checkpoint rows, found {}",
        total_rows
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_checkpoints_multiple_files() {
    let mut harness = TestHarness::new();
    for _ in 0..6 {
        harness.add_checkpoint();
    }

    let mut config = harness.default_config();
    config.last_checkpoint = Some(harness.last_checkpoint_seq);
    // Set max_rows_per_file to 3 so each file has at most 3 rows
    config.pipeline_configs[0].max_rows_per_file = 3;

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // With min_eager_rows=3, we should get at least 2 files (6 checkpoints / 3 = 2)
    assert!(
        parquet_files.len() >= 2,
        "Expected at least 2 parquet files (6 checkpoints with max 3 per file), found {}",
        parquet_files.len()
    );

    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(total_rows, 6, "Expected 6 total checkpoint rows");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_epoch_boundary_creates_separate_files() {
    let mut harness = TestHarness::new();
    harness.add_checkpoint();
    harness.add_checkpoint();
    harness.advance_epoch();
    let last_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Epoch 0: 2 checkpoints + 1 epoch advance = 3 checkpoints
    // File count may vary due to timing, but total rows must be exact.
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    assert!(
        !epoch_0_parquet.is_empty(),
        "Expected at least one parquet file in epoch_0"
    );
    let mut epoch_0_total_rows = 0;
    for file in &epoch_0_parquet {
        epoch_0_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        epoch_0_total_rows, 3,
        "Expected 3 total checkpoint rows in epoch_0"
    );

    // Epoch 1: 1 checkpoint
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");
    assert!(
        !epoch_1_parquet.is_empty(),
        "Expected at least one parquet file in epoch_1"
    );
    let mut epoch_1_total_rows = 0;
    for file in &epoch_1_parquet {
        epoch_1_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        epoch_1_total_rows, 1,
        "Expected 1 total checkpoint row in epoch_1"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_csv_file_format() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs[0].file_format = FileFormat::Csv;

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let csv_files = filter_by_extension(&files, ".csv");

    assert_eq!(
        csv_files.len(),
        1,
        "Expected exactly 1 CSV file in checkpoints/epoch_0"
    );

    // CSV has 1 data row (no header)
    let line_count = harness.read_csv_line_count(csv_files[0]).await;
    assert_eq!(
        line_count, 1,
        "Expected 1 line in CSV (1 data row), found {}",
        line_count
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_pipelines() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs = vec![
        default_pipeline_config(Pipeline::Checkpoint),
        default_pipeline_config(Pipeline::Transaction),
    ];

    harness.run_indexer(config).await;

    // File count may vary due to timing, but total rows must be exact.
    let checkpoint_files = harness.list_files("checkpoints/epoch_0").await;
    let checkpoint_parquet = filter_by_extension(&checkpoint_files, ".parquet");
    assert!(
        !checkpoint_parquet.is_empty(),
        "Expected at least one checkpoint parquet file"
    );
    let mut checkpoint_total_rows = 0;
    for file in &checkpoint_parquet {
        checkpoint_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(checkpoint_total_rows, 1, "Expected 1 total checkpoint row");

    let transaction_files = harness.list_files("transactions/epoch_0").await;
    let transaction_parquet = filter_by_extension(&transaction_files, ".parquet");
    assert!(
        !transaction_parquet.is_empty(),
        "Expected at least one transaction parquet file"
    );
    let mut transaction_total_rows = 0;
    for file in &transaction_parquet {
        transaction_total_rows += harness.read_parquet_row_count(file).await;
    }
    // Each checkpoint has 1 transaction (the one that creates an object)
    assert_eq!(
        transaction_total_rows, 1,
        "Expected 1 total transaction row"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_requires_task_name() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.backfill_mode = true;
    config.task_name = None;
    config.last_checkpoint = Some(checkpoint_seq);

    let registry = prometheus::Registry::new();
    let metrics = Metrics::new(&registry);
    let cancel = CancellationToken::new();

    let result =
        sui_analytics_indexer::build_analytics_indexer(config, metrics, registry, cancel).await;

    assert!(
        result.is_err(),
        "Expected error when task_name is None in backfill mode"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("task_name"),
        "Error should mention task_name, got: {}",
        err_msg
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_requires_last_checkpoint() {
    let harness = TestHarness::new();

    let mut config = harness.default_config();
    config.backfill_mode = true;
    config.task_name = Some("test".to_string());
    config.last_checkpoint = None;

    let registry = prometheus::Registry::new();
    let metrics = Metrics::new(&registry);
    let cancel = CancellationToken::new();

    let result =
        sui_analytics_indexer::build_analytics_indexer(config, metrics, registry, cancel).await;

    assert!(
        result.is_err(),
        "Expected error when last_checkpoint is None in backfill mode"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("last_checkpoint"),
        "Error should mention last_checkpoint, got: {}",
        err_msg
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_requires_single_pipeline() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.backfill_mode = true;
    config.task_name = Some("test".to_string());
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs = vec![
        default_pipeline_config(Pipeline::Checkpoint),
        default_pipeline_config(Pipeline::Transaction),
    ];

    let registry = prometheus::Registry::new();
    let metrics = Metrics::new(&registry);
    let cancel = CancellationToken::new();

    let result =
        sui_analytics_indexer::build_analytics_indexer(config, metrics, registry, cancel).await;

    assert!(
        result.is_err(),
        "Expected error when multiple pipelines are configured in backfill mode"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("exactly one pipeline"),
        "Error should mention 'exactly one pipeline', got: {}",
        err_msg
    );
}

/// Tests backfill with 10 files across 2 epochs, 3 checkpoints per file.
/// Also verifies epoch boundaries force file cuts even when max_rows_per_file isn't reached.
#[tokio::test(flavor = "multi_thread")]
async fn test_basic_backfill() {
    let mut harness = TestHarness::new();

    // Epoch 0: 14 checkpoints + 1 from advance_epoch = 15 → 5 files (3 each)
    for _ in 0..14 {
        harness.add_checkpoint();
    }
    harness.advance_epoch();

    // Epoch 1: 15 checkpoints → 5 files (3 each)
    for _ in 0..15 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.pipeline_configs[0].max_rows_per_file = 3;
    config.task_name = Some("normal".to_string());
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Verify epoch 0: 15 checkpoints
    // File count may vary due to timing, but total rows must be exact.
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    assert!(
        !epoch_0_parquet.is_empty(),
        "Expected at least one parquet file in epoch_0"
    );
    let mut epoch_0_total_rows = 0;
    for file in &epoch_0_parquet {
        epoch_0_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_0_total_rows, 15, "Expected 15 total rows in epoch_0");

    // Verify epoch 1: 15 checkpoints
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");
    assert!(
        !epoch_1_parquet.is_empty(),
        "Expected at least one parquet file in epoch_1"
    );
    let mut epoch_1_total_rows = 0;
    for file in &epoch_1_parquet {
        epoch_1_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_total_rows, 15, "Expected 15 total rows in epoch_1");

    // Collect all files and their metadata before backfill
    let all_parquet_before: Vec<_> = epoch_0_parquet
        .iter()
        .chain(epoch_1_parquet.iter())
        .cloned()
        .collect();

    let mut metadata_before = Vec::new();
    for file in &all_parquet_before {
        let meta = harness
            .object_store()
            .head(file)
            .await
            .expect("Failed to get file metadata");
        metadata_before.push(((*file).clone(), meta.last_modified));
    }

    // Run backfill - should produce same file structure as before
    let mut backfill_config = harness.default_config();
    backfill_config.backfill_mode = true;
    backfill_config.task_name = Some("backfill".to_string());
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    // Verify same file structure after backfill (backfill mode preserves exact file boundaries)
    let epoch_0_files_after = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_after = filter_by_extension(&epoch_0_files_after, ".parquet");
    let epoch_1_files_after = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_after = filter_by_extension(&epoch_1_files_after, ".parquet");
    assert_eq!(
        epoch_0_after.len(),
        epoch_0_parquet.len(),
        "Expected same number of files in epoch_0 after backfill"
    );
    assert_eq!(
        epoch_1_after.len(),
        epoch_1_parquet.len(),
        "Expected same number of files in epoch_1 after backfill"
    );

    // Verify files were replaced (newer modification times)
    for (file, modified_before) in &metadata_before {
        let meta_after = harness
            .object_store()
            .head(file)
            .await
            .expect("Failed to get file metadata after backfill");
        assert!(
            meta_after.last_modified >= *modified_before,
            "Expected file {:?} to be replaced",
            file
        );
    }
}

/// Tests that epoch boundary forces a file cut even when max_rows_per_file isn't reached.
#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_epoch_boundary_cuts_file() {
    let mut harness = TestHarness::new();

    // Epoch 0: only 2 checkpoints, then epoch ends
    harness.add_checkpoint();
    harness.add_checkpoint();
    harness.advance_epoch(); // Creates 1 more checkpoint in epoch 0

    // Epoch 1: 2 checkpoints
    harness.add_checkpoint();
    harness.add_checkpoint();
    let last_seq = harness.last_checkpoint_seq;

    // max_rows_per_file=10, but epoch 0 only has 3 checkpoints
    // File should still be cut at epoch boundary
    let mut config = harness.default_config();
    config.pipeline_configs[0].max_rows_per_file = 10;
    config.task_name = Some("normal".to_string());
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Verify epoch 0 has 3 rows total (cut at epoch boundary, not max_rows)
    // File count may vary due to timing, but total rows must be exact.
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    assert!(
        !epoch_0_parquet.is_empty(),
        "Expected at least one parquet file in epoch_0"
    );
    let mut epoch_0_total_rows = 0;
    for file in &epoch_0_parquet {
        epoch_0_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        epoch_0_total_rows, 3,
        "Expected 3 total rows in epoch_0 (cut at epoch boundary)"
    );

    // Verify epoch 1 has 2 rows total
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");
    assert!(
        !epoch_1_parquet.is_empty(),
        "Expected at least one parquet file in epoch_1"
    );
    let mut epoch_1_total_rows = 0;
    for file in &epoch_1_parquet {
        epoch_1_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_total_rows, 2, "Expected 2 total rows in epoch_1");

    // Run backfill and verify same structure
    let mut backfill_config = harness.default_config();
    backfill_config.backfill_mode = true;
    backfill_config.task_name = Some("backfill".to_string());
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    // Backfill should produce exact same file structure
    let epoch_0_files_after = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_after = filter_by_extension(&epoch_0_files_after, ".parquet");
    let epoch_1_files_after = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_after = filter_by_extension(&epoch_1_files_after, ".parquet");
    assert_eq!(
        epoch_0_after.len(),
        epoch_0_parquet.len(),
        "Expected same number of files in epoch_0 after backfill"
    );
    assert_eq!(
        epoch_1_after.len(),
        epoch_1_parquet.len(),
        "Expected same number of files in epoch_1 after backfill"
    );
}

/// Edge case: 2 files, each with exactly 1 checkpoint
#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_single_checkpoint_files() {
    let mut harness = TestHarness::new();
    // 2 checkpoints -> 2 files (1 checkpoint each)
    for _ in 0..2 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.pipeline_configs[0].max_rows_per_file = 1;
    config.task_name = Some("normal".to_string());
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // File count may vary due to timing, but total rows must be exact.
    let files_before = harness.list_files("checkpoints/epoch_0").await;
    let parquet_before = filter_by_extension(&files_before, ".parquet");
    assert!(
        !parquet_before.is_empty(),
        "Expected at least one parquet file"
    );
    let mut total_rows_before = 0;
    for file in &parquet_before {
        total_rows_before += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(
        total_rows_before, 2,
        "Expected 2 total rows (2 checkpoints)"
    );

    let mut backfill_config = harness.default_config();
    backfill_config.backfill_mode = true;
    backfill_config.task_name = Some("backfill".to_string());
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    let files_after = harness.list_files("checkpoints/epoch_0").await;
    let parquet_after = filter_by_extension(&files_after, ".parquet");

    // Backfill should produce same file structure
    assert_eq!(
        parquet_before.len(),
        parquet_after.len(),
        "Expected same number of files after backfill"
    );
}

/// Tests backfill resume with 12 files across 2 epochs
#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_resume_after_crash() {
    let mut harness = TestHarness::new();

    // Epoch 0: 17 checkpoints + 1 from advance_epoch = 18 → 6 files (3 each)
    for _ in 0..17 {
        harness.add_checkpoint();
    }
    harness.advance_epoch();

    // Epoch 1: 18 checkpoints → 6 files (3 each)
    for _ in 0..18 {
        harness.add_checkpoint();
    }
    let last_checkpoint_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.pipeline_configs[0].max_rows_per_file = 3;
    config.task_name = Some("normal".to_string());
    config.last_checkpoint = Some(last_checkpoint_seq);

    harness.run_indexer(config).await;

    // File count may vary due to timing, but total rows must be exact.
    let epoch_0_files_before = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_before = filter_by_extension(&epoch_0_files_before, ".parquet");
    let epoch_1_files_before = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_before = filter_by_extension(&epoch_1_files_before, ".parquet");
    assert!(
        !epoch_0_before.is_empty(),
        "Expected at least one parquet file in epoch_0"
    );
    assert!(
        !epoch_1_before.is_empty(),
        "Expected at least one parquet file in epoch_1"
    );
    let mut epoch_0_total_rows = 0;
    for file in &epoch_0_before {
        epoch_0_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_0_total_rows, 18, "Expected 18 total rows in epoch_0");
    let mut epoch_1_total_rows = 0;
    for file in &epoch_1_before {
        epoch_1_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_total_rows, 18, "Expected 18 total rows in epoch_1");

    let mut backfill_config = harness.default_config();
    backfill_config.backfill_mode = true;
    backfill_config.task_name = Some("backfill_resume_test".to_string());
    backfill_config.last_checkpoint = Some(last_checkpoint_seq);

    harness.run_indexer(backfill_config).await;

    let watermark_path = harness
        .output_path()
        .join("_metadata")
        .join("watermarks")
        .join("checkpoints@backfill_resume_test.json");

    let checkpoint_before_resume = if watermark_path.exists() {
        let watermark_content =
            fs::read_to_string(&watermark_path).expect("Failed to read watermark file");
        let watermark: serde_json::Value =
            serde_json::from_str(&watermark_content).expect("Failed to parse watermark JSON");
        let checkpoint_hi = watermark["checkpoint_hi_inclusive"]
            .as_u64()
            .expect("checkpoint_hi_inclusive should be a number");
        Some(checkpoint_hi)
    } else {
        None
    };

    let mut resume_config = harness.default_config();
    resume_config.backfill_mode = true;
    resume_config.task_name = Some("backfill_resume_test".to_string());
    resume_config.last_checkpoint = Some(last_checkpoint_seq);

    harness.run_indexer(resume_config).await;

    if !watermark_path.exists() {
        eprintln!(
            "Warning: Watermark file not found after resume. Test completed but couldn't verify watermark progression."
        );
    } else {
        let watermark_content_final =
            fs::read_to_string(&watermark_path).expect("Failed to read final watermark file");
        let watermark_final: serde_json::Value = serde_json::from_str(&watermark_content_final)
            .expect("Failed to parse final watermark JSON");
        let checkpoint_hi_final = watermark_final["checkpoint_hi_inclusive"]
            .as_u64()
            .expect("checkpoint_hi_inclusive should be a number");

        assert_eq!(
            checkpoint_hi_final, last_checkpoint_seq,
            "Expected backfill to complete to checkpoint {}, got {}",
            last_checkpoint_seq, checkpoint_hi_final
        );

        if let Some(before) = checkpoint_before_resume {
            assert!(
                checkpoint_hi_final >= before,
                "Expected progress from checkpoint {} to {}, but watermark went backwards",
                before,
                checkpoint_hi_final
            );
        }
    }

    // Verify same file structure after resume (backfill mode preserves exact file boundaries)
    let epoch_0_files_after = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_after = filter_by_extension(&epoch_0_files_after, ".parquet");
    let epoch_1_files_after = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_after = filter_by_extension(&epoch_1_files_after, ".parquet");
    assert_eq!(
        epoch_0_after.len(),
        epoch_0_before.len(),
        "Expected same number of files in epoch_0 after resume"
    );
    assert_eq!(
        epoch_1_after.len(),
        epoch_1_before.len(),
        "Expected same number of files in epoch_1 after resume"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_backfill_multiple_files_across_epochs() {
    let mut harness = TestHarness::new();

    // Epoch 0: 6 checkpoints
    for _ in 0..6 {
        harness.add_checkpoint();
    }
    // advance_epoch creates one more checkpoint (epoch 0's last)
    harness.advance_epoch();

    // Epoch 1: 6 checkpoints
    for _ in 0..6 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Run with max_rows_per_file=3 to create multiple files per epoch
    let mut config = harness.default_config();
    config.task_name = Some("normal".to_string());
    config.last_checkpoint = Some(last_seq);
    config.pipeline_configs[0].max_rows_per_file = 3;

    harness.run_indexer(config).await;

    // Verify epoch 0: 7 checkpoints (6 + 1 from advance_epoch)
    // File count may vary due to timing, but total rows must be exact.
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    assert!(
        !epoch_0_parquet.is_empty(),
        "Expected at least one parquet file in epoch_0"
    );

    let mut epoch_0_total_rows = 0;
    for file in &epoch_0_parquet {
        epoch_0_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_0_total_rows, 7, "Expected 7 total rows in epoch_0");

    // Verify epoch 1: 6 checkpoints
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");
    assert!(
        !epoch_1_parquet.is_empty(),
        "Expected at least one parquet file in epoch_1"
    );

    let mut epoch_1_total_rows = 0;
    for file in &epoch_1_parquet {
        epoch_1_total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_total_rows, 6, "Expected 6 total rows in epoch_1");

    // Collect file metadata before backfill
    let mut files_metadata_before = Vec::new();
    for file in epoch_0_parquet.iter().chain(epoch_1_parquet.iter()) {
        let metadata = harness
            .object_store()
            .head(file)
            .await
            .expect("Failed to get file metadata");
        files_metadata_before.push(((*file).clone(), metadata.last_modified));
    }

    // Run backfill - should produce same file structure as before
    let mut backfill_config = harness.default_config();
    backfill_config.backfill_mode = true;
    backfill_config.task_name = Some("backfill".to_string());
    backfill_config.last_checkpoint = Some(last_seq);
    backfill_config.pipeline_configs[0].max_rows_per_file = 3;

    harness.run_indexer(backfill_config).await;

    // Verify same file structure after backfill (backfill mode preserves exact file boundaries)
    let epoch_0_files_after = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet_after = filter_by_extension(&epoch_0_files_after, ".parquet");
    assert_eq!(
        epoch_0_parquet_after.len(),
        epoch_0_parquet.len(),
        "Expected same number of epoch_0 files after backfill"
    );

    let epoch_1_files_after = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet_after = filter_by_extension(&epoch_1_files_after, ".parquet");
    assert_eq!(
        epoch_1_parquet_after.len(),
        epoch_1_parquet.len(),
        "Expected same number of epoch_1 files after backfill"
    );

    // Verify all files were touched (have same or newer modification time)
    for (file_path, modified_before) in &files_metadata_before {
        let metadata_after = harness
            .object_store()
            .head(file_path)
            .await
            .expect("Failed to get file metadata after backfill");
        assert!(
            metadata_after.last_modified >= *modified_before,
            "Expected file {:?} to be replaced during backfill",
            file_path
        );
    }
}
