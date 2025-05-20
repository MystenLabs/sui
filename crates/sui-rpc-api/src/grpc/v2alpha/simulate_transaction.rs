// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::field_mask::FieldMaskTree;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2alpha::SimulateTransactionRequest;
use crate::proto::rpc::v2alpha::SimulateTransactionResponse;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::Transaction;
use crate::proto::rpc::v2beta::TransactionEffects;
use crate::proto::rpc::v2beta::TransactionEvents;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_types::balance_change::derive_balance_changes;
use sui_types::transaction_executor::SimulateTransactionResult;
use sui_types::transaction_executor::TransactionExecutor;
use tap::Pipe;

pub fn simulate_transaction(
    service: &RpcService,
    request: SimulateTransactionRequest,
) -> Result<SimulateTransactionResponse> {
    let executor = service
        .executor
        .as_ref()
        .ok_or_else(|| RpcError::new(tonic::Code::Unimplemented, "no transaction executor"))?;

    let read_mask = request
        .read_mask
        .map(FieldMaskTree::from)
        .unwrap_or_else(FieldMaskTree::new_wildcard);

    let transaction = request
        .transaction
        .as_ref()
        .ok_or_else(|| FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing))
        .map_err(RpcError::from)?
        .pipe(sui_sdk_types::Transaction::try_from)
        .map_err(|e| {
            FieldViolation::new("transaction")
                .with_description(format!("invalid transaction: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })
        .map_err(RpcError::from)?;

    simulate_transaction_impl(executor, transaction, &read_mask)
}

pub fn simulate_transaction_impl(
    executor: &Arc<dyn TransactionExecutor>,
    transaction: sui_sdk_types::Transaction,
    read_mask: &FieldMaskTree,
) -> Result<SimulateTransactionResponse> {
    if transaction.gas_payment.objects.is_empty() {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "no gas payment provided",
        ));
    }

    let SimulateTransactionResult {
        input_objects,
        output_objects,
        events,
        effects,
        mock_gas_id: _,
    } = executor
        .simulate_transaction(transaction.clone().try_into()?)
        .map_err(anyhow::Error::from)?;

    let transaction = if let Some(submask) = read_mask.subtree("transaction") {
        let mut message = ExecutedTransaction::default();

        let input_objects = input_objects.into_values().collect::<Vec<_>>();
        let output_objects = output_objects.into_values().collect::<Vec<_>>();

        message.balance_changes = read_mask
            .contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
            .then(|| {
                derive_balance_changes(&effects, &input_objects, &output_objects)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            })
            .unwrap_or_default();

        message.effects = {
            let effects = sui_sdk_types::TransactionEffects::try_from(effects)?;
            submask
                .subtree(ExecutedTransaction::EFFECTS_FIELD.name)
                .map(|mask| TransactionEffects::merge_from(&effects, &mask))
        };

        message.events = submask
            .subtree(ExecutedTransaction::EVENTS_FIELD.name)
            .and_then(|mask| {
                events.map(|events| {
                    sui_sdk_types::TransactionEvents::try_from(events)
                        .map(|events| TransactionEvents::merge_from(events, &mask))
                })
            })
            .transpose()?;

        message.transaction = submask
            .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
            .map(|mask| Transaction::merge_from(transaction, &mask));

        Some(message)
    } else {
        None
    };

    let response = SimulateTransactionResponse { transaction };
    Ok(response)
}
