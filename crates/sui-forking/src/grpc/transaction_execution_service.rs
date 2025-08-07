// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use sui_rpc::field::{FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction,
    SimulateTransactionRequest, SimulateTransactionResponse, Transaction, TransactionEffects,
    TransactionEvents, UserSignature,
    transaction_execution_service_server::TransactionExecutionService,
};
use sui_rpc_api::{ErrorReason, RpcError};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::TransactionData;
use tap::Pipe;

use crate::context::Context;
use crate::execution;

const EXECUTE_TRANSACTION_READ_MASK_DEFAULT: &str = "effects";

/// A TransactionExecutionService implementation backed by the ForkingStore/Simulacrum.
pub struct ForkingTransactionExecutionService {
    context: Context,
}

impl ForkingTransactionExecutionService {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl TransactionExecutionService for ForkingTransactionExecutionService {
    async fn execute_transaction(
        &self,
        request: tonic::Request<ExecuteTransactionRequest>,
    ) -> Result<tonic::Response<ExecuteTransactionResponse>, tonic::Status> {
        execute_transaction_impl(&self.context, request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn simulate_transaction(
        &self,
        request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        simulate_transaction_impl(&self.context, request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

async fn execute_transaction_impl(
    context: &Context,
    request: ExecuteTransactionRequest,
) -> Result<ExecuteTransactionResponse, RpcError> {
    // Parse transaction from proto
    let transaction = request
        .transaction
        .as_ref()
        .ok_or_else(|| FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing))?
        .pipe(sui_sdk_types::Transaction::try_from)
        .map_err(|e| {
            FieldViolation::new("transaction")
                .with_description(format!("invalid transaction: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    // Parse signatures (we don't validate them in forking mode)
    let signatures = request
        .signatures
        .iter()
        .enumerate()
        .map(|(i, signature)| {
            sui_sdk_types::UserSignature::try_from(signature).map_err(|e| {
                FieldViolation::new_at("signatures", i)
                    .with_description(format!("invalid signature: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Validate and parse read_mask
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(EXECUTE_TRANSACTION_READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    // Convert to sui_types::TransactionData
    let tx_data: TransactionData = transaction.clone().try_into().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("Failed to convert transaction: {e}"),
        )
    })?;

    // Execute using shared executor
    let execution::ExecutionResult { effects, .. } =
        execution::execute_transaction(context, tx_data).await?;

    // Build response based on read_mask
    let mut message = ExecutedTransaction::default();

    if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(transaction.digest().to_string());
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
        message.transaction = Some(Transaction::merge_from(transaction.clone(), &submask));
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
        message.signatures = signatures
            .into_iter()
            .map(|s| UserSignature::merge_from(s, &submask))
            .collect();
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        let effects_sdk: sui_sdk_types::TransactionEffects =
            effects.clone().try_into().map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to convert effects: {e}"),
                )
            })?;
        message.effects = Some(TransactionEffects::merge_from(&effects_sdk, &submask));
    }

    // Get events if requested
    if let Some(submask) = read_mask.subtree(ExecutedTransaction::EVENTS_FIELD.name) {
        let sim = context.simulacrum.read().await;
        let store = sim.store_static();
        if let Some(events) =
            sui_types::storage::ReadStore::get_events(&store, effects.transaction_digest())
        {
            let events_sdk: sui_sdk_types::TransactionEvents = events.try_into().map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to convert events: {e}"),
                )
            })?;
            message.events = Some(TransactionEvents::merge_from(events_sdk, &submask));
        }
    }

    Ok(ExecuteTransactionResponse::default().with_transaction(message))
}

async fn simulate_transaction_impl(
    context: &Context,
    request: SimulateTransactionRequest,
) -> Result<SimulateTransactionResponse, RpcError> {
    // Parse read_mask
    let read_mask = request
        .read_mask
        .as_ref()
        .map(FieldMaskTree::from_field_mask)
        .unwrap_or_else(FieldMaskTree::new_wildcard);

    // Parse transaction from proto
    let transaction_proto = request
        .transaction
        .as_ref()
        .ok_or_else(|| FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing))?;

    let transaction = sui_sdk_types::Transaction::try_from(transaction_proto).map_err(|e| {
        FieldViolation::new("transaction")
            .with_description(format!("invalid transaction: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;

    // Convert to sui_types::TransactionData
    let tx_data: TransactionData = transaction.clone().try_into().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("Failed to convert transaction: {e}"),
        )
    })?;

    // Dry run using shared executor
    let execution::DryRunResult { effects, .. } =
        execution::dry_run_transaction(context, tx_data).await?;

    // Build the ExecutedTransaction part of the response
    let executed_transaction = if let Some(submask) = read_mask.subtree("transaction") {
        let mut message = ExecutedTransaction::default();

        if submask.contains(ExecutedTransaction::EFFECTS_FIELD.name) {
            let effects_sdk: sui_sdk_types::TransactionEffects =
                effects.clone().try_into().map_err(|e| {
                    RpcError::new(
                        tonic::Code::Internal,
                        format!("Failed to convert effects: {e}"),
                    )
                })?;
            message.effects = Some(TransactionEffects::merge_from(
                &effects_sdk,
                &submask
                    .subtree(ExecutedTransaction::EFFECTS_FIELD.name)
                    .unwrap_or_else(FieldMaskTree::new_wildcard),
            ));
        }

        if let Some(tx_submask) = submask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
            message.transaction = Some(Transaction::merge_from(transaction, &tx_submask));
        }

        Some(message)
    } else {
        None
    };

    let mut response = SimulateTransactionResponse::default();
    response.transaction = executed_transaction;
    Ok(response)
}
