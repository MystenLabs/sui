// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::CheckpointNotFoundError;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointResponse;
use sui_sdk_types::Digest;

pub const READ_MASK_DEFAULT: &str = "sequence_number,digest";

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

    let mut checkpoint = Checkpoint::default();

    checkpoint.merge(summary, &read_mask);
    checkpoint.merge(signature.clone(), &read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name)
        || read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
    {
        let core_contents = service
            .reader
            .inner()
            .get_checkpoint_contents_by_sequence_number(sequence_number)
            .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?;

        if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            checkpoint.merge(core_contents.clone(), &read_mask);
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
                    let balance_changes = submask
                        .contains(ExecutedTransaction::BALANCE_CHANGES_FIELD)
                        .then(|| {
                            service
                                .reader
                                .get_transaction_info(t.transaction.digest())
                                .map(|info| {
                                    info.balance_changes
                                        .into_iter()
                                        .map(sui_rpc::proto::sui::rpc::v2::BalanceChange::from)
                                        .collect::<Vec<_>>()
                                })
                        })
                        .flatten()
                        .unwrap_or_default();
                    let mut transaction = ExecutedTransaction::merge_from(t, &submask);
                    transaction.checkpoint = submask
                        .contains(ExecutedTransaction::CHECKPOINT_FIELD)
                        .then_some(sequence_number);
                    transaction.timestamp = submask
                        .contains(ExecutedTransaction::TIMESTAMP_FIELD)
                        .then(|| sui_rpc::proto::timestamp_ms_to_proto(timestamp_ms));
                    transaction.balance_changes = balance_changes;
                    transaction
                })
                .collect();
        }
    }

    Ok(GetCheckpointResponse::new(checkpoint))
}
