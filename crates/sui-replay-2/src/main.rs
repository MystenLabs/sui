// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{
    data_store::DataStore, diff_effects, environment::ReplayEnvironment,
    execution::execute_transaction_to_effects, replay_txn::ReplayTransaction, ReplayConfig,
};
use sui_types::{effects::TransactionEffects, gas::SuiGasStatus};
use tracing::debug;

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = ReplayConfig::parse();
    debug!("Parsed config: {:#?}", config);
    let ReplayConfig {
        node,
        tx_digest,
        show_effects,
        verify,
        trace_execution,
    } = config;

    //
    // create DataStore and EpochStore
    let data_store =
        // TODO: use `new` instead of `new_eager` once we have the indexer new schema available
        // DataStore::new(node).unwrap_or_else(|e| panic!("Failed to create data store: {:?}", e));
        DataStore::new_eager(node).await.unwrap_or_else(|e| panic!("Failed to create data store: {:?}", e));

    //
    // create ReplayEnvironment
    let mut env = ReplayEnvironment::new(data_store)
        .await
        .unwrap_or_else(|e| panic!("Failed to create replay environment: {:?}", e));

    //
    // load transaction input
    let replay_txn = ReplayTransaction::load(&mut env, &tx_digest)
        .await
        .unwrap_or_else(|e: sui_replay_2::errors::ReplayError| {
            panic!("Failed to get transaction data: {:?}", e)
        });

    //
    // replay transaction
    debug!("Start execute_transaction_to_effects");
    let (result, effects, gas_status, expected_effects) =
        execute_transaction_to_effects(replay_txn, &env, trace_execution)
            .unwrap_or_else(|e| panic!("Error running a transaction: {:?}", e));
    debug!("End execute_transaction_to_effects");

    println!("\n** TRANSACTION RESULT -> {:?}", result);
    if show_effects {
        print_txn_effects(&effects, &gas_status);
    }
    if verify {
        verify_txn(&expected_effects, &effects);
    }
}

fn print_txn_effects(effects: &TransactionEffects, gas_status: &SuiGasStatus) {
    println!("\n** TRANSACTION EFFECTS -> {:?}", effects);
    println!("\n** TRANSACTION GAS STATUS -> {:?}", gas_status);
}

fn verify_txn(expected_effects: &TransactionEffects, effects: &TransactionEffects) {
    if effects != expected_effects {
        println!("\n** FORKING: TRANSACTION EFFECTS DO NOT MATCH");
        println!("{}", diff_effects(expected_effects, effects));
    }
}
