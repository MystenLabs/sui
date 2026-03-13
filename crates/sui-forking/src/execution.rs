// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared transaction execution logic for both JSON-RPC and gRPC services.

use sui_rpc_api::RpcError;
use sui_types::{
    base_types::ObjectID,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::ExecutionError,
    full_checkpoint_content::ObjectSet,
    transaction::{InputObjectKind, TransactionData, TransactionDataAPI},
    transaction_executor::{TransactionChecks, TransactionExecutor},
};
use tracing::{info, warn};

use crate::context::Context;

/// Result of executing a transaction.
pub struct ExecutionResult {
    pub effects: TransactionEffects,
    pub _execution_error: Option<ExecutionError>,
}

/// Result of a dry-run (simulation) without committing.
pub struct DryRunResult {
    pub effects: TransactionEffects,
    pub objects: ObjectSet,
    pub _mock_gas: Option<sui_types::base_types::ObjectID>,
    pub _execution_result: Result<Vec<sui_types::execution::ExecutionResult>, ExecutionError>,
}

/// Execute a transaction and commit it to the store.
///
/// This is the core execution logic shared by both JSON-RPC and gRPC services.
pub async fn execute_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<ExecutionResult, RpcError> {
    // Fetch and cache input objects
    fetch_input_objects(context, &tx_data).await?;

    // Execute the transaction
    let simulacrum = &context.simulacrum;
    let (effects, execution_error, checkpoint_sequence_number) = {
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

        // Create checkpoint
        let checkpoint = sim.create_checkpoint();
        (effects, execution_error, checkpoint.sequence_number)
    };

    if let Err(err) = context
        .publish_checkpoint_by_sequence_number(checkpoint_sequence_number)
        .await
    {
        warn!(
            checkpoint_sequence_number,
            "Failed to publish checkpoint to subscribers after transaction execution: {err}"
        );
    }

    info!(
        "Executed transaction with digest: {:?}",
        effects.transaction_digest()
    );

    Ok(ExecutionResult {
        effects,
        _execution_error: execution_error,
    })
}

/// Simulate a transaction without committing changes.
///
/// This is the core simulation logic shared by both JSON-RPC and gRPC services.
pub async fn simulate_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<DryRunResult, RpcError> {
    // Fetch and cache input objects
    fetch_input_objects(context, &tx_data).await?;

    // Perform simulation (read-only)
    let simulacrum = &context.simulacrum;
    let sim = simulacrum.read().await;
    let simulation = sim
        .simulate_transaction(tx_data, TransactionChecks::Disabled, true)
        .map_err(|e| RpcError::new(tonic::Code::Internal, format!("Simulation failed: {e}")))?;

    let effects = simulation.effects;
    let objects = simulation.objects;
    let mock_gas = simulation.mock_gas_id;
    let execution_result = simulation.execution_result;

    if let Err(ref err) = execution_result {
        info!("Simulation execution error: {:?}", err);
    }

    Ok(DryRunResult {
        effects,
        objects,
        _mock_gas: mock_gas,
        _execution_result: execution_result,
    })
}

/// Fetch and cache all input objects for a transaction.
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

async fn fetch_and_cache_object_from_rpc(
    context: &Context,
    object_id: &ObjectID,
) -> Result<(), anyhow::Error> {
    let simulacrum = context.simulacrum.read().await;
    let data_store = simulacrum.store_typed();
    let obj = data_store.get_object(object_id);
    obj.ok_or_else(|| anyhow::anyhow!("Object {} not found in store during execution", object_id))?;

    Ok(())
}
