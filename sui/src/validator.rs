// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use std::path::PathBuf;
use sui::{
    config::{GenesisConfig, PersistedConfig},
    sui_commands::{genesis, make_server},
};
use sui_types::base_types::{decode_bytes_hex, SuiAddress};
use sui_types::committee::Committee;
use tracing::{error, info};

#[derive(Parser)]
#[clap(
    name = "Sui Validator",
    about = "Validator for Sui Network",
    rename_all = "kebab-case"
)]
struct ValidatorOpt {
    /// The genesis config file location
    #[clap(long)]
    pub genesis_config_path: PathBuf,
    /// Public key/address of the validator to start
    #[clap(long, parse(try_from_str = decode_bytes_hex))]
    address: SuiAddress,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "sui".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    };
    #[allow(unused)]
    let guard = telemetry_subscribers::init(config);

    let cfg = ValidatorOpt::parse();
    let genesis_conf: GenesisConfig = PersistedConfig::read(&cfg.genesis_config_path)?;
    let address = cfg.address;

    let (network_config, _, _) = genesis(genesis_conf).await?;

    // Find the network config for this validator
    let net_cfg = network_config
        .authorities
        .iter()
        .find(|x| SuiAddress::from(x.key_pair.public_key_bytes()) == address)
        .ok_or_else(|| {
            anyhow!(
                "Network configs must include config for address {}",
                address
            )
        })?;

    info!(
        "Started {} authority on {}:{}",
        address, net_cfg.host, net_cfg.port
    );

    if let Err(e) = make_server(
        net_cfg,
        &Committee::from(&network_config),
        network_config.buffer_size,
    )
    .await
    .unwrap()
    .spawn()
    .await
    .unwrap()
    .join()
    .await
    {
        error!("Validator server ended with an error: {e}");
    }

    Ok(())
}
