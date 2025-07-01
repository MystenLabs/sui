// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::BatchGetTransactionsRequest;
use crate::proto::rpc::v2beta::BatchGetTransactionsResponse;
use crate::proto::rpc::v2beta::Event;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::GetTransactionRequest;
use crate::proto::rpc::v2beta::Transaction;
use crate::proto::rpc::v2beta::TransactionEffects;
use crate::proto::rpc::v2beta::TransactionEvents;
use crate::proto::rpc::v2beta::UserSignature;
use crate::proto::timestamp_ms_to_proto;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_sdk_types::TransactionDigest;
use sui_types::base_types::ObjectID;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

#[tracing::instrument(skip(service))]
pub fn get_transaction(
    service: &RpcService,
    request: GetTransactionRequest,
) -> Result<ExecutedTransaction, RpcError> {
    let transaction_digest = request
        .digest
        .ok_or_else(|| {
            FieldViolation::new("digest")
                .with_description("missing digest")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<TransactionDigest>()
        .map_err(|e| {
            FieldViolation::new("digest")
                .with_description(format!("invalid digest: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetTransactionRequest::READ_MASK_DEFAULT));
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

    Ok(transaction_to_response(
        service,
        transaction_read,
        &read_mask,
    ))
}

#[tracing::instrument(skip(service))]
pub fn batch_get_transactions(
    service: &RpcService,
    BatchGetTransactionsRequest { digests, read_mask }: BatchGetTransactionsRequest,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = {
        let read_mask = read_mask
            .unwrap_or_else(|| FieldMask::from_str(BatchGetTransactionsRequest::READ_MASK_DEFAULT));
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
        .collect::<Result<_, _>>()?;

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
                    let Ok(object_id) = changed_object.object_id().parse::<ObjectID>() else {
                        continue;
                    };

                    if let Some(ty) = object_types.get(&object_id) {
                        changed_object.object_type = Some(match ty {
                            sui_types::base_types::ObjectType::Package => "package".to_owned(),
                            sui_types::base_types::ObjectType::Struct(struct_tag) => {
                                struct_tag.to_canonical_string(true)
                            }
                        });
                    }
                }
            }

            if submask.contains(TransactionEffects::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
                for unchanged_shared_object in effects.unchanged_shared_objects.iter_mut() {
                    let Ok(object_id) = unchanged_shared_object.object_id().parse::<ObjectID>()
                    else {
                        continue;
                    };

                    if let Some(ty) = object_types.get(&object_id) {
                        unchanged_shared_object.object_type = Some(match ty {
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
                        message.json = struct_tag_sdk_to_core(event.type_.clone())
                            .ok()
                            .and_then(|struct_tag| {
                                let layout = service
                                    .reader
                                    .inner()
                                    .get_struct_layout(&struct_tag)
                                    .ok()
                                    .flatten()?;
                                Some((layout, &event.contents))
                            })
                            .and_then(|(layout, contents)| {
                                sui_types::proto_value::ProtoVisitorBuilder::new(
                                    service.config.max_json_move_value_size(),
                                )
                                .deserialize_value(contents, &layout)
                                .map_err(|e| tracing::debug!("unable to convert to JSON: {e}"))
                                .ok()
                                .map(Box::new)
                            });
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

impl From<sui_types::balance_change::BalanceChange> for crate::proto::rpc::v2beta::BalanceChange {
    fn from(value: sui_types::balance_change::BalanceChange) -> Self {
        Self {
            address: Some(value.address.to_string()),
            coin_type: Some(value.coin_type.to_canonical_string(true)),
            amount: Some(value.amount.to_string()),
        }
    }
}
