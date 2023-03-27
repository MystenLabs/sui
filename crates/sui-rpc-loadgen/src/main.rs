// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod load_test;
mod payload;

use anyhow::Result;
use clap::Parser;

use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_types::crypto::{EncodeDecodeBase64, SuiKeyPair};
use tracing::info;

use crate::load_test::{LoadTest, LoadTestConfig};
use crate::payload::{Command, RpcCommandProcessor, SignerInfo};

#[derive(Parser)]
#[clap(
    name = "Sui RPC Load Generator",
    version = "0.1",
    about = "A load test application for Sui RPC"
)]
struct Opts {
    // TODO(chris): support running multiple commands at once
    #[clap(subcommand)]
    pub command: ClapCommand,
    #[clap(long, default_value_t = 1)]
    pub num_threads: usize,
    #[clap(long, default_value_t = true)]
    pub cross_validate: bool,
    #[clap(long, multiple_values = true, default_value = "http://127.0.0.1:9000")]
    pub urls: Vec<String>,
    /// the path to log file directory
    #[clap(long, default_value = "~/.sui/sui_config/logs")]
    logs_directory: String,

    #[clap(long, default_value = "~/.sui/loadgen/data")]
    data_directory: String,
}

#[derive(Parser)]
pub struct CommonOptions {
    #[clap(short, long, default_value_t = 0)]
    pub repeat: usize,

    #[clap(short, long, default_value_t = 0)]
    pub interval_in_ms: u64,

    /// different chunks will be executed concurrently on the same thread
    #[clap(long, default_value_t = 1)]
    num_chunks_per_thread: usize,
}

#[derive(Parser)]
pub enum ClapCommand {
    #[clap(name = "dry-run")]
    DryRun {
        #[clap(flatten)]
        common: CommonOptions,
    },
    #[clap(name = "get-checkpoints")]
    GetCheckpoints {
        /// Default to start from checkpoint 0
        #[clap(short, long, default_value_t = 0)]
        start: u64,

        /// inclusive, uses `getLatestCheckpointSequenceNumber` if `None`
        #[clap(short, long)]
        end: Option<u64>,

        #[clap(long, default_value_t = true)]
        verify_transactions: bool,

        #[clap(long, default_value_t = true)]
        verify_objects: bool,

        // Whether to record data from checkpoint
        #[clap(long, default_value_t = true)]
        record: bool,

        #[clap(flatten)]
        common: CommonOptions,
    },
    #[clap(name = "pay-sui")]
    PaySui {
        // TODO(chris) customize recipients and amounts
        #[clap(flatten)]
        common: CommonOptions,
    },
    #[clap(name = "query-transactions")]
    QueryTransactions {
        #[clap(long)]
        address: Option<String>,

        #[clap(short, long)]
        address_type: Option<String>,

        #[clap(long)]
        from_file: Option<bool>,

        #[clap(flatten)]
        common: CommonOptions,
    },
}

fn get_keypair() -> Result<SignerInfo> {
    // TODO(chris) allow pass in custom path for keystore
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = get_sui_config_directory().join("sui.keystore");
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let active_address = keystore.addresses().pop().unwrap();
    let keypair: &SuiKeyPair = keystore.get_key(&active_address)?;
    println!("using address {active_address} for signing");
    Ok(SignerInfo::new(keypair.encode_base64()))
}

fn get_sui_config_directory() -> PathBuf {
    match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config"),
        None => panic!("Cannot obtain home directory path"),
    }
}

pub fn expand_path(dir_path: &str) -> String {
    shellexpand::full(&dir_path)
        .map(|v| v.into_owned())
        .unwrap_or_else(|e| panic!("Failed to expand directory '{:?}': {}", dir_path, e))
}

fn get_log_file_path(dir_path: String) -> String {
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let timestamp = current_time.as_secs();
    // use timestamp to signify which file is newer
    let log_filename = format!("sui-rpc-loadgen.{}.log", timestamp);

    let dir_path = expand_path(&dir_path);
    format!("{dir_path}/{log_filename}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tracing_level = "debug";
    let network_tracing_level = "info";
    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level}");
    let opts = Opts::parse();

    let log_filename = get_log_file_path(opts.logs_directory);

    // Initialize logger
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_log_level(&log_filter)
        .with_log_file(&log_filename)
        .init();

    println!("Logging to {}", &log_filename);
    info!("Running Load Gen with following urls {:?}", opts.urls);

    let (command, common, need_keystore) = match opts.command {
        ClapCommand::DryRun { common } => (Command::new_dry_run(), common, false),
        ClapCommand::PaySui { common } => (Command::new_pay_sui(), common, true),
        ClapCommand::GetCheckpoints {
            common,
            start,
            end,
            verify_transactions,
            verify_objects,
            record,
        } => (
            Command::new_get_checkpoints(start, end, verify_transactions, verify_objects, record),
            common,
            false,
        ),
        ClapCommand::QueryTransactions {
            common,
            address,
            address_type,
            from_file,
        } => (
            Command::new_query_transaction_blocks(address, address_type, from_file),
            common,
            false,
        ),
    };

    let signer_info = need_keystore.then_some(get_keypair()?);

    let command = command
        .with_repeat_interval(Duration::from_millis(common.interval_in_ms))
        .with_repeat_n_times(common.repeat);

    let processor = RpcCommandProcessor::new(&opts.urls, expand_path(&opts.data_directory)).await;

    let load_test = LoadTest {
        processor,
        config: LoadTestConfig {
            command,
            num_threads: opts.num_threads,
            // TODO: pass in from config
            divide_tasks: true,
            signer_info,
            num_chunks_per_thread: common.num_chunks_per_thread,
            max_repeat: common.repeat,
        },
    };
    load_test.run().await?;

    Ok(())
}
