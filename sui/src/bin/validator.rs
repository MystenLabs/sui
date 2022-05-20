// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use multiaddr::Multiaddr;
use std::{num::NonZeroUsize, path::PathBuf};
use sui::{
    config::{sui_config_dir, SUI_NETWORK_CONFIG},
    sui_commands::make_server,
};
use sui_config::{builder::ConfigBuilder, PersistedConfig};
use sui_config::{GenesisConfig, ValidatorConfig};
use tracing::{error, info};

const PROM_PORT_ADDR: &str = "0.0.0.0:9184";

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
    #[clap(long, help = "If set, run genesis even if network.conf already exists")]
    pub force_genesis: bool,

    #[clap(long)]
    pub config_path: Option<PathBuf>,

    #[clap(long, help = "Specify host:port to listen on")]
    listen_address: Option<Multiaddr>,
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

    let config_path = sui_config_dir()?.join(SUI_NETWORK_CONFIG);

    let validator_config = match (config_path.exists(), cfg.force_genesis) {
        (true, false) => PersistedConfig::<ValidatorConfig>::read(&config_path)?,

        // If network.conf is missing, or if --force-genesis is true, we run genesis.
        _ => {
            let genesis_conf: GenesisConfig = PersistedConfig::read(&cfg.genesis_config_path)?;
            let network_config = ConfigBuilder::new(sui_config_dir()?)
                .committee_size(NonZeroUsize::new(1).unwrap())
                .initial_accounts_config(genesis_conf)
                .build();
            network_config.into_validator_configs().remove(0)
        }
    };
    let listen_address = cfg
        .listen_address
        .unwrap_or_else(|| validator_config.network_address().to_owned());

    info!(validator =? validator_config.public_key(), public_addr =? validator_config.network_address(),
        "Initializing authority listening on {}", listen_address
    );

    // TODO: Switch from prometheus exporter. See https://github.com/MystenLabs/sui/issues/1907
    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", PROM_PORT_ADDR);
    prometheus_exporter::start(prom_binding).expect("Failed to start Prometheus exporter");

    // Pass in the newtwork parameters of all authorities
    if let Err(e) = make_server(&validator_config)
        .await?
        .spawn_with_bind_address(listen_address)
        .await
        .unwrap()
        .join()
        .await
    {
        error!("Validator server ended with an error: {e}");
    }

    Ok(())
}
