// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark::TpsLogger;
use rayon::prelude::*;
use simulacrum::Simulacrum;
use std::path::PathBuf;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::MIST_PER_SUI;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::utils::to_sender_signed_transaction;
use tokio::sync::oneshot;
use tracing::info;

/// 1 is fairly sufficient for now. It can sustain about 6000-8000 TPS.
/// We can tune it up if we really need more.
/// TODO: If we need to further scale up, we need to make Simulacrum support parallel execution.
const NUM_GEN_TASKS: usize = 1;

pub(crate) async fn run_synthetic_ingestion(
    ingestion_dir: PathBuf,
    checkpoint_size: usize,
    num_checkpoints: usize,
    warmup_tx_count: u64,
    warmup_finish_sender: oneshot::Sender<()>,
) {
    info!(
        "Generating synthetic ingestion data, warmup tx count: {}",
        warmup_tx_count
    );
    let mut warmup_finish_sender = Some(warmup_finish_sender);
    let mut sim = Simulacrum::new();
    sim.set_data_ingestion_path(ingestion_dir);

    let gas_price = sim.reference_gas_price();
    let (sender, keypair) = get_account_key_pair();
    let mut gas_objects: Vec<_> = (0..NUM_GEN_TASKS)
        .map(|_| {
            let effects = sim.request_gas(sender, MIST_PER_SUI * 1000000).unwrap();
            effects.created()[0].0
        })
        .collect();
    let mut logger = TpsLogger::new("TxGenerator", 100000);
    let mut tx_count = 0;
    for checkpoint in 0..num_checkpoints {
        let transactions = gas_objects
            .into_par_iter()
            .map(|gas_object| {
                (0..checkpoint_size / NUM_GEN_TASKS)
                    .map(|_| {
                        let tx_data = TestTransactionBuilder::new(sender, gas_object, gas_price)
                            .transfer_sui(Some(1), sender)
                            .build();
                        to_sender_signed_transaction(tx_data, &keypair)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        gas_objects = transactions
            .into_iter()
            .map(|transactions| {
                let mut gas_object = None;
                for tx in transactions {
                    let (effects, _) = sim.execute_transaction(tx).unwrap();
                    gas_object = Some(effects.gas_object().0);
                    tx_count += 1;
                }
                gas_object.unwrap()
            })
            .collect();
        sim.create_checkpoint();
        if tx_count >= warmup_tx_count && warmup_finish_sender.is_some() {
            let sender = warmup_finish_sender.take().unwrap();
            sender.send(()).unwrap();
        }
        logger.log(tx_count, checkpoint as CheckpointSequenceNumber);
    }
}
