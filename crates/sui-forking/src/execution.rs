// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared transaction execution logic for the forking gRPC services.

use sui_rpc_api::RpcError;
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
    pub _execution_error: Option<ExecutionError>,
}

/// Execute a transaction and commit it to the store. This will fetch and store all input objects
/// before execution to ensure the Simulacrum has all necessary data to execute the transaction.
pub async fn execute_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<ExecutionResult, RpcError> {
    // Fetch and cache input objects
    fetch_input_objects(context, &tx_data).await?;

    // Execute the transaction
    let simulacrum = &context.simulacrum();
    let (effects, execution_error) = {
        let mut sim = simulacrum.write().await;
        let (effects, execution_error) =
            sim.execute_transaction_impersonating(tx_data)
                .map_err(|e| {
                    RpcError::new(
                        tonic::Code::Internal,
                        format!("Transaction execution failed: {e}"),
                    )
                })?;

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
        _execution_error: execution_error,
    })
}

/// Fetch and store on disk all missing input objects for a transaction.
pub async fn fetch_input_objects(
    context: &Context,
    tx_data: &TransactionData,
) -> Result<(), RpcError> {
    let input_objs = tx_data.input_objects().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("Failed to get input objects: {e}"),
        )
    })?;

    for input_obj in input_objs {
        let object_id = match input_obj {
            InputObjectKind::MovePackage(object_id) => object_id,
            InputObjectKind::ImmOrOwnedMoveObject(obj_ref) => obj_ref.0,
            InputObjectKind::SharedMoveObject { id, .. } => id,
        };

        fetch_and_cache_object_from_rpc(context, &object_id)
            .await
            .map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to fetch object {}: {e}", object_id),
                )
            })?;
    }

    Ok(())
}

/// Fetch an object from RPC. If it does not exist on disk, it will download it and store it on
/// disk.
async fn fetch_and_cache_object_from_rpc(
    context: &Context,
    object_id: &ObjectID,
) -> Result<(), anyhow::Error> {
    let simulacrum = context.simulacrum().read().await;
    let data_store = simulacrum.store();
    let obj = data_store.get_object(object_id).ok().flatten();
    obj.ok_or_else(|| anyhow::anyhow!("Object {} not found in store during execution", object_id))?;

    Ok(())
}
