// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

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

        /// How often to check whether write-ahead logs related to the consistent range can be
        /// pruned.
        #[arg(
            long,
            default_value = "300",
            value_name = "SECONDS",
            value_parser = |s: &str| s.parse().map(Duration::from_secs),
        )]
        consistent_pruning_interval: Duration,

        /// Number of checkpoints to delay indexing summary tables for.
        #[clap(long)]
        consistent_range: Option<u64>,
    },

    /// Wipe the database of its contents
    ResetDatabase {
        /// If true, only drop all tables but do not run the migrations.
        /// That is, no tables will exist in the DB after the reset.
        #[clap(long, default_value_t = false)]
        skip_migrations: bool,
    },
}
