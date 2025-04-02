// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{
    data_store::DataStore, diff_effects, environment::ReplayEnvironment, epoch_store::EpochStore,
    execution::execute_transaction_to_effects, replay_txn_data::ReplayTransaction, ReplayConfig,
};
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
        DataStore::new(node).unwrap_or_else(|e| panic!("Failed to create data store: {:?}", e));
    let epoch_store = EpochStore::new(&data_store)
        .await
        .unwrap_or_else(|e| panic!("Failed to create epoch store: {:?}", e));

    //
    // create ReplayEnvironment
    let mut env = ReplayEnvironment::new(data_store, epoch_store)
        .await
        .unwrap_or_else(|e| panic!("Failed to create replay environment: {:?}", e));
    debug!("Environment After Creation: {:#?}", env);

    //
    // load transaction data
    let replay_txn = ReplayTransaction::load(&mut env, &tx_digest)
        .await
        .unwrap_or_else(|e: sui_replay_2::errors::ReplayError| {
            panic!("Failed to get transaction data: {:?}", e)
        });
    debug!("Environment After Transaction Load: {:#?}", env);

    //
    // replay transaction
    debug!("Start execute_transaction_to_effects");
    let (result, effects, gas_status) =
        execute_transaction_to_effects(&replay_txn, &env, trace_execution)
            .unwrap_or_else(|e| panic!("Error running a transaction: {:?}", e));
    debug!("End execute_transaction_to_effects");
    debug!("Environment After Execution: {:#?}", env);

    println!("\n** TRANSACTION RESULT -> {:?}", result);
    if show_effects {
        println!("\n** TRANSACTION EFFECTS -> {:?}", effects);
        println!("\n** TRANSACTION GAS STATUS -> {:?}", gas_status);
    }
    if verify {
        if effects != replay_txn.effects {
            println!("\n** FORKING: TRANSACTION EFFECTS DO NOT MATCH");
            println!("{}", diff_effects(&replay_txn.effects, &effects));
        }
    }
}
