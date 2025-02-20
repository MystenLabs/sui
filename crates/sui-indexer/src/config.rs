// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db::ConnectionPoolConfig;
use crate::{backfill::BackfillTaskKind, handlers::pruner::PrunableTable};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf};
use strum::IntoEnumIterator;
use sui_name_service::NameServiceConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use url::Url;

/// The primary purpose of objects_history is to serve consistency query.
/// A short retention is sufficient.
const OBJECTS_HISTORY_EPOCHS_TO_KEEP: u64 = 2;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui indexer",
    about = "An off-fullnode service serving data from Sui protocol"
)]
pub struct IndexerConfig {
    #[clap(long, alias = "db-url")]
    pub database_url: Url,

    #[clap(flatten)]
    pub connection_pool_config: ConnectionPoolConfig,

    #[clap(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Debug, Clone)]
pub struct NameServiceOptions {
    #[arg(default_value_t = NameServiceConfig::default().package_address)]
    #[arg(long = "name-service-package-address")]
    pub package_address: SuiAddress,
    #[arg(default_value_t = NameServiceConfig::default().registry_id)]
    #[arg(long = "name-service-registry-id")]
    pub registry_id: ObjectID,
    #[arg(default_value_t = NameServiceConfig::default().reverse_registry_id)]
    #[arg(long = "name-service-reverse-registry-id")]
    pub reverse_registry_id: ObjectID,
}

impl NameServiceOptions {
    pub fn to_config(&self) -> NameServiceConfig {
        let Self {
            package_address,
            registry_id,
            reverse_registry_id,
        } = self.clone();
        NameServiceConfig {
            package_address,
            registry_id,
            reverse_registry_id,
        }
    }
}

impl Default for NameServiceOptions {
    fn default() -> Self {
        let NameServiceConfig {
            package_address,
            registry_id,
            reverse_registry_id,
        } = NameServiceConfig::default();
        Self {
            package_address,
            registry_id,
            reverse_registry_id,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct JsonRpcConfig {
    #[command(flatten)]
    pub name_service_options: NameServiceOptions,

    #[clap(long, default_value = "0.0.0.0:9000")]
    pub rpc_address: SocketAddr,

    #[clap(long)]
    pub rpc_client_url: String,
}

#[derive(Args, Debug, Default, Clone)]
#[group(required = true, multiple = true)]
pub struct IngestionSources {
    #[arg(long)]
    pub data_ingestion_path: Option<PathBuf>,

    #[arg(long)]
    pub remote_store_url: Option<Url>,

    #[arg(long)]
    pub rpc_client_url: Option<Url>,
}

#[derive(Args, Debug, Clone)]
pub struct IngestionConfig {
    #[clap(flatten)]
    pub sources: IngestionSources,

    #[arg(
        long,
        default_value_t = Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE,
        env = "DOWNLOAD_QUEUE_SIZE",
    )]
    pub checkpoint_download_queue_size: usize,

    /// Start checkpoint to ingest from, this is optional and if not provided, the ingestion will
    /// start from the next checkpoint after the latest committed checkpoint.
    #[arg(long, env = "START_CHECKPOINT")]
    pub start_checkpoint: Option<u64>,

    /// End checkpoint to ingest until, this is optional and if not provided, the ingestion will
    /// continue until u64::MAX.
    #[arg(long, env = "END_CHECKPOINT")]
    pub end_checkpoint: Option<u64>,

    #[arg(
        long,
        default_value_t = Self::DEFAULT_CHECKPOINT_DOWNLOAD_TIMEOUT,
        env = "INGESTION_READER_TIMEOUT_SECS",
    )]
    pub checkpoint_download_timeout: u64,

    /// Limit indexing parallelism on big checkpoints to avoid OOMing by limiting the total size of
    /// the checkpoint download queue.
    #[arg(
        long,
        default_value_t = Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE_BYTES,
        env = "CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT",
    )]
    pub checkpoint_download_queue_size_bytes: usize,

    /// Whether to delete processed checkpoint files from the local directory,
    /// when running Fullnode-colocated indexer.
    #[arg(
        long,
        default_value_t = true,
        default_missing_value = "true",
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        require_equals = false,
    )]
    pub gc_checkpoint_files: bool,
}

impl IngestionConfig {
    const DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE: usize = 200;
    const DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE_BYTES: usize = 20_000_000;
    const DEFAULT_CHECKPOINT_DOWNLOAD_TIMEOUT: u64 = 20;
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            sources: Default::default(),
            start_checkpoint: None,
            end_checkpoint: None,
            checkpoint_download_queue_size: Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE,
            checkpoint_download_timeout: Self::DEFAULT_CHECKPOINT_DOWNLOAD_TIMEOUT,
            checkpoint_download_queue_size_bytes:
                Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE_BYTES,
            gc_checkpoint_files: true,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct BackFillConfig {
    /// Maximum number of concurrent tasks to run.
    #[arg(
        long,
        default_value_t = Self::DEFAULT_MAX_CONCURRENCY,
    )]
    pub max_concurrency: usize,
    /// Number of checkpoints to backfill in a single SQL command.
    #[arg(
        long,
        default_value_t = Self::DEFAULT_CHUNK_SIZE,
    )]
    pub chunk_size: usize,
}

impl BackFillConfig {
    const DEFAULT_MAX_CONCURRENCY: usize = 10;
    const DEFAULT_CHUNK_SIZE: usize = 1000;
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    Indexer {
        #[command(flatten)]
        ingestion_config: IngestionConfig,
        #[command(flatten)]
        snapshot_config: SnapshotLagConfig,
        #[command(flatten)]
        pruning_options: PruningOptions,
        #[command(flatten)]
        upload_options: UploadOptions,
        /// If true, the indexer will run in MVR mode. It will only index data to
        /// `objects_snapshot`, `objects_history`, `packages`, `checkpoints`, and `epochs` to
        /// support MVR queries.
        #[clap(long, default_value_t = false)]
        mvr_mode: bool,
    },
    JsonRpcService(JsonRpcConfig),
    ResetDatabase {
        #[clap(long)]
        force: bool,
        /// If true, only drop all tables but do not run the migrations.
        /// That is, no tables will exist in the DB after the reset.
        #[clap(long, default_value_t = false)]
        skip_migrations: bool,
    },
    /// Run through the migration scripts.
    RunMigrations,
    /// Backfill DB tables for some ID range [\start, \end].
    /// The tool will automatically slice it into smaller ranges and for each range,
    /// it first makes a read query to the DB to get data needed for backfil if needed,
    /// which then can be processed and written back to the DB.
    /// To add a new backfill, add a new module and implement the `BackfillTask` trait.
    /// full_objects_history.rs provides an example to do SQL-only backfills.
    /// system_state_summary_json.rs provides an example to do SQL + processing backfills.
    RunBackFill {
        /// Start of the range to backfill, inclusive.
        /// It can be a checkpoint number or an epoch or any other identifier that can be used to
        /// slice the backfill range.
        start: usize,
        /// End of the range to backfill, inclusive.
        end: usize,
        #[clap(subcommand)]
        runner_kind: BackfillTaskKind,
        #[command(flatten)]
        backfill_config: BackFillConfig,
    },
    /// Restore the database from formal snaphots.
    Restore(RestoreConfig),
}

#[derive(Args, Default, Debug, Clone)]
pub struct PruningOptions {
    /// Path to TOML file containing configuration for retention policies.
    #[arg(long)]
    pub pruning_config_path: Option<PathBuf>,
}

/// Represents the default retention policy and overrides for prunable tables. Instantiated only if
/// `PruningOptions` is provided on indexer start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Default retention policy for all tables.
    pub epochs_to_keep: u64,
    /// A map of tables to their respective retention policies that will override the default.
    /// Prunable tables not named here will use the default retention policy.
    #[serde(default)]
    pub overrides: HashMap<PrunableTable, u64>,
}

impl PruningOptions {
    /// Load default retention policy and overrides from file.
    pub fn load_from_file(&self) -> Option<RetentionConfig> {
        let config_path = self.pruning_config_path.as_ref()?;

        let contents = std::fs::read_to_string(config_path)
            .expect("Failed to read default retention policy and overrides from file");
        let retention_with_overrides = toml::de::from_str::<RetentionConfig>(&contents)
            .expect("Failed to parse into RetentionConfig struct");

        let default_retention = retention_with_overrides.epochs_to_keep;

        assert!(
            default_retention > 0,
            "Default retention must be greater than 0"
        );
        assert!(
            retention_with_overrides
                .overrides
                .values()
                .all(|&policy| policy > 0),
            "All retention overrides must be greater than 0"
        );

        Some(retention_with_overrides)
    }
}

impl RetentionConfig {
    /// Create a new `RetentionConfig` with the specified default retention and overrides. Call
    /// `finalize()` on the instance to update the `policies` field with the default retention
    /// policy for all tables that do not have an override specified.
    pub fn new(epochs_to_keep: u64, overrides: HashMap<PrunableTable, u64>) -> Self {
        Self {
            epochs_to_keep,
            overrides,
        }
    }

    pub fn new_with_default_retention_only_for_testing(epochs_to_keep: u64) -> Self {
        let mut overrides = HashMap::new();
        overrides.insert(
            PrunableTable::ObjectsHistory,
            OBJECTS_HISTORY_EPOCHS_TO_KEEP,
        );

        Self::new(epochs_to_keep, HashMap::new())
    }

    /// Consumes this struct to produce a full mapping of every prunable table and its retention
    /// policy. By default, every prunable table will have the default retention policy from
    /// `epochs_to_keep`. Some tables like `objects_history` will observe a different default
    /// retention policy. These default values are overridden by any entries in `overrides`.
    pub fn retention_policies(self) -> HashMap<PrunableTable, u64> {
        let RetentionConfig {
            epochs_to_keep,
            mut overrides,
        } = self;

        for table in PrunableTable::iter() {
            let default_retention = match table {
                PrunableTable::ObjectsHistory => OBJECTS_HISTORY_EPOCHS_TO_KEEP,
                _ => epochs_to_keep,
            };

            overrides.entry(table).or_insert(default_retention);
        }

        overrides
    }
}

#[derive(Args, Debug, Clone)]
pub struct SnapshotLagConfig {
    #[arg(
        long = "objects-snapshot-min-checkpoint-lag",
        default_value_t = Self::DEFAULT_MIN_LAG,
        env = "OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG",
    )]
    pub snapshot_min_lag: usize,

    #[arg(
        long = "objects-snapshot-sleep-duration",
        default_value_t = Self::DEFAULT_SLEEP_DURATION_SEC,
    )]
    pub sleep_duration: u64,
}

impl SnapshotLagConfig {
    const DEFAULT_MIN_LAG: usize = 300;
    const DEFAULT_SLEEP_DURATION_SEC: u64 = 5;
}

impl Default for SnapshotLagConfig {
    fn default() -> Self {
        SnapshotLagConfig {
            snapshot_min_lag: Self::DEFAULT_MIN_LAG,
            sleep_duration: Self::DEFAULT_SLEEP_DURATION_SEC,
        }
    }
}

#[derive(Args, Debug, Clone, Default)]
pub struct UploadOptions {
    #[arg(long, env = "GCS_DISPLAY_BUCKET")]
    pub gcs_display_bucket: Option<String>,
    #[arg(long, env = "GCS_CRED_PATH")]
    pub gcs_cred_path: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct RestoreConfig {
    #[arg(long, env = "START_EPOCH", required = true)]
    pub start_epoch: u64,
    #[arg(long, env = "SNAPSHOT_ENDPOINT")]
    pub snapshot_endpoint: String,
    #[arg(long, env = "SNAPSHOT_BUCKET")]
    pub snapshot_bucket: String,
    #[arg(long, env = "SNAPSHOT_DOWNLOAD_DIR", required = true)]
    pub snapshot_download_dir: String,

    #[arg(long, env = "GCS_ARCHIVE_BUCKET")]
    pub gcs_archive_bucket: String,
    #[arg(long, env = "GCS_DISPLAY_BUCKET")]
    pub gcs_display_bucket: String,

    #[arg(env = "OBJECT_STORE_CONCURRENT_LIMIT")]
    pub object_store_concurrent_limit: usize,
    #[arg(env = "OBJECT_STORE_MAX_TIMEOUT_SECS")]
    pub object_store_max_timeout_secs: u64,
}

impl Default for RestoreConfig {
    fn default() -> Self {
        Self {
            start_epoch: 0, // not used b/c it's required
            snapshot_endpoint: "https://formal-snapshot.mainnet.sui.io".to_string(),
            snapshot_bucket: "mysten-mainnet-formal".to_string(),
            snapshot_download_dir: "".to_string(), // not used b/c it's required
            gcs_archive_bucket: "mysten-mainnet-archives".to_string(),
            gcs_display_bucket: "mysten-mainnet-display-table".to_string(),
            object_store_concurrent_limit: 50,
            object_store_max_timeout_secs: 512,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct BenchmarkConfig {
    #[arg(
        long,
        default_value_t = 200,
        help = "Number of transactions in a checkpoint."
    )]
    pub checkpoint_size: u64,
    #[arg(
        long,
        default_value_t = 2000,
        help = "Total number of synthetic checkpoints to generate."
    )]
    pub num_checkpoints: u64,
    #[arg(
        long,
        default_value_t = 1,
        help = "Customize the first checkpoint sequence number to be committed, must be non-zero."
    )]
    pub starting_checkpoint: u64,
    #[arg(
        long,
        default_value_t = false,
        help = "Whether to reset the database before running."
    )]
    pub reset_db: bool,
    #[arg(
        long,
        help = "Path to workload directory. If not provided, a temporary directory will be created.\
        If provided, synthetic workload generator will either load data from it if it exists or generate new data.\
        This avoids repeat generation of the same data."
    )]
    pub workload_dir: Option<PathBuf>,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use tap::Pipe;
    use tempfile::NamedTempFile;

    fn parse_args<'a, T>(args: impl IntoIterator<Item = &'a str>) -> Result<T, clap::error::Error>
    where
        T: clap::Args + clap::FromArgMatches,
    {
        clap::Command::new("test")
            .no_binary_name(true)
            .pipe(T::augment_args)
            .try_get_matches_from(args)
            .and_then(|matches| T::from_arg_matches(&matches))
    }

    #[test]
    fn name_service() {
        parse_args::<NameServiceOptions>(["--name-service-registry-id=0x1"]).unwrap();
        parse_args::<NameServiceOptions>([
            "--name-service-package-address",
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        ])
        .unwrap();
        parse_args::<NameServiceOptions>(["--name-service-reverse-registry-id=0x1"]).unwrap();
        parse_args::<NameServiceOptions>([
            "--name-service-registry-id=0x1",
            "--name-service-package-address",
            "0x0000000000000000000000000000000000000000000000000000000000000002",
            "--name-service-reverse-registry-id=0x3",
        ])
        .unwrap();
        parse_args::<NameServiceOptions>([]).unwrap();
    }

    #[test]
    fn ingestion_sources() {
        parse_args::<IngestionSources>(["--data-ingestion-path=/tmp/foo"]).unwrap();
        parse_args::<IngestionSources>(["--remote-store-url=http://example.com"]).unwrap();
        parse_args::<IngestionSources>(["--rpc-client-url=http://example.com"]).unwrap();

        parse_args::<IngestionSources>([
            "--data-ingestion-path=/tmp/foo",
            "--remote-store-url=http://example.com",
            "--rpc-client-url=http://example.com",
        ])
        .unwrap();

        // At least one must be present
        parse_args::<IngestionSources>([]).unwrap_err();
    }

    #[test]
    fn json_rpc_config() {
        parse_args::<JsonRpcConfig>(["--rpc-client-url=http://example.com"]).unwrap();

        // Can include name service options and bind address
        parse_args::<JsonRpcConfig>([
            "--rpc-address=127.0.0.1:8080",
            "--name-service-registry-id=0x1",
            "--rpc-client-url=http://example.com",
        ])
        .unwrap();

        // fullnode rpc url must be present
        parse_args::<JsonRpcConfig>([]).unwrap_err();
    }

    #[test]
    fn pruning_options_with_objects_history_override() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let toml_content = r#"
        epochs_to_keep = 5

        [overrides]
        objects_history = 10
        transactions = 20
        "#;
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        let temp_path: PathBuf = temp_file.path().to_path_buf();
        let pruning_options = PruningOptions {
            pruning_config_path: Some(temp_path.clone()),
        };
        let retention_config = pruning_options.load_from_file().unwrap();

        // Assert the parsed values
        assert_eq!(retention_config.epochs_to_keep, 5);
        assert_eq!(
            retention_config
                .overrides
                .get(&PrunableTable::ObjectsHistory)
                .copied(),
            Some(10)
        );
        assert_eq!(
            retention_config
                .overrides
                .get(&PrunableTable::Transactions)
                .copied(),
            Some(20)
        );
        assert_eq!(retention_config.overrides.len(), 2);

        let retention_policies = retention_config.retention_policies();

        for table in PrunableTable::iter() {
            let Some(retention) = retention_policies.get(&table).copied() else {
                panic!("Expected a retention policy for table {:?}", table);
            };

            match table {
                PrunableTable::ObjectsHistory => assert_eq!(retention, 10),
                PrunableTable::Transactions => assert_eq!(retention, 20),
                _ => assert_eq!(retention, 5),
            };
        }
    }

    #[test]
    fn pruning_options_no_objects_history_override() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let toml_content = r#"
        epochs_to_keep = 5

        [overrides]
        tx_affected_addresses = 10
        transactions = 20
        "#;
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        let temp_path: PathBuf = temp_file.path().to_path_buf();
        let pruning_options = PruningOptions {
            pruning_config_path: Some(temp_path.clone()),
        };
        let retention_config = pruning_options.load_from_file().unwrap();

        // Assert the parsed values
        assert_eq!(retention_config.epochs_to_keep, 5);
        assert_eq!(
            retention_config
                .overrides
                .get(&PrunableTable::TxAffectedAddresses)
                .copied(),
            Some(10)
        );
        assert_eq!(
            retention_config
                .overrides
                .get(&PrunableTable::Transactions)
                .copied(),
            Some(20)
        );
        assert_eq!(retention_config.overrides.len(), 2);

        let retention_policies = retention_config.retention_policies();

        for table in PrunableTable::iter() {
            let Some(retention) = retention_policies.get(&table).copied() else {
                panic!("Expected a retention policy for table {:?}", table);
            };

            match table {
                PrunableTable::ObjectsHistory => {
                    assert_eq!(retention, OBJECTS_HISTORY_EPOCHS_TO_KEEP)
                }
                PrunableTable::TxAffectedAddresses => assert_eq!(retention, 10),
                PrunableTable::Transactions => assert_eq!(retention, 20),
                _ => assert_eq!(retention, 5),
            };
        }
    }

    #[test]
    fn test_invalid_pruning_config_file() {
        let toml_str = r#"
        epochs_to_keep = 5

        [overrides]
        objects_history = 10
        transactions = 20
        invalid_table = 30
        "#;

        let result = toml::from_str::<RetentionConfig>(toml_str);
        assert!(result.is_err(), "Expected an error, but parsing succeeded");

        if let Err(e) = result {
            assert!(
                e.to_string().contains("unknown variant `invalid_table`"),
                "Error message doesn't mention the invalid table"
            );
        }
    }
}
