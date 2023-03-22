// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod load_test;
mod payload;

use anyhow::Result;
use clap::Parser;
use std::error::Error;
use std::time::Duration;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, SuiKeyPair};
use tracing::info;

use crate::load_test::LoadTest;
use crate::payload::{Command, Payload, RpcCommandProcessor};

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
}

#[derive(Parser)]
pub struct CommonOptions {
    #[clap(short, long, default_value_t = 0)]
    pub repeat: usize,

    #[clap(short, long, default_value_t = 0)]
    pub interval_in_ms: u64,
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

        /// inclusive, use `getLatestCheckpointSequenceNumber` if `None`
        #[clap(short, long)]
        end: Option<u64>,

        #[clap(flatten)]
        common: CommonOptions,
    },
    #[clap(name = "pay-sui")]
    PaySui {
        // TODO(chris) customize recipients and amounts
        #[clap(flatten)]
        common: CommonOptions,
    },
}

fn get_keypair() -> Result<(SuiAddress, String)> {
    // TODO(chris) allow pass in custom path for keystore
    // Load keystore from ~/.sui/sui_config/sui.keystore
    let keystore_path = match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    };
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let active_address = keystore.addresses().pop().unwrap();
    let keypair: &SuiKeyPair = keystore.get_key(&active_address)?;
    Ok((active_address, keypair.encode_base64()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tracing_level = "debug";
    let network_tracing_level = "info";
    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level}");

    // Initialize logger
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_log_level(&log_filter)
        .init();
    let opts = Opts::parse();
    info!("Running Load Gen with following urls {:?}", opts.urls);

    // TODO(chris): remove hardcoded value since we only need keystore for write commands
    let need_keystore = true;
    let (signer_address, encoded_keypair) = if need_keystore {
        let (address, keypair) = get_keypair()?;
        info!("Using address {address} from keystore");
        (Some(address), Some(keypair))
    } else {
        (None, None)
    };

    let (command, common) = match opts.command {
        ClapCommand::DryRun { common } => (Command::new_dry_run(), common),
        ClapCommand::PaySui { common } => (Command::new_pay_sui(), common),
        ClapCommand::GetCheckpoints { common, start, end } => {
            (Command::new_get_checkpoints(start, end), common)
        }
    };

    let command = command
        .with_repeat_interval(Duration::from_millis(common.interval_in_ms))
        .with_repeat_n_times(common.repeat);

    let processor = RpcCommandProcessor::new(&opts.urls).await;
    let load_test = LoadTest {
        processor,
        payloads: vec![
            // TODO(chris): use different gas_payment for different threads
            Payload {
                commands: vec![command],
                encoded_keypair,
                signer_address,
                gas_payment: None
            };
            opts.num_threads
        ],
    };
    load_test.run().await?;

    Ok(())
}
