// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

#[cfg(feature = "benchmark")]
use crate::benchmark::BenchmarkArgs;
use crate::IndexerArgs;
use clap::Subcommand;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_pg_db::DbArgs;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub db_args: DbArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// Run the indexer.
    Indexer {
        #[command(flatten)]
        client_args: ClientArgs,

        #[command(flatten)]
        indexer_args: IndexerArgs,

        #[command(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the indexer's configuration TOML file.
        #[arg(long)]
        config: PathBuf,
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,

    /// Combine the configuration held across multiple files into one and output it to STDOUT. When
    /// two configurations set the same field, the last write wins.
    MergeConfigs {
        /// Path to a TOML file to be merged
        #[arg(long, required = true, action = clap::ArgAction::Append)]
        config: Vec<PathBuf>,
    },

    /// Wipe the database of its contents
    ResetDatabase {
        /// If true, only drop all tables but do not run the migrations.
        /// That is, no tables will exist in the DB after the reset.
        #[clap(long, default_value_t = false)]
        skip_migrations: bool,
    },

    /// Run the benchmark. It will load ingestion data from the given path and run the pipelines.
    /// The first and last checkpoint will be determined automatically based on the ingestion data.
    /// Note that the indexer will not be bootstrapped from genesis, and hence will
    /// skip any pipelines that rely on genesis data.
    #[cfg(feature = "benchmark")]
    Benchmark {
        #[command(flatten)]
        benchmark_args: BenchmarkArgs,

        /// Path to the indexer's configuration TOML file.
        #[arg(long)]
        config: PathBuf,
    },
}
