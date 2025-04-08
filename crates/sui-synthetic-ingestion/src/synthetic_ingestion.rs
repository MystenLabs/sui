// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use simulacrum::Simulacrum;
use std::collections::BTreeMap;
use std::path::PathBuf;
use sui_storage::blob::Blob;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::gas_coin::MIST_PER_SUI;
use sui_types::utils::to_sender_signed_transaction;
use tokio::fs;
use tracing::info;

#[derive(clap::Parser, Debug, Clone)]
pub struct Config {
    /// Directory to write the ingestion data to.
    #[clap(long)]
    pub ingestion_dir: PathBuf,
    /// Customize the first checkpoint sequence number in the workload.
    /// This is useful if we want to generate workload to benchmark a non-empty database.
    #[clap(long, default_value_t = 0)]
    pub starting_checkpoint: u64,
    /// Total number of synthetic checkpoints to generate.
    #[clap(long, default_value_t = 2000)]
    pub num_checkpoints: u64,
    /// Number of transactions in a checkpoint.
    #[clap(long, default_value_t = 200)]
    pub checkpoint_size: u64,
}

// TODO: Simulacrum does serial execution which could be slow if
// we need to generate a large number of transactions.
// We may want to make Simulacrum support parallel execution.

pub async fn generate_ingestion(config: Config) {
    info!("Generating synthetic ingestion data. config: {:?}", config);
    let timer = std::time::Instant::now();
    let mut sim = Simulacrum::new();
    let Config {
        ingestion_dir,
        checkpoint_size,
        num_checkpoints,
        starting_checkpoint,
    } = config;
    sim.set_data_ingestion_path(ingestion_dir.clone());
    // Simulacrum will generate 0.chk as the genesis checkpoint.
    // We do not need it and might even override if starting_checkpoint is 0.
    fs::remove_file(ingestion_dir.join("0.chk")).await.unwrap();

    let gas_price = sim.reference_gas_price();
    let (sender, keypair) = get_account_key_pair();
    let mut gas_object = {
        let effects = sim.request_gas(sender, MIST_PER_SUI * 1000000).unwrap();
        // `request_gas` will create a transaction, which we don't want to include in the benchmark.
        // Put it in a checkpoint and then remove the checkpoint file.
        sim.create_checkpoint();
        fs::remove_file(ingestion_dir.join("1.chk")).await.unwrap();
        effects.created()[0].0
    };
    sim.override_next_checkpoint_number(starting_checkpoint);

    let mut tx_count = 0;
    for i in 0..num_checkpoints {
        for _ in 0..checkpoint_size {
            let tx_data = TestTransactionBuilder::new(sender, gas_object, gas_price)
                .transfer_sui(Some(1), sender)
                .build();
            let tx = to_sender_signed_transaction(tx_data, &keypair);
            let (effects, _) = sim.execute_transaction(tx).unwrap();
            gas_object = effects.gas_object().0;
            tx_count += 1;
        }
        let checkpoint = sim.create_checkpoint();
        assert_eq!(checkpoint.sequence_number, i + starting_checkpoint);
        if (i + 1) % 100 == 0 {
            info!("Generated {} checkpoints, {} transactions", i + 1, tx_count);
        }
    }
    info!(
        "Generated {} transactions in {} checkpoints. Total time: {:?}",
        tx_count,
        num_checkpoints,
        timer.elapsed()
    );
}

pub async fn read_ingestion_data(path: &PathBuf) -> anyhow::Result<BTreeMap<u64, CheckpointData>> {
    let mut data = BTreeMap::new();
    let mut dir = fs::read_dir(path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        let bytes = fs::read(path).await?;
        let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes)?;
        data.insert(
            checkpoint_data.checkpoint_summary.sequence_number,
            checkpoint_data,
        );
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use crate::synthetic_ingestion::generate_ingestion;
    use std::path::PathBuf;
    use sui_storage::blob::Blob;
    use sui_types::full_checkpoint_content::CheckpointData;

    #[tokio::test]
    async fn test_ingestion_from_zero() {
        let ingestion_dir = tempfile::tempdir().unwrap().into_path();
        let config = super::Config {
            ingestion_dir: ingestion_dir.clone(),
            starting_checkpoint: 0,
            num_checkpoints: 10,
            checkpoint_size: 2,
        };
        generate_ingestion(config).await;
        check_checkpoint_data(ingestion_dir, 0, 10, 2).await;
    }

    #[tokio::test]
    async fn test_ingestion_from_non_zero() {
        let ingestion_dir = tempfile::tempdir().unwrap().into_path();
        let config = super::Config {
            ingestion_dir: ingestion_dir.clone(),
            starting_checkpoint: 10,
            num_checkpoints: 10,
            checkpoint_size: 2,
        };
        generate_ingestion(config).await;
        check_checkpoint_data(ingestion_dir, 10, 10, 2).await;
    }

    async fn check_checkpoint_data(
        ingestion_dir: PathBuf,
        first_checkpoint: u64,
        num_checkpoints: u64,
        checkpoint_size: u64,
    ) {
        for checkpoint in first_checkpoint..first_checkpoint + num_checkpoints {
            let path = ingestion_dir.join(format!("{}.chk", checkpoint));
            let bytes = tokio::fs::read(&path).await.unwrap();
            let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes).unwrap();

            assert_eq!(
                checkpoint_data.checkpoint_summary.sequence_number,
                checkpoint
            );
            assert_eq!(checkpoint_data.transactions.len(), checkpoint_size as usize);
        }
    }
}
