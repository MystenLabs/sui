// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SyntheticIngestionConfig;
use simulacrum::Simulacrum;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::MIST_PER_SUI;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ReadStore;
use sui_types::utils::to_sender_signed_transaction;
use tracing::info;
// TODO: Simulacrum does serial execution which could be slow if
// we need to generate a large number of transactions.
// We may want to make Simulacrum support parallel execution.

pub(crate) struct SyntheticIngestion {
    config: SyntheticIngestionConfig,
    sim: Simulacrum,
}

impl SyntheticIngestion {
    pub fn new(config: SyntheticIngestionConfig) -> Self {
        let sim = Simulacrum::new();
        Self { config, sim }
    }

    pub fn get_last_checkpoint(&self) -> VerifiedCheckpoint {
        self.sim.get_latest_checkpoint().unwrap()
    }

    pub fn run(self) {
        info!(
            "Generating synthetic ingestion data. config: {:?}",
            self.config
        );
        let timer = std::time::Instant::now();
        let Self { mut sim, config } = self;
        let SyntheticIngestionConfig {
            ingestion_dir,
            checkpoint_size,
            num_checkpoints,
        } = config;
        sim.set_data_ingestion_path(ingestion_dir);

        let gas_price = sim.reference_gas_price();
        let (sender, keypair) = get_account_key_pair();
        let effects = sim.request_gas(sender, MIST_PER_SUI * 1000000).unwrap();
        let mut gas_object = effects.created()[0].0;
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
            sim.create_checkpoint();
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
}
