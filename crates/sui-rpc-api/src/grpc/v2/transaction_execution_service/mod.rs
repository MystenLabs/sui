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
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::ObjectSet;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionService;
use sui_types::balance_change::derive_balance_changes_2;
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

    let request = sui_types::transaction_driver_types::ExecuteTransactionRequestV3 {
        transaction: signed_transaction.try_into()?,
        include_events: read_mask.contains(ExecutedTransaction::EVENTS_FIELD.name),
        include_input_objects: read_mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
            || read_mask.contains(ExecutedTransaction::OBJECTS_FIELD.name)
            || read_mask.contains(ExecutedTransaction::EFFECTS_FIELD.name),
        include_output_objects: read_mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
            || read_mask.contains(ExecutedTransaction::OBJECTS_FIELD.name)
            || read_mask.contains(ExecutedTransaction::EFFECTS_FIELD.name),
        include_auxiliary_data: false,
    };

    let sui_types::transaction_driver_types::ExecuteTransactionResponseV3 {
        effects:
            sui_types::transaction_driver_types::FinalizedEffects {
                effects,
                finality_info: _,
            },
        events,
        input_objects,
        output_objects,
        auxiliary_data: _,
    } = executor.execute_transaction(request, None).await?;

    let executed_transaction = {
        let events = read_mask
            .subtree(ExecutedTransaction::EVENTS_FIELD)
            .and_then(|mask| events.map(|events| service.render_events_to_proto(&events, &mask)));

        let objects = {
            let mut objects = sui_types::full_checkpoint_content::ObjectSet::default();
            for o in input_objects
                .into_iter()
                .chain(output_objects.into_iter())
                .flatten()
            {
                objects.insert(o);
            }
            objects
        };

        let balance_changes = if read_mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD) {
            derive_balance_changes_2(&effects, &objects)
                .into_iter()
                .map(Into::into)
                .collect()
        } else {
            vec![]
        };

        let effects = read_mask
            .subtree(ExecutedTransaction::EFFECTS_FIELD)
            .map(|mask| {
                service.render_effects_to_proto(
                    &effects,
                    &[],
                    |object_id| {
                        objects
                            .iter()
                            .find(|o| o.id() == *object_id)
                            .map(|o| o.into())
                    },
                    &mask,
                )
            });

        let mut message = ExecutedTransaction::default();
        message.digest = read_mask
            .contains(ExecutedTransaction::DIGEST_FIELD)
            .then(|| transaction.digest().to_string());
        message.transaction = read_mask
            .subtree(ExecutedTransaction::TRANSACTION_FIELD)
            .map(|mask| Transaction::merge_from(transaction, &mask));
        message.signatures = read_mask
            .subtree(ExecutedTransaction::SIGNATURES_FIELD)
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
        message.objects = read_mask
            .subtree(
                ExecutedTransaction::path_builder()
                    .objects()
                    .objects()
                    .finish(),
            )
            .map(|mask| {
                ObjectSet::default().with_objects(
                    objects
                        .iter()
                        .map(|o| service.render_object_to_proto(o, &mask))
                        .collect(),
                )
            });
        message
    };

    Ok(ExecuteTransactionResponse::default().with_transaction(executed_transaction))
}
