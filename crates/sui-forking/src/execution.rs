// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared transaction execution logic for both JSON-RPC and gRPC services.

use sui_rpc_api::RpcError;
use sui_types::{
    base_types::ObjectID,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::ExecutionError,
    inner_temporary_store::InnerTemporaryStore,
    transaction::{InputObjectKind, TransactionData, TransactionDataAPI},
};
use tracing::{info, warn};

use crate::context::Context;
use crate::store::ForkingStore;

/// Result of executing a transaction.
pub struct ExecutionResult {
    pub effects: TransactionEffects,
    pub execution_error: Option<ExecutionError>,
}

/// Result of a dry-run (simulation) without committing.
pub struct DryRunResult {
    pub inner_temp_store: InnerTemporaryStore,
    pub effects: TransactionEffects,
    pub mock_gas: Option<sui_types::base_types::ObjectID>,
    pub execution_result: Result<(), ExecutionError>,
}

/// Execute a transaction and commit it to the store.
///
/// This is the core execution logic shared by both JSON-RPC and gRPC services.
pub async fn execute_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<ExecutionResult, RpcError> {
    // Fetch and cache input objects
    {
        let simulacrum = context.simulacrum.clone();
        let mut sim = simulacrum.write().await;
        let data_store = sim.store_mut();
        fetch_input_objects(context, data_store, &tx_data).await?;
    }

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
        execution_error,
    })
}

/// Dry-run a transaction without committing changes.
///
/// This is the core dry-run logic shared by both JSON-RPC and gRPC services.
pub async fn dry_run_transaction(
    context: &Context,
    tx_data: TransactionData,
) -> Result<DryRunResult, RpcError> {
    // Fetch and cache input objects
    {
        let simulacrum = context.simulacrum.clone();
        let mut sim = simulacrum.write().await;
        let data_store = sim.store_mut();
        fetch_input_objects(context, data_store, &tx_data).await?;
    }

    // Perform dry run simulation (read-only)
    let simulacrum = &context.simulacrum;
    let sim = simulacrum.read().await;
    let (inner_temp_store, _sui_gas_status, effects, mock_gas, execution_result) = sim
        .dry_run_transaction(tx_data)
        .map_err(|e| RpcError::new(tonic::Code::Internal, format!("Simulation failed: {e}")))?;

    if let Err(ref err) = execution_result {
        info!("Dry run execution error: {:?}", err);
    }

    Ok(DryRunResult {
        inner_temp_store,
        effects,
        mock_gas,
        execution_result,
    })
}

/// Fetch and cache all input objects for a transaction.
pub async fn fetch_input_objects(
    context: &Context,
    data_store: &ForkingStore,
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

        // crate::rpc::fetch_and_cache_object_from_rpc(data_store, context, &object_id)
        fetch_and_cache_object_from_rpc(data_store, context, &object_id)
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
    data_store: &ForkingStore,
    _context: &Context,
    object_id: &ObjectID,
) -> Result<(), anyhow::Error> {
    let obj = data_store.get_object(object_id);
    obj.ok_or_else(|| anyhow::anyhow!("Object {} not found in store during execution", object_id))?;

    Ok(())
}
