// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;

use std::path::PathBuf;
use std::sync::Arc;
use sui_config::{Config, NodeConfig};
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::epoch::committee_store::CommitteeStore;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
struct Args {
    #[clap(subcommand)]
    command: ForkRecoveryCommand,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum ForkRecoveryCommand {
    /// Wipe all local execution data, mainly all the objects.
    #[clap(name = "wipe-all-objects")]
    Wipe {
        #[clap(long)]
        config_path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();
    match args.command {
        ForkRecoveryCommand::Wipe { config_path } => {
            wipe_all_local_execution_data(&config_path).await
        }
    }
}

async fn wipe_all_local_execution_data(config_path: &PathBuf) -> anyhow::Result<()> {
    let registry = prometheus::Registry::new();
    typed_store::DBMetrics::init(&registry);

    let config = NodeConfig::load(config_path)?;
    let genesis = config.genesis()?;
    let genesis_committee = genesis.committee()?;
    let committee_store = Arc::new(CommitteeStore::new(
        config.db_path().join("epochs"),
        &genesis_committee,
        None,
    ));
    let store_path = config.db_path().join("store");
    AuthorityPerEpochStore::wipe_all_per_epoch_stores(&store_path)?;
    let authority_store = AuthorityStore::open(
        &store_path,
        None,
        genesis,
        &committee_store,
        config.indirect_objects_threshold,
        false,
    )
    .await?;
    authority_store.wipe_local_execution_state(genesis).await?;
    let _ = std::fs::remove_dir_all(config.db_path().join("indexes"));
    let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
    checkpoint_store.wipe_all_local_execution_data()?;
    Ok(())
}
