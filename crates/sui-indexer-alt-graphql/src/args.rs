// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_metrics::MetricsArgs;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use tonic::transport::Uri;
use url::Url;

use crate::RpcArgs;

/// Arguments for configuring KV store access (either Bigtable or Ledger gRPC).
///
/// These options are mutually exclusive - only one KV store source can be configured at a time.
#[derive(clap::Args, Debug, Clone, Default)]
#[group(required = false)]
pub struct KvArgs {
    /// Bigtable instance ID to make KV store requests to.
    #[arg(long, group = "kv_source")]
    pub bigtable_instance: Option<String>,

    /// App profile ID to use for Bigtable client. If not provided, the default profile will be used.
    #[arg(long)]
    pub bigtable_app_profile_id: Option<String>,

    /// gRPC endpoint URL for the ledger service (e.g., archive.mainnet.sui.io)
    #[arg(long, group = "kv_source")]
    pub ledger_grpc_url: Option<Uri>,

    /// Time spent waiting for a request to the kv store to complete, in milliseconds.
    #[arg(long)]
    pub kv_statement_timeout_ms: Option<u64>,
}

impl KvArgs {
    /// Extract BigtableArgs from KvArgs
    pub fn bigtable_args(&self) -> BigtableArgs {
        BigtableArgs {
            bigtable_statement_timeout_ms: self.kv_statement_timeout_ms,
            bigtable_app_profile_id: self.bigtable_app_profile_id.clone(),
        }
    }

    /// Extract LedgerGrpcArgs from KvArgs
    pub fn ledger_grpc_args(&self) -> LedgerGrpcArgs {
        LedgerGrpcArgs {
            ledger_grpc_statement_timeout_ms: self.kv_statement_timeout_ms,
        }
    }
}

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the RPC service.
    Rpc {
        /// The URL of the database to connect to.
        #[clap(
            long,
            default_value = "postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt"
        )]
        database_url: Url,

        #[command(flatten)]
        fullnode_args: FullnodeArgs,

        #[command(flatten)]
        db_args: DbArgs,

        #[command(flatten)]
        kv_args: KvArgs,

        #[command(flatten)]
        consistent_reader_args: ConsistentReaderArgs,

        #[command(flatten)]
        rpc_args: RpcArgs,

        #[command(flatten)]
        system_package_task_args: SystemPackageTaskArgs,

        #[command(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the RPC's configuration TOML file. If one is not provided, the default values for
        /// the configuration will be set.
        #[arg(long)]
        config: Option<PathBuf>,

        /// Path to indexer configuration TOML files (multiple can be supplied). These are used to
        /// identify the pipelines that the RPC will monitor for watermark purposes.
        #[arg(long, action = clap::ArgAction::Append)]
        indexer_config: Vec<PathBuf>,
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,
}
