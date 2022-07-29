// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use clap::{Parser, ValueHint};

use sui_config::genesis_config::GenesisConfig;
use sui_test_data::create_test_data;
use test_utils::network::{start_rpc_test_network_with_fullnode, TestNetwork};

/// Start a Sui validator and fullnode for easy testing.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config directory that will be used to store network configuration
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    config: Option<std::path::PathBuf>,

    /// If enabled, a set of test transactions will be executed to seed activity on the validator
    #[clap(long)]
    with_preset_data: bool,

    /// Port to start the RPC server on
    #[clap(long, default_value = "9000")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let network = create_network(&args).await?;

    if args.with_preset_data {
        println!("Populating validator with preset data...");
        create_test_data(&network).await?;
    }

    println!("RPC URL: {}", network.rpc_url);

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        for node in network.network.validators() {
            node.health_check().await?;
        }

        interval.tick().await;
    }
}

async fn create_network(args: &Args) -> Result<TestNetwork, anyhow::Error> {
    let config_dir = args.config.as_deref();

    let network = start_rpc_test_network_with_fullnode(
        Some(GenesisConfig::for_local_testing()),
        1,
        config_dir,
        Some(args.port),
    )
    .await?;

    // Let nodes connect to one another
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(network)
}
