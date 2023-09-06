// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;

pub mod analytics_handler;
pub mod analytics_metrics;
pub mod csv_writer;
pub mod errors;
pub mod tables;
pub mod writer;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Analytics Indexer",
    about = "Indexer service to upload data for the analytics pipeline.",
    rename_all = "kebab-case"
)]
pub struct AnalyticsIndexerConfig {
    /// The url of the checkpoint client to connect to.
    #[clap(long)]
    pub rest_url: String,
    /// The url of the metrics client to connect to.
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
    /// Checkpoint to start from.
    #[clap(long, global = true)]
    pub starting_checkpoint: Option<u64>,
    /// Directory to contain the temporary files for checkpoint entries.
    /// They will be uploded to the datastore.
    /// If not specified, the current directory will be used.
    #[clap(long, global = true)]
    pub checkpoint_dir: Option<String>,
    /// Number of checkpoints to process before uploading to the datastore.
    #[clap(long, default_value = "30", global = true)]
    pub checkpoint_interval: u64,
}
