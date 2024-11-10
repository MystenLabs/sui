// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{
    data_store::DataStore, 
    environment::ReplayEnvironment, 
    execution, 
    replay_txn_data::ReplayTransaction, 
    ReplayCommand,
};
use tracing::{debug, info};

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = ReplayCommand::parse();
    debug!("Parsed config: {:#?}", config);

    match config {
        ReplayCommand::ReplayTransaction {
            node,
            tx_digest,
            show_effects:_,
            verify:_,
            config_objects:_, 
        } => {
            //
            // create DataStore
            let data_store = DataStore::new(node)
                .unwrap_or_else(|e| panic!("Failed to create data store: {:?}", e));
            
            //
            // create ReplayEnvironment
            let mut env = ReplayEnvironment::new(data_store)
                .await
                .unwrap_or_else(|e| panic!("Failed to create replay environment: {:?}", e));
            debug!("After Creation: {:?}", env);

            //
            // load transaction data
            let replay_txn = ReplayTransaction::load(&mut env, &tx_digest)
                .await
                .unwrap_or_else(|e: sui_replay_2::errors::ReplayError| 
                    panic!("Failed to get transaction data: {:?}", e)
                );
            debug!("After Transaction Load: {:?}", env);

            //
            // replay transaction
            execution::execute_transaction_to_effects(replay_txn, &env)
                .unwrap_or_else(|e| panic!("Error running a transaction {:?}", e));
            debug!("After Execution: {:?}", env);
        }
    }

    info!("DONE: OVER");
}
