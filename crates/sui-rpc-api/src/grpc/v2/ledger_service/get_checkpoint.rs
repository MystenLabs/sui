// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::CheckpointNotFoundError;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointResponse;
use sui_rpc::proto::sui::rpc::v2::ObjectSet;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_sdk_types::Digest;
use sui_types::balance_change::derive_balance_changes_2;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::CHECKPOINT;

#[tracing::instrument(skip(service))]
pub fn get_checkpoint(
    service: &RpcService,
    request: GetCheckpointRequest,
) -> Result<GetCheckpointResponse, RpcError> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
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
            let digest = digest.parse::<Digest>().map_err(|e| {
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
        _ => service.reader.inner().get_latest_checkpoint()?,
    };

    let summary = verified_summary.data();
    let signature = verified_summary.auth_sig();
    let sequence_number = summary.sequence_number;
    let timestamp_ms = summary.timestamp_ms;

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;

    if !(lowest_available_checkpoint..=latest_checkpoint).contains(&sequence_number) {
        return Err(CheckpointNotFoundError::sequence_number(sequence_number).into());
    }

    let mut checkpoint = Checkpoint::default();

    checkpoint.merge(summary, &read_mask);
    checkpoint.merge(signature.clone(), &read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name)
        || read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
        || read_mask.contains(Checkpoint::OBJECTS_FIELD.name)
    {
        let core_contents = service
            .reader
            .inner()
            .get_checkpoint_contents_by_sequence_number(sequence_number)
            .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?;

        if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            checkpoint.merge(core_contents.clone(), &read_mask);
        }

        if read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
            || read_mask.contains(Checkpoint::OBJECTS_FIELD.name)
        {
            let checkpoint_data = service
                .reader
                .inner()
                .get_checkpoint_data(verified_summary, core_contents)?;

            if let Some(submask) = read_mask
                .subtree(Checkpoint::OBJECTS_FIELD)
                .and_then(|submask| submask.subtree(ObjectSet::OBJECTS_FIELD))
            {
                let set = checkpoint_data
                    .object_set
                    .iter()
                    .map(|o| sui_rpc::proto::sui::rpc::v2::Object::merge_from(o, &submask))
                    .collect();
                checkpoint.objects = Some(ObjectSet::default().with_objects(set));
            }

            if let Some(submask) = read_mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
                // Share a single JSON-rendering budget across every event in
                // every transaction in the checkpoint. Without this, an
                // unauthenticated `GetCheckpoint` with a permissive `read_mask`
                // multiplies one input checkpoint into thousands of per-event
                // renders, each with its own `max_json_move_value_size` budget.
                let mut json_budget = service.config.max_json_move_value_response_size();
                checkpoint.transactions = checkpoint_data
                    .transactions
                    .into_iter()
                    .map(|t| {
                        let balance_changes =
                            if submask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD) {
                                derive_balance_changes_2(&t.effects, &checkpoint_data.object_set)
                                    .into_iter()
                                    .map(Into::into)
                                    .collect()
                            } else {
                                Vec::new()
                            };
                        let mut transaction = ExecutedTransaction::merge_from(&t, &submask);
                        transaction.checkpoint = submask
                            .contains(ExecutedTransaction::CHECKPOINT_FIELD)
                            .then_some(sequence_number);
                        transaction.timestamp = submask
                            .contains(ExecutedTransaction::TIMESTAMP_FIELD)
                            .then(|| sui_rpc::proto::timestamp_ms_to_proto(timestamp_ms));
                        transaction.balance_changes = balance_changes;
                        service.annotate_checkpoint_transaction(
                            &mut transaction,
                            &t,
                            &checkpoint_data.object_set,
                            &submask,
                            &mut json_budget,
                        );

                        transaction
                    })
                    .collect();
            }
        }
    }

    Ok(GetCheckpointResponse::new(checkpoint))
}
