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
use sui_rpc::proto::sui::rpc::v2beta2::BatchGetTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::BatchGetTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2beta2::Event;
use sui_rpc::proto::sui::rpc::v2beta2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2beta2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2beta2::GetTransactionResponse;
use sui_rpc::proto::sui::rpc::v2beta2::GetTransactionResult;
use sui_rpc::proto::sui::rpc::v2beta2::Transaction;
use sui_rpc::proto::sui::rpc::v2beta2::TransactionEffects;
use sui_rpc::proto::sui::rpc::v2beta2::TransactionEvents;
use sui_rpc::proto::sui::rpc::v2beta2::UserSignature;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::{Address, Digest};
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

pub const READ_MASK_DEFAULT: &str = "digest";

#[tracing::instrument(skip(service))]
pub fn get_transaction(
    service: &RpcService,
    request: GetTransactionRequest,
) -> Result<GetTransactionResponse, RpcError> {
    let transaction_digest = request
        .digest
        .ok_or_else(|| {
            FieldViolation::new("digest")
                .with_description("missing digest")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<Digest>()
        .map_err(|e| {
            FieldViolation::new("digest")
                .with_description(format!("invalid digest: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    let transaction_read = service.reader.get_transaction_read(transaction_digest)?;

    let transaction = transaction_to_response(service, transaction_read, &read_mask);

    Ok(GetTransactionResponse {
        transaction: Some(transaction),
    })
}

#[tracing::instrument(skip(service))]
pub fn batch_get_transactions(
    service: &RpcService,
    BatchGetTransactionsRequest { digests, read_mask }: BatchGetTransactionsRequest,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = {
        let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    let transactions = digests
        .into_iter()
        .enumerate()
        .map(|(idx, digest)| {
            let digest = digest.parse().map_err(|e| {
                FieldViolation::new_at("digests", idx)
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

            service
                .reader
                .get_transaction_read(digest)
                .map(|transaction_read| {
                    transaction_to_response(service, transaction_read, &read_mask)
                })
        })
        .map(|result| match result {
            Ok(transaction) => GetTransactionResult::new_transaction(transaction),
            Err(error) => GetTransactionResult::new_error(error.into_status_proto()),
        })
        .collect();

    Ok(BatchGetTransactionsResponse { transactions })
}

fn transaction_to_response(
    service: &RpcService,
    source: crate::reader::TransactionRead,
    mask: &FieldMaskTree,
) -> ExecutedTransaction {
    let mut message = ExecutedTransaction::default();

    if mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(source.digest.to_string());
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
        message.transaction = Some(Transaction::merge_from(source.transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
        message.signatures = source
            .signatures
            .into_iter()
            .map(|s| UserSignature::merge_from(s, &submask))
            .collect();
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        let mut effects = TransactionEffects::merge_from(&source.effects, &submask);

        if let Some(object_types) = source.object_types {
            if submask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD.name) {
                for changed_object in effects.changed_objects.iter_mut() {
                    let Ok(object_id) = changed_object.object_id().parse::<Address>() else {
                        continue;
                    };

                    if let Some(ty) = object_types.get(&object_id.into()) {
                        changed_object.object_type = Some(match ty {
                            sui_types::base_types::ObjectType::Package => "package".to_owned(),
                            sui_types::base_types::ObjectType::Struct(struct_tag) => {
                                struct_tag.to_canonical_string(true)
                            }
                        });
                    }
                }
            }

            if submask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name) {
                for unchanged_consensus_object in effects.unchanged_consensus_objects.iter_mut() {
                    let Ok(object_id) = unchanged_consensus_object.object_id().parse::<Address>()
                    else {
                        continue;
                    };

                    if let Some(ty) = object_types.get(&object_id.into()) {
                        unchanged_consensus_object.object_type = Some(match ty {
                            sui_types::base_types::ObjectType::Package => "package".to_owned(),
                            sui_types::base_types::ObjectType::Struct(struct_tag) => {
                                struct_tag.to_canonical_string(true)
                            }
                        });
                    }
                }
            }
        }

        message.effects = Some(effects);
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD.name) {
        message.events = source.events.map(|events| {
            let mut message = TransactionEvents::merge_from(events.clone(), &submask);

            if let Some(event_mask) = submask.subtree(TransactionEvents::EVENTS_FIELD.name) {
                if event_mask.contains(Event::JSON_FIELD.name) {
                    for (message, event) in message.events.iter_mut().zip(&events.0) {
                        message.json = struct_tag_sdk_to_core(event.type_.clone()).ok().and_then(
                            |struct_tag| {
                                crate::grpc::v2beta2::render_json(
                                    service,
                                    &struct_tag,
                                    &event.contents,
                                )
                                .map(Box::new)
                            },
                        );
                    }
                }
            }

            message
        });
    }

    if mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
        message.checkpoint = source.checkpoint;
    }

    if mask.contains(ExecutedTransaction::TIMESTAMP_FIELD.name) {
        message.timestamp = source.timestamp_ms.map(timestamp_ms_to_proto);
    }

    if mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name) {
        message.balance_changes = source
            .balance_changes
            .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
            .unwrap_or_default();
    }

    message
}
