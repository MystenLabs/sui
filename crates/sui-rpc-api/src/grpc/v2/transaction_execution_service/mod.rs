// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionService;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::TransactionEffects;
use sui_rpc::proto::sui::rpc::v2::TransactionEvents;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_sdk_types::Address;
use sui_types::balance_change::derive_balance_changes;
use sui_types::transaction_executor::TransactionExecutor;
use tap::Pipe;

mod simulate;

#[tonic::async_trait]
impl TransactionExecutionService for RpcService {
    async fn execute_transaction(
        &self,
        request: tonic::Request<ExecuteTransactionRequest>,
    ) -> Result<tonic::Response<ExecuteTransactionResponse>, tonic::Status> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| tonic::Status::unimplemented("no transaction executor"))?;

        execute_transaction(self, executor, request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn simulate_transaction(
        &self,
        request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        simulate::simulate_transaction(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

pub const EXECUTE_TRANSACTION_READ_MASK_DEFAULT: &str = "effects";

#[tracing::instrument(skip(service, executor))]
pub async fn execute_transaction(
    service: &RpcService,
    executor: &std::sync::Arc<dyn TransactionExecutor>,
    request: ExecuteTransactionRequest,
) -> Result<ExecuteTransactionResponse, RpcError> {
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

    let signed_transaction = sui_sdk_types::SignedTransaction {
        transaction: transaction.clone(),
        signatures: signatures.clone(),
    };

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

    let request = {
        let mask = read_mask
            .subtree(ExecuteTransactionResponse::TRANSACTION_FIELD.name)
            .unwrap_or_default();

        sui_types::quorum_driver_types::ExecuteTransactionRequestV3 {
            transaction: signed_transaction.try_into()?,
            include_events: mask.contains(ExecutedTransaction::EVENTS_FIELD.name),
            include_input_objects: mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
                || mask.contains(ExecutedTransaction::INPUT_OBJECTS_FIELD.name)
                || mask.contains(ExecutedTransaction::EFFECTS_FIELD.name),
            include_output_objects: mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
                || mask.contains(ExecutedTransaction::OUTPUT_OBJECTS_FIELD.name)
                || mask.contains(ExecutedTransaction::EFFECTS_FIELD.name),
            include_auxiliary_data: false,
        }
    };

    let sui_types::quorum_driver_types::ExecuteTransactionResponseV3 {
        effects:
            sui_types::quorum_driver_types::FinalizedEffects {
                effects,
                finality_info: _,
            },
        events,
        input_objects,
        output_objects,
        auxiliary_data: _,
    } = executor.execute_transaction(request, None).await?;

    let executed_transaction = if let Some(mask) =
        read_mask.subtree(ExecuteTransactionResponse::TRANSACTION_FIELD.name)
    {
        let events = mask
            .subtree(ExecutedTransaction::EVENTS_FIELD)
            .and_then(|mask| events.map(|e| TransactionEvents::merge_from(e, &mask)));

        let input_objects = input_objects.unwrap_or_default();
        let output_objects = output_objects.unwrap_or_default();

        let balance_changes = mask
            .contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
            .then(|| {
                derive_balance_changes(&effects, &input_objects, &output_objects)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            })
            .unwrap_or_default();

        let input_objects = input_objects
            .into_iter()
            .map(sui_sdk_types::Object::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let output_objects = output_objects
            .into_iter()
            .map(sui_sdk_types::Object::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let effects = sui_sdk_types::TransactionEffects::try_from(effects)?;
        let effects = mask
            .subtree(ExecutedTransaction::EFFECTS_FIELD.name)
            .map(|mask| {
                let mut effects = TransactionEffects::merge_from(&effects, &mask);

                if mask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD.name) {
                    for changed_object in effects.changed_objects.iter_mut() {
                        let Ok(object_id) = changed_object.object_id().parse::<Address>() else {
                            continue;
                        };

                        if let Some(object) = input_objects
                            .iter()
                            .chain(&output_objects)
                            .find(|o| o.object_id() == object_id)
                        {
                            changed_object.object_type = Some(match object.object_type() {
                                sui_sdk_types::ObjectType::Package => "package".to_owned(),
                                sui_sdk_types::ObjectType::Struct(struct_tag) => {
                                    struct_tag.to_string()
                                }
                            });
                        }
                    }
                }

                if mask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name) {
                    for unchanged_consensus_object in effects.unchanged_consensus_objects.iter_mut()
                    {
                        let Ok(object_id) =
                            unchanged_consensus_object.object_id().parse::<Address>()
                        else {
                            continue;
                        };

                        if let Some(object) =
                            input_objects.iter().find(|o| o.object_id() == object_id)
                        {
                            unchanged_consensus_object.object_type =
                                Some(match object.object_type() {
                                    sui_sdk_types::ObjectType::Package => "package".to_owned(),
                                    sui_sdk_types::ObjectType::Struct(struct_tag) => {
                                        struct_tag.to_string()
                                    }
                                });
                        }
                    }
                }

                // Try to render clever error info
                super::ledger_service::render_clever_error(service, &mut effects);

                effects
            });

        let mut message = ExecutedTransaction::default();
        message.digest = mask
            .contains(ExecutedTransaction::DIGEST_FIELD.name)
            .then(|| transaction.digest().to_string());
        message.transaction = mask
            .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
            .map(|mask| Transaction::merge_from(transaction, &mask));
        message.signatures = mask
            .subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
            .map(|mask| {
                signatures
                    .into_iter()
                    .map(|s| UserSignature::merge_from(s, &mask))
                    .collect()
            })
            .unwrap_or_default();
        message.effects = effects;
        message.events = events;
        message.balance_changes = balance_changes;
        message.input_objects = mask
            .subtree(ExecutedTransaction::INPUT_OBJECTS_FIELD.name)
            .map(|mask| {
                input_objects
                    .into_iter()
                    .map(|o| Object::merge_from(o, &mask))
                    .collect()
            })
            .unwrap_or_default();
        message.output_objects = mask
            .subtree(ExecutedTransaction::OUTPUT_OBJECTS_FIELD.name)
            .map(|mask| {
                output_objects
                    .into_iter()
                    .map(|o| Object::merge_from(o, &mask))
                    .collect()
            })
            .unwrap_or_default();
        Some(message)
    } else {
        None
    };

    let mut message = ExecuteTransactionResponse::default();
    message.transaction = executed_transaction;
    Ok(message)
}
