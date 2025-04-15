// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::CheckpointNotFoundError;
use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::message::MessageMerge;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::get_checkpoint_request::CheckpointId;
use crate::proto::rpc::v2beta::Checkpoint;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::GetCheckpointRequest;
use crate::proto::rpc::v2beta::Object;
use crate::proto::rpc::v2beta::Transaction;
use crate::proto::rpc::v2beta::TransactionEffects;
use crate::proto::rpc::v2beta::TransactionEvents;
use crate::proto::rpc::v2beta::UserSignature;
use crate::proto::types::timestamp_ms_to_proto;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::CheckpointDigest;

#[tracing::instrument(skip(service))]
pub fn get_checkpoint(
    service: &RpcService,
    request: GetCheckpointRequest,
) -> Result<Checkpoint, RpcError> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetCheckpointRequest::READ_MASK_DEFAULT));
        read_mask.validate::<Checkpoint>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let verified_summary = match request.checkpoint_id {
        Some(CheckpointId::SequenceNumber(s)) => service
            .reader
            .inner()
            .get_checkpoint_by_sequence_number(s)
            .ok_or(CheckpointNotFoundError::sequence_number(s))?,
        Some(CheckpointId::Digest(digest)) => {
            let digest = digest.parse::<CheckpointDigest>().map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

            service
                .reader
                .inner()
                .get_checkpoint_by_digest(&digest.into())
                .ok_or(CheckpointNotFoundError::digest(digest))?
        }
        None => service.reader.inner().get_latest_checkpoint()?,
    };

    let sui_sdk_types::SignedCheckpointSummary {
        checkpoint: summary,
        signature,
    } = verified_summary.clone().into_inner().try_into()?;

    let mut checkpoint = Checkpoint::default();

    checkpoint.merge(&summary, &read_mask);
    checkpoint.merge(signature, &read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name)
        || read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
    {
        let core_contents = service
            .reader
            .inner()
            .get_checkpoint_contents_by_sequence_number(summary.sequence_number)
            .ok_or(CheckpointNotFoundError::sequence_number(
                summary.sequence_number,
            ))?;

        if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            checkpoint.merge(
                sui_sdk_types::CheckpointContents::try_from(core_contents.clone())?,
                &read_mask,
            );
        }

        if let Some(submask) = read_mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
            let checkpoint_data = service
                .reader
                .inner()
                .get_checkpoint_data(verified_summary, core_contents)?;

            checkpoint.transactions = checkpoint_data
                .transactions
                .into_iter()
                .map(|t| {
                    core_transaction_to_executed_transaction_proto(
                        t,
                        summary.sequence_number,
                        summary.timestamp_ms,
                        &submask,
                    )
                })
                .collect::<Result<_, _>>()?;
        }
    }

    Ok(checkpoint)
}

pub(crate) fn checkpoint_data_to_checkpoint_proto(
    checkpoint_data: sui_types::full_checkpoint_content::CheckpointData,
    read_mask: &FieldMaskTree,
) -> Result<Checkpoint, RpcError> {
    let sequence_number = checkpoint_data.checkpoint_summary.sequence_number;
    let timestamp_ms = checkpoint_data.checkpoint_summary.timestamp_ms;

    let sui_sdk_types::SignedCheckpointSummary {
        checkpoint: summary,
        signature,
    } = checkpoint_data.checkpoint_summary.try_into()?;

    let mut checkpoint = Checkpoint::default();

    checkpoint.merge(&summary, read_mask);
    checkpoint.merge(signature, read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
        checkpoint.merge(
            sui_sdk_types::CheckpointContents::try_from(checkpoint_data.checkpoint_contents)?,
            read_mask,
        );
    }

    if let Some(submask) = read_mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
        checkpoint.transactions = checkpoint_data
            .transactions
            .into_iter()
            .map(|t| {
                core_transaction_to_executed_transaction_proto(
                    t,
                    sequence_number,
                    timestamp_ms,
                    &submask,
                )
            })
            .collect::<Result<_, _>>()?;
    }

    Ok(checkpoint)
}

fn core_transaction_to_executed_transaction_proto(
    sui_types::full_checkpoint_content::CheckpointTransaction {
        transaction,
        effects,
        events,
        input_objects,
        output_objects,
    }: sui_types::full_checkpoint_content::CheckpointTransaction,
    checkpoint: u64,
    timestamp_ms: u64,
    read_mask: &FieldMaskTree,
) -> Result<ExecutedTransaction, RpcError> {
    let digest = read_mask
        .contains(ExecutedTransaction::DIGEST_FIELD.name)
        .then(|| {
            sui_sdk_types::TransactionDigest::from(transaction.digest().to_owned()).to_string()
        });

    let (transaction_data, signatures) = {
        let sender_signed = transaction.into_data().into_inner();
        (
            sender_signed.intent_message.value,
            sender_signed.tx_signatures,
        )
    };
    let transaction = read_mask
        .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
        .map(|mask| {
            sui_sdk_types::Transaction::try_from(transaction_data)
                .map(|transaction| Transaction::merge_from(transaction, &mask))
        })
        .transpose()?;

    let signatures = read_mask
        .subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
        .map(|mask| {
            signatures
                .into_iter()
                .map(|s| {
                    sui_sdk_types::UserSignature::try_from(s)
                        .map(|s| UserSignature::merge_from(s, &mask))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let effects = read_mask
        .subtree(ExecutedTransaction::EFFECTS_FIELD.name)
        .map(|mask| {
            sui_sdk_types::TransactionEffects::try_from(effects)
                .map(|effects| TransactionEffects::merge_from(&effects, &mask))
        })
        .transpose()?;

    let events = read_mask
        .subtree(ExecutedTransaction::EVENTS_FIELD.name)
        .and_then(|mask| {
            events.map(|events| {
                sui_sdk_types::TransactionEvents::try_from(events)
                    .map(|events| TransactionEvents::merge_from(events, &mask))
            })
        })
        .transpose()?;

    let input_objects = read_mask
        .subtree("input_objects")
        .map(|read_mask| {
            input_objects
                .into_iter()
                .map(|object| core_object_to_object_proto(object, &read_mask))
                .collect::<Result<_, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let output_objects = read_mask
        .subtree("output_objects")
        .map(|read_mask| {
            output_objects
                .into_iter()
                .map(|object| core_object_to_object_proto(object, &read_mask))
                .collect::<Result<_, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(ExecutedTransaction {
        digest,
        transaction,
        signatures,
        effects,
        events,
        checkpoint: read_mask
            .contains(ExecutedTransaction::CHECKPOINT_FIELD.name)
            .then_some(checkpoint),
        timestamp: read_mask
            .contains(ExecutedTransaction::TIMESTAMP_FIELD.name)
            .then(|| timestamp_ms_to_proto(timestamp_ms)),
        balance_changes: Vec::new(),
        input_objects,
        output_objects,
    })
}

fn core_object_to_object_proto(
    object: sui_types::object::Object,
    read_mask: &FieldMaskTree,
) -> Result<Object, RpcError> {
    let object = sui_sdk_types::Object::try_from(object)?;
    Ok(Object::merge_from(object, read_mask))
}
