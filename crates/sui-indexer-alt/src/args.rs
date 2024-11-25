// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

#[cfg(feature = "benchmark")]
use crate::benchmark::BenchmarkConfig;
use crate::db::DbConfig;
use crate::IndexerConfig;
use clap::Subcommand;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub db_config: DbConfig,

    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// Run the indexer.
    Indexer {
        #[command(flatten)]
        indexer: IndexerConfig,

        #[command(flatten)]
        consistency_config: ConsistencyConfig,
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
        config: BenchmarkConfig,
    },
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConsistencyConfig {
    /// How often to check whether write-ahead logs related to the consistent range can be
    /// pruned.
    #[arg(
        long,
        default_value = "300",
        value_name = "SECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_secs),
    )]
    pub consistent_pruning_interval: Duration,

    /// How long to wait before honouring reader low watermarks.
    #[arg(
        long,
        default_value = "120",
        value_name = "SECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_secs),
    )]
    pub pruner_delay: Duration,

    /// Number of checkpoints to delay indexing summary tables for.
    #[clap(long)]
    pub consistent_range: Option<u64>,
}

impl ConsistencyConfig {
    const DEFAULT_CONSISTENT_PRUNING_INTERVAL: &'static str = "300";
    const DEFAULT_PRUNER_DELAY: &'static str = "120";

    pub fn default_consistent_pruning_interval() -> Duration {
        Self::DEFAULT_CONSISTENT_PRUNING_INTERVAL
            .parse()
            .map(Duration::from_secs)
            .unwrap()
    }

    pub fn default_pruner_delay() -> Duration {
        Self::DEFAULT_PRUNER_DELAY
            .parse()
            .map(Duration::from_secs)
            .unwrap()
    }
}
