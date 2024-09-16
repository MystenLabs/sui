// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use url::Url;

use crate::db::ConnectionPoolConfig;

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
            checkpoint_download_queue_size: Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE,
            checkpoint_download_timeout: Self::DEFAULT_CHECKPOINT_DOWNLOAD_TIMEOUT,
            checkpoint_download_queue_size_bytes:
                Self::DEFAULT_CHECKPOINT_DOWNLOAD_QUEUE_SIZE_BYTES,
        }
    }
}

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
        restore_config: RestoreConfig,
    },
    JsonRpcService(JsonRpcConfig),
    ResetDatabase {
        #[clap(long)]
        force: bool,
    },
    /// Run through the migration scripts.
    RunMigrations,
    /// Backfill DB tables for checkpoint range [\first_checkpoint, \last_checkpoint].
    /// by running a SQL query provided in \sql.
    /// The tool will automatically slice it into smaller checkpoint ranges and for each range [start, end],
    /// it augments the \sql query with:
    ///   "WHERE {checkpoint_column_name} BETWEEN {start} AND {end}"
    /// to avoid running out of memory.
    /// Example:
    ///  ./sui-indexer --database-url <...> sql-back-fill
    ///   "INSERT INTO full_objects_history (object_id, object_version, serialized_object) SELECT object_id, object_version, serialized_object FROM objects_history"
    ///   "checkpoint_sequence_number" 0 100000
    SqlBackFill {
        sql: String,
        checkpoint_column_name: String,
        first_checkpoint: u64,
        last_checkpoint: u64,
    },
}

#[derive(Args, Default, Debug, Clone)]
pub struct PruningOptions {
    #[arg(long, env = "EPOCHS_TO_KEEP")]
    pub epochs_to_keep: Option<u64>,
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
pub struct RestoreConfig {
    #[arg(long, env = "GCS_CRED_PATH")]
    pub gcs_cred_path: Option<String>,
    #[arg(long, env = "GCS_DISPLAY_BUCKET")]
    pub gcs_display_bucket: Option<String>,
}

#[cfg(test)]
mod test {
    use super::*;
    use tap::Pipe;

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
}
