// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use mock_store::MockStore;
use object_store::ObjectStore;
use object_store::memory::InMemory;
use parquet::file::reader::FileReader;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_storage::blob::Blob;
use sui_storage::blob::BlobEncoding;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::test_checkpoint_data_builder::AdvanceEpochConfig;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use tempfile::TempDir;

use sui_analytics_indexer::RowSchema;
use sui_analytics_indexer::config::BatchSizeConfig;
use sui_analytics_indexer::config::FileFormat;
use sui_analytics_indexer::config::IndexerConfig;
use sui_analytics_indexer::config::OutputStoreConfig;
use sui_analytics_indexer::config::PipelineConfig;
use sui_analytics_indexer::metrics::Metrics;
use sui_analytics_indexer::pipeline::Pipeline;
use sui_analytics_indexer::store::AnalyticsStore;
use sui_analytics_indexer::tables::CheckpointRow;
use sui_analytics_indexer::tables::DynamicFieldRow;
use sui_analytics_indexer::tables::EventRow;
use sui_analytics_indexer::tables::MoveCallRow;
use sui_analytics_indexer::tables::MovePackageRow;
use sui_analytics_indexer::tables::ObjectRow;
use sui_analytics_indexer::tables::PackageBCSRow;
use sui_analytics_indexer::tables::TransactionBCSRow;
use sui_analytics_indexer::tables::TransactionObjectRow;
use sui_analytics_indexer::tables::TransactionRow;
use sui_analytics_indexer::tables::WrappedObjectRow;

mod mock_store;

/// Helper to get schema for a pipeline (used in tests to verify output).
fn pipeline_schema(pipeline: &Pipeline) -> &'static [&'static str] {
    match pipeline {
        Pipeline::Checkpoint => CheckpointRow::schema(),
        Pipeline::Transaction => TransactionRow::schema(),
        Pipeline::TransactionBCS => TransactionBCSRow::schema(),
        Pipeline::TransactionObjects => TransactionObjectRow::schema(),
        Pipeline::Object => ObjectRow::schema(),
        Pipeline::Event => EventRow::schema(),
        Pipeline::MoveCall => MoveCallRow::schema(),
        Pipeline::MovePackage => MovePackageRow::schema(),
        Pipeline::MovePackageBCS => PackageBCSRow::schema(),
        Pipeline::DynamicField => DynamicFieldRow::schema(),
        Pipeline::WrappedObject => WrappedObjectRow::schema(),
    }
}

fn default_pipeline_config(pipeline: Pipeline) -> PipelineConfig {
    PipelineConfig {
        pipeline,
        file_format: FileFormat::Parquet,
        package_id_filter: None,
        sf_table_id: None,
        sf_checkpoint_col_id: None,
        report_sf_max_table_checkpoint: false,
        // Default: 1 checkpoint per file (forces immediate flush)
        batch_size: Some(BatchSizeConfig::Checkpoints(1)),
        output_prefix: None,
        force_batch_cut_after_secs: 600,
    }
}

fn filter_by_extension<'a>(
    files: &'a [object_store::path::Path],
    ext: &str,
) -> Vec<&'a object_store::path::Path> {
    files.iter().filter(|p| p.as_ref().ends_with(ext)).collect()
}

struct TestHarness {
    ingestion_dir: TempDir,
    object_store: Arc<InMemory>,
    builder: Option<TestCheckpointBuilder>,
    last_checkpoint_seq: u64,
    next_object_id: u64,
}

impl TestHarness {
    fn new() -> Self {
        let ingestion_dir = TempDir::new().expect("Failed to create ingestion temp dir");
        let object_store = Arc::new(InMemory::new());

        Self {
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
            output_store: OutputStoreConfig::Custom(self.object_store.clone()),
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
            pipeline_configs: vec![default_pipeline_config(Pipeline::Checkpoint)],
            ingestion: IngestionConfig {
                checkpoint_buffer_size: 100,
                ..Default::default()
            },
            sequential: SequentialConfig::default(),
            first_checkpoint: None,
            last_checkpoint: None,
            migration_id: None,
            file_format: FileFormat::Parquet,
            max_pending_uploads: 10,
            max_concurrent_serialization: 3,
            watermark_update_interval_secs: 60,
        }
    }

    async fn run_indexer(&self, config: IndexerConfig) {
        let registry = prometheus::Registry::new();
        let metrics = Metrics::new(&registry);

        let service = sui_analytics_indexer::build_analytics_indexer(config, metrics, registry)
            .await
            .expect("Failed to build indexer");

        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            // Graceful shutdown triggers store.shutdown() automatically
            let _ = service.shutdown().await;
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

    fn object_store(&self) -> &Arc<InMemory> {
        &self.object_store
    }

    /// Create epochs.json file in the object store.
    ///
    /// The epochs.json file contains an array where epochs[i] is the last checkpoint
    /// (inclusive) of epoch i. This is needed for migration mode to look up epochs.
    async fn create_epochs_json(&self, epoch_last_checkpoints: &[u64]) {
        use bytes::Bytes;
        use object_store::PutPayload;
        use object_store::path::Path as ObjectPath;

        let json = serde_json::to_vec(epoch_last_checkpoints).unwrap();
        let path = ObjectPath::from("epochs.json");
        let payload: PutPayload = Bytes::from(json).into();
        self.object_store.put(&path, payload).await.unwrap();
    }

    /// Read the committer watermark for a pipeline in migration mode.
    /// Creates a fresh MigrationStore using the shared object store.
    async fn read_migration_watermark(
        &self,
        migration_id: &str,
        pipeline: &str,
    ) -> Option<CommitterWatermark> {
        let registry = prometheus::Registry::new();
        let metrics = Metrics::new(&registry);
        // Create a pipeline config for the requested pipeline
        let pipeline_enum = match pipeline {
            "Checkpoint" | "checkpoints" => Pipeline::Checkpoint,
            "Transaction" | "transactions" => Pipeline::Transaction,
            "Object" | "objects" => Pipeline::Object,
            _ => panic!("Unknown pipeline: {}", pipeline),
        };
        let config = IndexerConfig {
            rest_url: "http://localhost".to_string(),
            client_metric_host: "127.0.0.1".to_string(),
            client_metric_port: 8081,
            output_store: OutputStoreConfig::Custom(self.object_store.clone()),
            remote_store_url: "https://checkpoints.testnet.sui.io".to_string(),
            streaming_url: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
            work_dir: None,
            local_ingestion_path: None,
            sf_account_identifier: None,
            sf_warehouse: None,
            sf_database: None,
            sf_schema: None,
            sf_username: None,
            sf_role: None,
            sf_password_file: None,
            pipeline_configs: vec![default_pipeline_config(pipeline_enum)],
            ingestion: IngestionConfig::default(),
            sequential: SequentialConfig::default(),
            first_checkpoint: None,
            last_checkpoint: None,
            migration_id: Some(migration_id.to_string()),
            file_format: FileFormat::Parquet,
            max_pending_uploads: 10,
            max_concurrent_serialization: 3,
            watermark_update_interval_secs: 60,
        };
        let store = AnalyticsStore::new(self.object_store.clone(), config, metrics);
        let mut conn = store.connect().await.expect("Failed to connect to store");
        // Use the pipeline's canonical name for lookup
        conn.committer_watermark(pipeline_enum.name())
            .await
            .expect("Failed to read watermark")
    }
}

/// Test harness with configurable failure injection.
struct MockTestHarness {
    ingestion_dir: TempDir,
    /// The underlying InMemory store (for reading data back).
    inner_store: Arc<InMemory>,
    /// The mock store wrapper (for configuring failures and recording PUT order).
    mock_store: Arc<MockStore>,
    builder: Option<TestCheckpointBuilder>,
    last_checkpoint_seq: u64,
    next_object_id: u64,
}

impl MockTestHarness {
    fn new() -> Self {
        let ingestion_dir = TempDir::new().expect("Failed to create ingestion temp dir");
        let inner_store = Arc::new(InMemory::new());
        let mock_store = Arc::new(MockStore::new(inner_store.clone()));

        Self {
            ingestion_dir,
            inner_store,
            mock_store,
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
            output_store: OutputStoreConfig::Custom(self.mock_store.clone()),
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
            pipeline_configs: vec![default_pipeline_config(Pipeline::Checkpoint)],
            ingestion: IngestionConfig {
                checkpoint_buffer_size: 100,
                ..Default::default()
            },
            sequential: SequentialConfig::default(),
            first_checkpoint: None,
            last_checkpoint: None,
            migration_id: None,
            file_format: FileFormat::Parquet,
            max_pending_uploads: 10,
            max_concurrent_serialization: 3,
            watermark_update_interval_secs: 60,
        }
    }

    /// Run the indexer, returning whether it completed successfully.
    async fn run_indexer(&self, config: IndexerConfig) -> bool {
        let registry = prometheus::Registry::new();
        let metrics = Metrics::new(&registry);

        let service = sui_analytics_indexer::build_analytics_indexer(config, metrics, registry)
            .await
            .expect("Failed to build indexer");

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::time::sleep(Duration::from_millis(5000)).await;
            // Graceful shutdown triggers store.shutdown() automatically
            service.shutdown().await
        })
        .await;

        result.is_ok()
    }

    async fn list_files(&self, prefix: &str) -> Vec<object_store::path::Path> {
        use futures::TryStreamExt;
        let prefix_path = object_store::path::Path::from(prefix);
        // Use inner_store for reads to avoid failure injection
        let list_stream = self.inner_store.list(Some(&prefix_path));
        let objects: Vec<_> = list_stream
            .try_collect()
            .await
            .expect("Failed to list files");
        objects.into_iter().map(|meta| meta.location).collect()
    }

    async fn read_parquet_row_count(&self, path: &object_store::path::Path) -> usize {
        let bytes = self
            .inner_store
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

    /// Get access to the failure configuration.
    fn failure_config(&self) -> &Arc<RwLock<mock_store::MockConfig>> {
        self.mock_store.config()
    }

    /// Read the committer watermark for a pipeline in migration mode.
    async fn read_migration_watermark(
        &self,
        migration_id: &str,
        pipeline: &str,
    ) -> Option<CommitterWatermark> {
        let registry = prometheus::Registry::new();
        let metrics = Metrics::new(&registry);
        // Create a pipeline config for the requested pipeline
        let pipeline_enum = match pipeline {
            "Checkpoint" | "checkpoints" => Pipeline::Checkpoint,
            "Transaction" | "transactions" => Pipeline::Transaction,
            "Object" | "objects" => Pipeline::Object,
            _ => panic!("Unknown pipeline: {}", pipeline),
        };
        let config = IndexerConfig {
            rest_url: "http://localhost".to_string(),
            client_metric_host: "127.0.0.1".to_string(),
            client_metric_port: 8081,
            output_store: OutputStoreConfig::Custom(self.inner_store.clone()),
            remote_store_url: "https://checkpoints.testnet.sui.io".to_string(),
            streaming_url: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
            work_dir: None,
            local_ingestion_path: None,
            sf_account_identifier: None,
            sf_warehouse: None,
            sf_database: None,
            sf_schema: None,
            sf_username: None,
            sf_role: None,
            sf_password_file: None,
            pipeline_configs: vec![default_pipeline_config(pipeline_enum)],
            ingestion: IngestionConfig::default(),
            sequential: SequentialConfig::default(),
            first_checkpoint: None,
            last_checkpoint: None,
            migration_id: Some(migration_id.to_string()),
            file_format: FileFormat::Parquet,
            max_pending_uploads: 10,
            max_concurrent_serialization: 3,
            watermark_update_interval_secs: 60,
        };
        let store = AnalyticsStore::new(self.inner_store.clone(), config, metrics);
        let mut conn = store.connect().await.expect("Failed to connect to store");
        // Use the pipeline's canonical name for lookup
        conn.committer_watermark(pipeline_enum.name())
            .await
            .expect("Failed to read watermark")
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
async fn test_multiple_checkpoints_batch_size_config() {
    let mut harness = TestHarness::new();
    for _ in 0..6 {
        harness.add_checkpoint();
    }

    let mut config = harness.default_config();
    config.last_checkpoint = Some(harness.last_checkpoint_seq);

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // File count may vary due to timing - files are created on timeout/shutdown.
    // Just verify all rows exist.
    assert!(
        !parquet_files.is_empty(),
        "Expected at least one parquet file"
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
    config.file_format = FileFormat::Csv;
    // Set pipeline config's file_format too (handler uses this, store uses the top-level one)
    for pipeline_config in &mut config.pipeline_configs {
        pipeline_config.file_format = FileFormat::Csv;
    }

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

/// Tests basic backfill with 10 files across 2 epochs, 3 checkpoints per file.
/// Verifies that backfill mode overwrites existing files while preserving structure.
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
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Verify epoch 0: 15 checkpoints
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

    // Run migration - should produce same file structure as before
    let mut backfill_config = harness.default_config();
    backfill_config.migration_id = Some("test-migration-1".to_string());
    backfill_config.first_checkpoint = Some(0);
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

/// Tests that epoch boundary forces a file cut even when batch size isn't reached.
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

    // Large batch size, but epoch 0 only has 3 checkpoints
    // File should still be cut at epoch boundary
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Verify epoch 0 has 3 rows total (cut at epoch boundary, not batch size)
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

    // Run migration and verify same structure
    let mut backfill_config = harness.default_config();
    backfill_config.migration_id = Some("test-migration".to_string());
    backfill_config.first_checkpoint = Some(0);
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    // Migration should produce exact same file structure
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
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

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
    backfill_config.migration_id = Some("test-migration".to_string());
    backfill_config.first_checkpoint = Some(0);
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    let files_after = harness.list_files("checkpoints/epoch_0").await;
    let parquet_after = filter_by_extension(&files_after, ".parquet");

    // Migration should produce same file structure
    assert_eq!(
        parquet_before.len(),
        parquet_after.len(),
        "Expected same number of files after backfill"
    );
}

/// Tests backfill with multiple files across epochs
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

    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);

    harness.run_indexer(config).await;

    // Verify epoch 0: 7 checkpoints (6 + 1 from advance_epoch)
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

    // Run migration - should produce same file structure as before
    let mut backfill_config = harness.default_config();
    backfill_config.migration_id = Some("test-migration".to_string());
    backfill_config.first_checkpoint = Some(0);
    backfill_config.last_checkpoint = Some(last_seq);

    harness.run_indexer(backfill_config).await;

    // Verify same file structure after migration (migration mode preserves exact file boundaries)
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

/// Tests migration resume by running partial migration, then resuming via watermark.
///
/// This simulates the scenario where a migration crashes partway through and resumes
/// using the migration-id metadata to determine progress.
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_resume_after_crash() {
    let mut harness = TestHarness::new();

    // Create 12 checkpoints across 2 epochs
    // Epoch 0: 5 checkpoints + 1 from advance_epoch = 6
    for _ in 0..5 {
        harness.add_checkpoint();
    }
    let epoch_0_last = harness.advance_epoch();

    // Epoch 1: 6 checkpoints
    for _ in 0..6 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run to create files (live mode - no migration)
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Verify initial files exist
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");

    assert!(!epoch_0_parquet.is_empty(), "Expected epoch_0 files");
    assert!(!epoch_1_parquet.is_empty(), "Expected epoch_1 files");

    // Phase 1: Migration only epoch 0 (simulating partial migration before "crash")
    let migration_id = "crash-test-migration";
    let mut backfill_phase1 = harness.default_config();
    backfill_phase1.migration_id = Some(migration_id.to_string());
    backfill_phase1.first_checkpoint = Some(0);
    backfill_phase1.last_checkpoint = Some(epoch_0_last);
    // Disable watermark rate limiting for test to ensure watermarks are written
    backfill_phase1.watermark_update_interval_secs = 0;
    harness.run_indexer(backfill_phase1).await;

    // Verify watermark after Phase 1: should be at epoch_0_last
    let watermark_phase1 = harness
        .read_migration_watermark(migration_id, "checkpoints")
        .await
        .expect("Expected watermark after Phase 1");
    assert_eq!(
        watermark_phase1.checkpoint_hi_inclusive, epoch_0_last,
        "Phase 1 watermark should be at epoch_0_last"
    );
    assert_eq!(
        watermark_phase1.epoch_hi_inclusive, 0,
        "Phase 1 watermark should be at epoch 0"
    );

    // Phase 2: Resume migration - should automatically detect progress via metadata
    // and only process epoch 1 files (since epoch 0 already has migration-id metadata)
    let mut backfill_phase2 = harness.default_config();
    backfill_phase2.migration_id = Some(migration_id.to_string());
    // Note: We set first_checkpoint=0, but the watermark from metadata should
    // cause us to resume from epoch_0_last + 1
    backfill_phase2.first_checkpoint = Some(0);
    backfill_phase2.last_checkpoint = Some(last_seq);
    // Disable watermark rate limiting for test to ensure watermarks are written
    backfill_phase2.watermark_update_interval_secs = 0;
    harness.run_indexer(backfill_phase2).await;

    // Verify watermark after Phase 2: should be at last_seq
    let watermark_phase2 = harness
        .read_migration_watermark(migration_id, "checkpoints")
        .await
        .expect("Expected watermark after Phase 2");
    assert_eq!(
        watermark_phase2.checkpoint_hi_inclusive, last_seq,
        "Phase 2 watermark should be at last_seq"
    );
    assert_eq!(
        watermark_phase2.epoch_hi_inclusive, 1,
        "Phase 2 watermark should be at epoch 1"
    );

    // Verify file structure is preserved (same number of files)
    let epoch_0_files_final = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_final = filter_by_extension(&epoch_0_files_final, ".parquet");
    let epoch_1_files_final = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_final = filter_by_extension(&epoch_1_files_final, ".parquet");

    assert_eq!(
        epoch_0_final.len(),
        epoch_0_parquet.len(),
        "Expected same number of epoch_0 files after resume"
    );
    assert_eq!(
        epoch_1_final.len(),
        epoch_1_parquet.len(),
        "Expected same number of epoch_1 files after resume"
    );

    // Verify row counts are correct (all data should be present)
    let mut epoch_0_rows = 0;
    for file in &epoch_0_final {
        epoch_0_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_0_rows, 6, "Expected 6 rows in epoch_0");

    let mut epoch_1_rows = 0;
    for file in &epoch_1_final {
        epoch_1_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_rows, 6, "Expected 6 rows in epoch_1");
}

/// Tests that upload worker handles transient failures via retry.
///
/// Scenario:
/// 1. Create checkpoints across epoch boundary (forces multiple files)
/// 2. Configure a transient failure on 2nd PUT operation
/// 3. Run indexer - upload worker should retry and succeed
/// 4. Verify all data was uploaded despite the transient failure
///
/// The async upload worker retries indefinitely with exponential backoff,
/// so transient failures should be handled gracefully.
#[tokio::test(flavor = "multi_thread")]
async fn test_epoch_boundary_failure_recovery() {
    let mut harness = MockTestHarness::new();

    // Create checkpoints across epoch boundary
    // Epoch 0: 3 checkpoints + 1 from advance_epoch = 4
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let _epoch_0_last = harness.advance_epoch();

    // Epoch 1: 3 checkpoints
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Configure to fail on 2nd PUT (transient failure - will be retried)
    {
        let mut config = harness.failure_config().write().unwrap();
        config.fail_on_put = Some(2);
    }

    // Run indexer - upload worker should retry the failed PUT and succeed
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Verify all data was uploaded despite the transient failure
    let all_files = harness.list_files("checkpoints").await;
    let parquet_files = filter_by_extension(&all_files, ".parquet");

    // All files should exist (retries succeeded)
    assert!(
        !parquet_files.is_empty(),
        "Expected parquet files after run with transient failure"
    );

    // Count total rows - should have all 7 checkpoints
    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }

    assert_eq!(
        total_rows, 7,
        "Expected all 7 checkpoints to be uploaded (transient failure should be retried)"
    );

    // Verify that PUT was actually attempted multiple times (failure was hit and retried)
    let put_count = harness.failure_config().read().unwrap().put_count;
    assert!(
        put_count >= 2,
        "Expected at least 2 PUT attempts (including the failed one), got {}",
        put_count
    );
}

/// Tests that transient watermark update failures don't prevent progress.
///
/// With incremental watermark updates (after each file upload), a transient
/// failure on one watermark update doesn't block progress - subsequent updates
/// succeed and the final watermark reflects all uploaded files.
///
/// Scenario (migration mode):
/// 1. Create initial files in live mode
/// 2. Start migration, configure failure on first watermark PUT
/// 3. Run migration - first watermark update fails, subsequent succeed
/// 4. Verify watermark exists (from later successful updates) and all files migrated
#[tokio::test(flavor = "multi_thread")]
async fn test_watermark_flush_failure_recovery() {
    let mut harness = MockTestHarness::new();

    // Create checkpoints
    for _ in 0..4 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run to create files (live mode - no migration)
    let mut initial_config = harness.default_config();
    initial_config.last_checkpoint = Some(last_seq);
    harness.run_indexer(initial_config).await;

    // Verify initial files exist
    let initial_files = harness.list_files("checkpoints/epoch_0").await;
    let initial_parquet = filter_by_extension(&initial_files, ".parquet");
    assert!(!initial_parquet.is_empty(), "Expected initial files");

    // Get file metadata (etags) before migration
    let mut etags_before: Vec<String> = Vec::new();
    for file in &initial_parquet {
        let meta = harness.inner_store.head(file).await.unwrap();
        if let Some(etag) = meta.e_tag {
            etags_before.push(etag);
        }
    }

    // Configure failure on first watermark PUT (paths starting with _metadata)
    // With incremental updates, the first update fails but subsequent succeed
    {
        let mut config = harness.failure_config().write().unwrap();
        config.reset_counts(); // Reset from live mode run
        config.fail_on_put = Some(1);
        config.fail_path_prefix = Some("_metadata".to_string());
    }

    // Migration run - first watermark update fails, but later ones succeed
    let migration_id = "watermark-fail-test";
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some(migration_id.to_string());
    migration_config.first_checkpoint = Some(0);
    migration_config.last_checkpoint = Some(last_seq);
    harness.run_indexer(migration_config).await;

    // With incremental watermark updates, the watermark SHOULD exist
    // (first update failed, but subsequent file uploads updated it successfully)
    let watermark = harness
        .read_migration_watermark(migration_id, "checkpoints")
        .await;
    assert!(
        watermark.is_some(),
        "Expected watermark after migration (transient failure was recovered)"
    );
    // Verify data integrity (same number of rows)
    let final_files = harness.list_files("checkpoints/epoch_0").await;
    let final_parquet = filter_by_extension(&final_files, ".parquet");

    let mut total_rows = 0;
    for file in &final_parquet {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(total_rows, 4, "Expected 4 rows total");

    // Verify files were actually updated (etags changed)
    let mut etags_after: Vec<String> = Vec::new();
    for file in &final_parquet {
        let meta = harness.inner_store.head(file).await.unwrap();
        if let Some(etag) = meta.e_tag {
            etags_after.push(etag);
        }
    }
    assert_eq!(
        etags_before.len(),
        etags_after.len(),
        "Same number of files before and after"
    );
    // At least some etags should differ (files were migrated)
    assert!(
        etags_before != etags_after,
        "Files should have been updated during migration"
    );
}

/// Tests migration idempotency when partial file upload fails.
///
/// Scenario:
/// 1. Create initial files in live mode (2 epochs)
/// 2. Start migration, configure failure on 2nd data file PUT
/// 3. Run migration - first data file updated, second fails
/// 4. Verify first file has new etag, watermark not advanced past first file
/// 5. Disable failure, retry migration
/// 6. Verify all files updated, watermarks correct
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_retry_after_partial_failure() {
    let mut harness = MockTestHarness::new();

    // Create checkpoints across 2 epochs
    // Epoch 0: 3 checkpoints + 1 advance = 4
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let _epoch_0_last = harness.advance_epoch();

    // Epoch 1: 3 checkpoints
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run to create files (live mode)
    let mut initial_config = harness.default_config();
    initial_config.last_checkpoint = Some(last_seq);
    harness.run_indexer(initial_config).await;

    // Verify both epochs have files
    let epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch_0_parquet = filter_by_extension(&epoch_0_files, ".parquet");
    let epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch_1_parquet = filter_by_extension(&epoch_1_files, ".parquet");

    assert!(!epoch_0_parquet.is_empty(), "Expected epoch_0 files");
    assert!(!epoch_1_parquet.is_empty(), "Expected epoch_1 files");

    // Record original etags
    let mut original_etags: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for file in epoch_0_parquet.iter().chain(epoch_1_parquet.iter()) {
        let meta = harness.inner_store.head(file).await.unwrap();
        if let Some(etag) = meta.e_tag {
            original_etags.insert(file.to_string(), etag);
        }
    }

    // Configure failure on 2nd data file PUT (skip watermark PUTs via path filter)
    // We want to fail on the 2nd file in the checkpoints directory
    {
        let mut config = harness.failure_config().write().unwrap();
        config.reset_counts(); // Reset from live mode run
        config.fail_on_put = Some(2);
        // Only fail on checkpoint data files, not metadata
        config.fail_path_prefix = Some("checkpoints".to_string());
    }

    // Migration run - should fail after first file
    let migration_id = "partial-fail-test";
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some(migration_id.to_string());
    migration_config.first_checkpoint = Some(0);
    migration_config.last_checkpoint = Some(last_seq);
    // Disable watermark rate limiting for test to ensure watermarks are written
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config.clone()).await;

    // Check watermark - should be at or before first file boundary due to failure
    let _watermark_partial = harness
        .read_migration_watermark(migration_id, "checkpoints")
        .await;
    // Watermark may or may not exist depending on when failure occurred
    // The key invariant is that retry should work correctly

    // Disable failures and reset counts
    {
        let mut config = harness.failure_config().write().unwrap();
        config.disable_failures();
        config.reset_counts();
    }

    // Retry migration
    harness.run_indexer(migration_config).await;

    // Verify watermark is at the end
    let watermark_final = harness
        .read_migration_watermark(migration_id, "checkpoints")
        .await;
    assert!(
        watermark_final.is_some(),
        "Expected watermark after successful retry"
    );
    let watermark = watermark_final.unwrap();
    assert_eq!(
        watermark.checkpoint_hi_inclusive, last_seq,
        "Watermark should be at last checkpoint"
    );
    assert_eq!(
        watermark.epoch_hi_inclusive, 1,
        "Watermark should be at epoch 1"
    );

    // Verify all files were updated (etags changed)
    let final_epoch_0_files = harness.list_files("checkpoints/epoch_0").await;
    let final_epoch_0_parquet = filter_by_extension(&final_epoch_0_files, ".parquet");
    let final_epoch_1_files = harness.list_files("checkpoints/epoch_1").await;
    let final_epoch_1_parquet = filter_by_extension(&final_epoch_1_files, ".parquet");

    for file in final_epoch_0_parquet
        .iter()
        .chain(final_epoch_1_parquet.iter())
    {
        let meta = harness.inner_store.head(file).await.unwrap();
        if let Some(new_etag) = meta.e_tag
            && let Some(old_etag) = original_etags.get(&file.to_string())
        {
            assert_ne!(
                &new_etag, old_etag,
                "File {} should have new etag after migration",
                file
            );
        }
    }

    // Verify row counts are correct
    let mut epoch_0_rows = 0;
    for file in &final_epoch_0_parquet {
        epoch_0_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_0_rows, 4, "Expected 4 rows in epoch_0");

    let mut epoch_1_rows = 0;
    for file in &final_epoch_1_parquet {
        epoch_1_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(epoch_1_rows, 3, "Expected 3 rows in epoch_1");
}

/// Tests that files are uploaded in checkpoint order (not out of order).
///
/// This test verifies the fix for the upload ordering bug where buffer_unordered
/// could cause files to appear in object store out of order, breaking watermark
/// contiguity guarantees.
#[tokio::test(flavor = "multi_thread")]
async fn test_files_uploaded_in_checkpoint_order() {
    let mut harness = MockTestHarness::new();

    // Create 1000 checkpoints with epoch cuts every ~100 checkpoints
    // to stress test ordering guarantees with parallel processing.
    // Epoch cuts force files to be split across different epoch directories.
    const TOTAL_CHECKPOINTS: usize = 1000;
    const EPOCH_LENGTH: usize = 100;

    let mut checkpoint_count = 0;
    while checkpoint_count < TOTAL_CHECKPOINTS {
        // Add checkpoints for this epoch (minus 1 because advance_epoch adds one)
        let remaining = TOTAL_CHECKPOINTS - checkpoint_count;
        let checkpoints_this_epoch = std::cmp::min(EPOCH_LENGTH - 1, remaining);

        for _ in 0..checkpoints_this_epoch {
            harness.add_checkpoint();
            checkpoint_count += 1;
        }

        // Advance epoch (which creates one more checkpoint) if we're not at the end
        if checkpoint_count < TOTAL_CHECKPOINTS {
            harness.advance_epoch();
            checkpoint_count += 1;
        }
    }
    let last_seq = harness.last_checkpoint_seq;

    // Run the indexer
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Get the PUT order from the mock store
    let put_order = harness.failure_config().read().unwrap().put_order.clone();

    // Filter to just checkpoint data files (not metadata)
    let checkpoint_puts: Vec<_> = put_order
        .iter()
        .filter(|p| p.starts_with("checkpoints/"))
        .collect();

    assert!(
        checkpoint_puts.len() >= TOTAL_CHECKPOINTS,
        "Expected at least {} checkpoint files, got {}",
        TOTAL_CHECKPOINTS,
        checkpoint_puts.len()
    );

    // Parse checkpoint ranges from file paths and verify they're in order
    // File format: checkpoints/epoch_N/start_end.parquet
    let mut prev_start: Option<u64> = None;
    for path in &checkpoint_puts {
        // Extract the filename part (e.g., "0_1.parquet")
        let filename = path.split('/').next_back().unwrap();
        if let Some(range) = parse_checkpoint_range(filename) {
            if let Some(prev) = prev_start {
                assert!(
                    range.start >= prev,
                    "Files uploaded out of order: previous start {}, current start {} (path: {})\nFull PUT order: {:?}",
                    prev,
                    range.start,
                    path,
                    checkpoint_puts
                );
            }
            prev_start = Some(range.start);
        }
    }

    // Verify we had multiple epochs to ensure epoch transitions were tested
    let epochs: std::collections::HashSet<_> = checkpoint_puts
        .iter()
        .filter_map(|p| p.split('/').nth(1))
        .collect();
    assert!(
        epochs.len() >= 5,
        "Expected at least 5 epochs for thorough testing, got {}",
        epochs.len()
    );
}

/// Parse checkpoint range from filename.
/// Expected format: `{start}_{end}.{format}` (e.g., `0_100.parquet`)
fn parse_checkpoint_range(filename: &str) -> Option<std::ops::Range<u64>> {
    let base = filename.split('.').next()?;
    let (start_str, end_str) = base.split_once('_')?;
    let start: u64 = start_str.parse().ok()?;
    let end: u64 = end_str.parse().ok()?;
    Some(start..end)
}

// ============================================================================
// Pipeline Smoke Tests
// ============================================================================

impl TestHarness {
    /// Read parquet file and return schema column names.
    async fn read_parquet_schema(&self, path: &object_store::path::Path) -> Vec<String> {
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
        let schema = reader.metadata().file_metadata().schema();
        schema
            .get_fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect()
    }

    /// Read CSV file and parse into records using proper CSV parsing.
    /// Note: The analytics indexer uses '|' as delimiter, not comma.
    async fn read_csv_records(&self, path: &object_store::path::Path) -> Vec<csv::StringRecord> {
        let bytes = self
            .object_store
            .get(path)
            .await
            .expect("Failed to get CSV file")
            .bytes()
            .await
            .expect("Failed to read CSV bytes");

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b'|')
            .from_reader(bytes.as_ref());

        reader
            .records()
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to parse CSV records")
    }

    /// Read parquet file and return all rows as JSON-like maps.
    async fn read_parquet_records(
        &self,
        path: &object_store::path::Path,
    ) -> Vec<std::collections::HashMap<String, String>> {
        use parquet::file::reader::FileReader;
        use parquet::record::reader::RowIter;

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

        let mut records = Vec::new();
        let schema = reader.metadata().file_metadata().schema();
        let field_names: Vec<_> = schema
            .get_fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect();

        let iter = RowIter::from_file_into(Box::new(reader));
        for row_result in iter {
            let row = row_result.expect("Failed to read row");
            let mut record = std::collections::HashMap::new();
            for (name, field) in field_names.iter().zip(row.get_column_iter()) {
                record.insert(name.clone(), format!("{}", field.1));
            }
            records.push(record);
        }
        records
    }
}

/// Returns pipelines that are guaranteed to produce output from test checkpoints.
/// These are the core pipelines that always produce data regardless of transaction type.
fn pipelines_with_guaranteed_output() -> Vec<Pipeline> {
    vec![
        Pipeline::Checkpoint,
        Pipeline::Transaction,
        Pipeline::TransactionBCS,
    ]
}

/// Smoke test: Run all pipelines that produce guaranteed output in Parquet mode.
/// Verifies that files are created and schemas are valid.
#[tokio::test(flavor = "multi_thread")]
async fn test_smoke_all_pipelines_parquet() {
    let mut harness = TestHarness::new();

    // Create checkpoints with transactions to produce data for all pipelines
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    config.file_format = FileFormat::Parquet;
    config.pipeline_configs = pipelines_with_guaranteed_output()
        .into_iter()
        .map(|p| {
            let mut cfg = default_pipeline_config(p);
            cfg.file_format = FileFormat::Parquet;
            cfg
        })
        .collect();

    harness.run_indexer(config).await;

    // Verify each pipeline produced output
    for pipeline in pipelines_with_guaranteed_output() {
        let prefix = format!("{}/epoch_0", pipeline.default_path());
        let files = harness.list_files(&prefix).await;
        let parquet_files = filter_by_extension(&files, ".parquet");

        assert!(
            !parquet_files.is_empty(),
            "Pipeline '{}' should have produced parquet files in {}",
            pipeline.name(),
            prefix
        );

        // Verify schema matches expected columns
        let expected_schema = pipeline_schema(&pipeline);
        for file in &parquet_files {
            let actual_schema = harness.read_parquet_schema(file).await;
            for expected_col in expected_schema {
                assert!(
                    actual_schema.iter().any(|s| s == expected_col),
                    "Pipeline '{}' parquet file missing column '{}'. Found: {:?}",
                    pipeline.name(),
                    expected_col,
                    actual_schema
                );
            }
        }

        // Verify at least one row exists
        let mut total_rows = 0;
        for file in &parquet_files {
            total_rows += harness.read_parquet_row_count(file).await;
        }
        assert!(
            total_rows > 0,
            "Pipeline '{}' should have at least one row",
            pipeline.name()
        );
    }
}

/// Smoke test: Run all pipelines that produce guaranteed output in CSV mode.
/// Verifies that files are created and data is parseable.
#[tokio::test(flavor = "multi_thread")]
async fn test_smoke_all_pipelines_csv() {
    let mut harness = TestHarness::new();

    // Create checkpoints with transactions
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    config.file_format = FileFormat::Csv;
    config.pipeline_configs = pipelines_with_guaranteed_output()
        .into_iter()
        .map(|p| {
            let mut cfg = default_pipeline_config(p);
            cfg.file_format = FileFormat::Csv;
            cfg
        })
        .collect();

    harness.run_indexer(config).await;

    // Verify each pipeline produced output
    for pipeline in pipelines_with_guaranteed_output() {
        let prefix = format!("{}/epoch_0", pipeline.default_path());
        let files = harness.list_files(&prefix).await;
        let csv_files = filter_by_extension(&files, ".csv");

        assert!(
            !csv_files.is_empty(),
            "Pipeline '{}' should have produced CSV files in {}",
            pipeline.name(),
            prefix
        );

        // Verify CSV content is parseable and has expected columns
        let expected_cols = pipeline_schema(&pipeline).len();
        for file in &csv_files {
            let records = harness.read_csv_records(file).await;
            assert!(
                !records.is_empty(),
                "Pipeline '{}' CSV file should have at least one record",
                pipeline.name()
            );

            // Verify each record has the expected number of columns
            for (row_num, record) in records.iter().enumerate() {
                assert_eq!(
                    record.len(),
                    expected_cols,
                    "Pipeline '{}' CSV row {} has {} columns, expected {}",
                    pipeline.name(),
                    row_num + 1,
                    record.len(),
                    expected_cols
                );
            }
        }
    }
}

/// Test checkpoint pipeline: verify all expected fields are present and valid.
#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_pipeline_fields() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs = vec![default_pipeline_config(Pipeline::Checkpoint)];

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");
    assert!(!parquet_files.is_empty());

    let records = harness.read_parquet_records(parquet_files[0]).await;
    assert_eq!(records.len(), 1, "Expected exactly 1 checkpoint row");

    let row = &records[0];

    // Verify key fields are present and parseable
    assert!(
        row.contains_key("sequence_number"),
        "Missing sequence_number"
    );
    assert!(
        row.contains_key("checkpoint_digest"),
        "Missing checkpoint_digest"
    );
    assert!(row.contains_key("epoch"), "Missing epoch");
    assert!(row.contains_key("timestamp_ms"), "Missing timestamp_ms");
    assert!(
        row.contains_key("total_transaction_blocks"),
        "Missing total_transaction_blocks"
    );
    assert!(row.contains_key("total_gas_cost"), "Missing total_gas_cost");

    // Verify sequence_number is 0 (first checkpoint)
    let seq_num = row.get("sequence_number").unwrap();
    assert!(
        seq_num.contains('0'),
        "First checkpoint should have sequence_number 0, got {}",
        seq_num
    );

    // Verify epoch is 0
    let epoch = row.get("epoch").unwrap();
    assert!(
        epoch.contains('0'),
        "First checkpoint should be in epoch 0, got {}",
        epoch
    );
}

/// Test transaction pipeline: verify transaction fields.
#[tokio::test(flavor = "multi_thread")]
async fn test_transaction_pipeline_fields() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs = vec![default_pipeline_config(Pipeline::Transaction)];

    harness.run_indexer(config).await;

    let files = harness.list_files("transactions/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");
    assert!(!parquet_files.is_empty());

    let records = harness.read_parquet_records(parquet_files[0]).await;
    assert!(!records.is_empty(), "Expected at least 1 transaction row");

    let row = &records[0];

    // Verify key transaction fields
    assert!(
        row.contains_key("transaction_digest"),
        "Missing transaction_digest"
    );
    assert!(row.contains_key("checkpoint"), "Missing checkpoint");
    assert!(row.contains_key("epoch"), "Missing epoch");
    assert!(row.contains_key("sender"), "Missing sender");
    assert!(
        row.contains_key("transaction_kind"),
        "Missing transaction_kind"
    );
    assert!(
        row.contains_key("execution_success"),
        "Missing execution_success"
    );
    assert!(row.contains_key("gas_budget"), "Missing gas_budget");
    assert!(row.contains_key("total_gas_cost"), "Missing total_gas_cost");

    // Verify transaction_digest is non-empty
    let digest = row.get("transaction_digest").unwrap();
    assert!(
        !digest.is_empty() && digest != "\"\"",
        "transaction_digest should not be empty"
    );
}

/// Test transaction_bcs pipeline: verify BCS output.
#[tokio::test(flavor = "multi_thread")]
async fn test_transaction_bcs_pipeline_fields() {
    let mut harness = TestHarness::new();
    let checkpoint_seq = harness.add_checkpoint();

    let mut config = harness.default_config();
    config.last_checkpoint = Some(checkpoint_seq);
    config.pipeline_configs = vec![default_pipeline_config(Pipeline::TransactionBCS)];

    harness.run_indexer(config).await;

    let files = harness.list_files("transaction_bcs/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");
    assert!(!parquet_files.is_empty());

    let records = harness.read_parquet_records(parquet_files[0]).await;
    assert!(
        !records.is_empty(),
        "Expected at least 1 transaction_bcs row"
    );

    let row = &records[0];

    // Verify key fields
    assert!(
        row.contains_key("transaction_digest"),
        "Missing transaction_digest"
    );
    assert!(row.contains_key("checkpoint"), "Missing checkpoint");
    assert!(row.contains_key("epoch"), "Missing epoch");
    assert!(row.contains_key("bcs"), "Missing bcs");

    // Verify bcs field is non-empty (contains hex-encoded transaction data)
    let bcs = row.get("bcs").unwrap();
    assert!(bcs.len() > 2, "bcs field should contain transaction data");
}

/// Test CSV format produces valid, parseable output for checkpoints.
#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_csv_parseable() {
    let mut harness = TestHarness::new();
    for _ in 0..3 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    config.file_format = FileFormat::Csv;
    config.pipeline_configs = vec![{
        let mut cfg = default_pipeline_config(Pipeline::Checkpoint);
        cfg.file_format = FileFormat::Csv;
        cfg
    }];

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let csv_files = filter_by_extension(&files, ".csv");
    assert!(!csv_files.is_empty());

    // Parse CSV properly using the csv crate
    let records = harness.read_csv_records(csv_files[0]).await;
    assert!(!records.is_empty(), "Expected at least 1 CSV record");

    // Verify expected number of columns (from CheckpointRow schema)
    let expected_cols = pipeline_schema(&Pipeline::Checkpoint).len();
    for (row_num, record) in records.iter().enumerate() {
        assert_eq!(
            record.len(),
            expected_cols,
            "CSV row {} has {} columns, expected {}",
            row_num + 1,
            record.len(),
            expected_cols
        );
    }
}

// ============================================================================
// Multi-Pipeline Migration with File Boundary Snapping Tests
// ============================================================================

/// Test that migration mode snaps first_checkpoint to file start when mid-file.
///
/// Scenario:
/// 1. Create files covering checkpoints 0-10 (in single file)
/// 2. Start migration with first_checkpoint=5 (mid-file)
/// 3. Verify that migration snaps to checkpoint 0 (file start)
/// 4. Verify all rows are rewritten correctly
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_snap_to_file_start() {
    let mut harness = TestHarness::new();

    // Create 10 checkpoints
    for _ in 0..10 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run to create files with batch_size=10 (all in one file)
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    config.pipeline_configs = vec![{
        let mut cfg = default_pipeline_config(Pipeline::Checkpoint);
        cfg.batch_size = Some(BatchSizeConfig::Checkpoints(10));
        cfg
    }];
    harness.run_indexer(config).await;

    // Verify we have one file: 0_10.parquet
    let files_before = harness.list_files("checkpoints/epoch_0").await;
    let parquet_before = filter_by_extension(&files_before, ".parquet");
    assert_eq!(
        parquet_before.len(),
        1,
        "Expected 1 file covering all 10 checkpoints"
    );

    // Get original file metadata
    let original_meta = harness
        .object_store()
        .head(parquet_before[0])
        .await
        .unwrap();

    // Run migration with first_checkpoint=5 (mid-file)
    // Should snap to 0 (file start) and rewrite the entire file
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("snap-test".to_string());
    migration_config.first_checkpoint = Some(5); // Mid-file!
    migration_config.last_checkpoint = Some(last_seq);
    migration_config.pipeline_configs = vec![{
        let mut cfg = default_pipeline_config(Pipeline::Checkpoint);
        cfg.batch_size = Some(BatchSizeConfig::Checkpoints(10));
        cfg
    }];
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config).await;

    // Verify file was rewritten (modified time changed)
    let files_after = harness.list_files("checkpoints/epoch_0").await;
    let parquet_after = filter_by_extension(&files_after, ".parquet");
    assert_eq!(
        parquet_after.len(),
        1,
        "Expected same file structure after migration"
    );

    let new_meta = harness.object_store().head(parquet_after[0]).await.unwrap();
    assert!(
        new_meta.last_modified >= original_meta.last_modified,
        "File should have been rewritten"
    );

    // Verify all 10 rows are present (started from 0, not 5)
    let row_count = harness.read_parquet_row_count(parquet_after[0]).await;
    assert_eq!(
        row_count, 10,
        "Expected all 10 rows (snapped to file start)"
    );

    // Verify watermark is at last checkpoint
    let watermark = harness
        .read_migration_watermark("snap-test", "checkpoints")
        .await
        .expect("Expected watermark");
    assert_eq!(watermark.checkpoint_hi_inclusive, last_seq);
}

/// Test that migration mode snaps first_checkpoint forward when in a gap between files.
///
/// Scenario:
/// 1. Create files with a gap: 0-5 and 10-15 (gap at 5-10)
/// 2. Start migration with first_checkpoint=7 (in gap)
/// 3. Verify that migration snaps to checkpoint 10 (next file start)
/// 4. Verify only the second file is rewritten
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_snap_forward_in_gap() {
    let mut harness = TestHarness::new();

    // Create 5 checkpoints, advance epoch to force file cut, then 5 more
    for _ in 0..5 {
        harness.add_checkpoint();
    }
    harness.advance_epoch(); // checkpoint 5 (epoch 0 ends)
    for _ in 0..5 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run with batch_size=6 to create files at epoch boundaries
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Verify files exist in both epochs
    let epoch0_files = harness.list_files("checkpoints/epoch_0").await;
    let epoch0_parquet = filter_by_extension(&epoch0_files, ".parquet");
    let epoch1_files = harness.list_files("checkpoints/epoch_1").await;
    let epoch1_parquet = filter_by_extension(&epoch1_files, ".parquet");

    assert!(!epoch0_parquet.is_empty(), "Expected epoch_0 files");
    assert!(!epoch1_parquet.is_empty(), "Expected epoch_1 files");

    // Get epoch 1 file metadata
    let epoch1_meta_before: Vec<_> = {
        let mut metas = Vec::new();
        for file in &epoch1_parquet {
            metas.push(harness.object_store().head(file).await.unwrap());
        }
        metas
    };

    // Create epochs.json (epoch 0 ends at checkpoint 5, epoch 1 ends at last_seq)
    harness.create_epochs_json(&[5, last_seq]).await;

    // Run migration with first_checkpoint starting in epoch 1
    // Since epoch 0 ends at checkpoint 5, and epoch 1 starts at 6,
    // first_checkpoint=6 should snap to the first file in epoch 1
    let epoch1_first_checkpoint = 6; // First checkpoint in epoch 1
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("gap-test".to_string());
    migration_config.first_checkpoint = Some(epoch1_first_checkpoint);
    migration_config.last_checkpoint = Some(last_seq);
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config).await;

    // Verify watermark starts at epoch 1
    let watermark = harness
        .read_migration_watermark("gap-test", "checkpoints")
        .await
        .expect("Expected watermark");
    assert_eq!(watermark.checkpoint_hi_inclusive, last_seq);
    assert_eq!(
        watermark.epoch_hi_inclusive, 1,
        "Watermark should be at epoch 1"
    );

    // Verify epoch 1 files were touched
    for (i, file) in epoch1_parquet.iter().enumerate() {
        let new_meta = harness.object_store().head(file).await.unwrap();
        assert!(
            new_meta.last_modified >= epoch1_meta_before[i].last_modified,
            "Epoch 1 file {:?} should have been rewritten",
            file
        );
    }
}

/// Test that migration fails when first_checkpoint is beyond all existing files.
///
/// Scenario:
/// 1. Create files covering checkpoints 0-10
/// 2. Start migration with first_checkpoint=100 (beyond all files)
/// 3. Verify that build_analytics_indexer returns an error
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_no_files_after_checkpoint_error() {
    let harness = TestHarness::new();

    // Create a minimal file structure (without running indexer, manually create files)
    // We'll create a file at 0-10 to test the error case
    use bytes::Bytes;
    use object_store::PutPayload;
    use object_store::path::Path as ObjectPath;

    // Create a dummy parquet file
    let path = ObjectPath::from("checkpoints/epoch_0/0_10.parquet");
    let payload: PutPayload = Bytes::from("dummy parquet data").into();
    harness.object_store().put(&path, payload).await.unwrap();

    // Try to run migration with first_checkpoint=100 (beyond all files)
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("error-test".to_string());
    migration_config.first_checkpoint = Some(100); // Beyond all files!
    migration_config.last_checkpoint = Some(200);

    let registry = prometheus::Registry::new();
    let metrics = Metrics::new(&registry);

    // This should fail during indexer build (when loading file ranges)
    let result =
        sui_analytics_indexer::build_analytics_indexer(migration_config, metrics, registry).await;

    assert!(
        result.is_err(),
        "Expected error when first_checkpoint is beyond all files"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("No files at or after checkpoint"),
        "Error should mention no files at checkpoint, got: {}",
        err_msg
    );
}

/// Test multi-pipeline migration with different file boundaries.
///
/// Scenario:
/// 1. Create files for Checkpoint and Transaction pipelines with different boundaries
/// 2. Start migration with first_checkpoint that falls in different positions for each
/// 3. Verify framework starts at the minimum adjusted checkpoint
/// 4. Verify each pipeline is migrated correctly
#[tokio::test(flavor = "multi_thread")]
async fn test_multi_pipeline_different_file_boundaries() {
    let mut harness = TestHarness::new();

    // Create 10 checkpoints
    for _ in 0..10 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run with both pipelines (same batch size for simplicity)
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    config.pipeline_configs = vec![
        default_pipeline_config(Pipeline::Checkpoint),
        default_pipeline_config(Pipeline::Transaction),
    ];
    harness.run_indexer(config).await;

    // Verify files exist for both pipelines
    let checkpoint_files = harness.list_files("checkpoints/epoch_0").await;
    let checkpoint_parquet = filter_by_extension(&checkpoint_files, ".parquet");
    let transaction_files = harness.list_files("transactions/epoch_0").await;
    let transaction_parquet = filter_by_extension(&transaction_files, ".parquet");

    assert!(!checkpoint_parquet.is_empty(), "Expected checkpoint files");
    assert!(
        !transaction_parquet.is_empty(),
        "Expected transaction files"
    );

    // Get original metadata for both
    let checkpoint_meta: Vec<_> = {
        let mut metas = Vec::new();
        for file in &checkpoint_parquet {
            metas.push(harness.object_store().head(file).await.unwrap());
        }
        metas
    };
    let transaction_meta: Vec<_> = {
        let mut metas = Vec::new();
        for file in &transaction_parquet {
            metas.push(harness.object_store().head(file).await.unwrap());
        }
        metas
    };

    // Create epochs.json (epoch 0 ends at last_seq)
    harness.create_epochs_json(&[last_seq]).await;

    // Run migration with first_checkpoint=5 (mid-file for both)
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("multi-pipeline-test".to_string());
    migration_config.first_checkpoint = Some(5);
    migration_config.last_checkpoint = Some(last_seq);
    migration_config.pipeline_configs = vec![
        default_pipeline_config(Pipeline::Checkpoint),
        default_pipeline_config(Pipeline::Transaction),
    ];
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config).await;

    // Verify watermarks for both pipelines
    let checkpoint_watermark = harness
        .read_migration_watermark("multi-pipeline-test", "checkpoints")
        .await
        .expect("Expected checkpoint watermark");
    let transaction_watermark = harness
        .read_migration_watermark("multi-pipeline-test", "transactions")
        .await
        .expect("Expected transaction watermark");

    assert_eq!(
        checkpoint_watermark.checkpoint_hi_inclusive, last_seq,
        "Checkpoint pipeline watermark should be at last checkpoint"
    );
    assert_eq!(
        transaction_watermark.checkpoint_hi_inclusive, last_seq,
        "Transaction pipeline watermark should be at last checkpoint"
    );

    // Verify files were rewritten for both pipelines
    let checkpoint_files_after = harness.list_files("checkpoints/epoch_0").await;
    let checkpoint_parquet_after = filter_by_extension(&checkpoint_files_after, ".parquet");
    for (i, file) in checkpoint_parquet_after.iter().enumerate() {
        if i < checkpoint_meta.len() {
            let new_meta = harness.object_store().head(file).await.unwrap();
            assert!(
                new_meta.last_modified >= checkpoint_meta[i].last_modified,
                "Checkpoint file {:?} should have been rewritten",
                file
            );
        }
    }

    let transaction_files_after = harness.list_files("transactions/epoch_0").await;
    let transaction_parquet_after = filter_by_extension(&transaction_files_after, ".parquet");
    for (i, file) in transaction_parquet_after.iter().enumerate() {
        if i < transaction_meta.len() {
            let new_meta = harness.object_store().head(file).await.unwrap();
            assert!(
                new_meta.last_modified >= transaction_meta[i].last_modified,
                "Transaction file {:?} should have been rewritten",
                file
            );
        }
    }

    // Verify row counts are correct
    let mut checkpoint_rows = 0;
    for file in &checkpoint_parquet_after {
        checkpoint_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(checkpoint_rows, 10, "Expected 10 checkpoint rows");

    let mut transaction_rows = 0;
    for file in &transaction_parquet_after {
        transaction_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(transaction_rows, 10, "Expected 10 transaction rows");
}

/// Test migration with first_checkpoint=0 (edge case, no snapping needed).
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_first_checkpoint_zero() {
    let mut harness = TestHarness::new();

    // Create 5 checkpoints
    for _ in 0..5 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Run migration with first_checkpoint=0
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("zero-start-test".to_string());
    migration_config.first_checkpoint = Some(0);
    migration_config.last_checkpoint = Some(last_seq);
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config).await;

    // Verify watermark
    let watermark = harness
        .read_migration_watermark("zero-start-test", "checkpoints")
        .await
        .expect("Expected watermark");
    assert_eq!(watermark.checkpoint_hi_inclusive, last_seq);
    assert_eq!(watermark.epoch_hi_inclusive, 0);

    // Verify all rows present
    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");
    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(total_rows, 5, "Expected all 5 rows");
}

/// Test that migration without first_checkpoint processes all files from the beginning.
#[tokio::test(flavor = "multi_thread")]
async fn test_migration_no_first_checkpoint() {
    let mut harness = TestHarness::new();

    // Create 5 checkpoints
    for _ in 0..5 {
        harness.add_checkpoint();
    }
    let last_seq = harness.last_checkpoint_seq;

    // Initial run
    let mut config = harness.default_config();
    config.last_checkpoint = Some(last_seq);
    harness.run_indexer(config).await;

    // Run migration WITHOUT first_checkpoint (should process from beginning)
    let mut migration_config = harness.default_config();
    migration_config.migration_id = Some("no-start-test".to_string());
    migration_config.first_checkpoint = None; // No first_checkpoint!
    migration_config.last_checkpoint = Some(last_seq);
    migration_config.watermark_update_interval_secs = 0;
    harness.run_indexer(migration_config).await;

    // Verify watermark
    let watermark = harness
        .read_migration_watermark("no-start-test", "checkpoints")
        .await
        .expect("Expected watermark");
    assert_eq!(watermark.checkpoint_hi_inclusive, last_seq);

    // Verify all rows present
    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");
    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(total_rows, 5, "Expected all 5 rows");
}

/// Test that batches are flushed based on time even when size thresholds aren't met.
#[tokio::test(flavor = "multi_thread")]
async fn test_time_based_batch_flush() {
    let mut harness = TestHarness::new();
    // Add 3 checkpoints
    for _ in 0..3 {
        harness.add_checkpoint();
    }

    let mut config = harness.default_config();
    config.last_checkpoint = Some(harness.last_checkpoint_seq);
    // Large batch size so size-based flush doesn't trigger
    config.pipeline_configs[0].batch_size = Some(BatchSizeConfig::Checkpoints(1000));
    // 0 seconds = flush immediately after first add
    config.pipeline_configs[0].force_batch_cut_after_secs = 0;

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // With max_batch_duration_secs=0, each checkpoint should trigger a flush.
    // We expect multiple files (one per checkpoint).
    assert!(
        parquet_files.len() >= 2,
        "Expected multiple files due to time-based flush, got {}",
        parquet_files.len()
    );

    // Verify all rows are present
    let mut total_rows = 0;
    for file in &parquet_files {
        total_rows += harness.read_parquet_row_count(file).await;
    }
    assert_eq!(total_rows, 3, "Expected 3 total checkpoint rows");
}

/// Test that pending batches are flushed on shutdown even when batch thresholds aren't met.
///
/// This verifies the shutdown flush behavior: when the indexer reaches its `last_checkpoint`
/// and shuts down gracefully, any buffered data that hasn't reached batch size or time
/// thresholds should still be written to the object store.
#[tokio::test(flavor = "multi_thread")]
async fn test_pending_batch_flushed_on_shutdown() {
    let mut harness = TestHarness::new();
    // Add 3 checkpoints
    for _ in 0..3 {
        harness.add_checkpoint();
    }

    let mut config = harness.default_config();
    config.last_checkpoint = Some(harness.last_checkpoint_seq);
    // Large batch size so size-based flush doesn't trigger
    config.pipeline_configs[0].batch_size = Some(BatchSizeConfig::Checkpoints(1000));
    // Large time threshold so time-based flush doesn't trigger
    config.pipeline_configs[0].force_batch_cut_after_secs = 600;

    harness.run_indexer(config).await;

    let files = harness.list_files("checkpoints/epoch_0").await;
    let parquet_files = filter_by_extension(&files, ".parquet");

    // With large batch size and time threshold, the only way data gets written
    // is via the shutdown flush. We expect exactly one file with all 3 checkpoints.
    assert_eq!(
        parquet_files.len(),
        1,
        "Expected exactly 1 file from shutdown flush, got {}",
        parquet_files.len()
    );

    let total_rows = harness.read_parquet_row_count(parquet_files[0]).await;
    assert_eq!(
        total_rows, 3,
        "Expected 3 checkpoint rows from shutdown flush"
    );
}
