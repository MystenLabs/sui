// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction execution logic for the forked network.

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::ExecutionError;
use sui_types::transaction::InputObjectKind;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use tracing::info;

use crate::context::Context;

/// Result of executing a transaction.
pub struct ExecutionResult {
    pub effects: TransactionEffects,
    pub execution_error: Option<ExecutionError>,
}

/// Execute a transaction and commit it to the store.
pub async fn execute_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<ExecutionResult, anyhow::Error> {
    // Execute the transaction
    let simulacrum = &context.simulacrum();
    let (effects, execution_error) = {
        let mut sim = simulacrum.write().await;
        let (effects, execution_error) = sim.execute_transaction_impersonating(tx_data)?;

        if let Some(ref err) = execution_error {
            info!("Transaction execution error: {:?}", err);
        }

        (effects, execution_error)
    };

    info!(
        "Executed transaction with digest: {:?}",
        effects.transaction_digest()
    );

    Ok(ExecutionResult {
        effects,
        execution_error,
    })
}
