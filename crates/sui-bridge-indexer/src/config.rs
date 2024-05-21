// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::path::PathBuf;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Bridge Indexer",
    about = "Run an indexer for the bridge.\n\
    It uses the data ingestion framework to read sui transactions and listens\n\
    to Ethereum events in order to generate data related to the bridge.\n\
    Data is written to postgres tables and can be used for dashboards and general checks\n\
    on bridge health.",
    rename_all = "kebab-case"
)]
pub struct BridgeIndexerConfig {
    /// URL of the sui remote store.
    #[clap(long, short = 'r', required = true)]
    pub remote_store_url: Option<String>,
    /// URL for Eth fullnode.
    #[clap(long, short = 'e', required = true)]
    pub eth_rpc_url: String,
    /// URL of the DB instance holding indexed bridge data.
    #[clap(long, short = 'd', required = true)]
    pub db_url: String,
    /// Path to the file where the progress store is stored.
    #[clap(
        long,
        short = 'p',
        default_value = "/tmp/progress_store",
        global = true
    )]
    pub progress_store_file: PathBuf,
    /// Path to the directory where the checkpoints are stored.
    #[clap(long, short = 'c', default_value = "/tmp", global = true)]
    pub checkpoints_path: PathBuf,
    /// Number of worker threads to use.
    #[clap(long, short = 't', default_value = "1", global = true)]
    pub concurrency: usize,
    /// Address of the SuiBridge contract
    #[clap(long, required = true)]
    pub eth_sui_bridge_contract_address: String,
    /// Block to start indexing from
    #[clap(long, required = true)]
    pub start_block: u64,
}
