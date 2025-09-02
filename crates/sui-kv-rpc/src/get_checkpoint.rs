// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2beta2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2beta2::{Checkpoint, GetCheckpointRequest, GetCheckpointResponse};
use sui_rpc_api::{
    proto::google::rpc::bad_request::FieldViolation, CheckpointNotFoundError, ErrorReason, RpcError,
};
use sui_types::digests::CheckpointDigest;

pub const READ_MASK_DEFAULT: &str = "sequence_number,digest";

pub async fn get_checkpoint(
    mut client: BigTableClient,
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
    let checkpoint = match request.checkpoint_id {
        Some(CheckpointId::Digest(digest)) => {
            let digest = digest.parse::<CheckpointDigest>().map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
            client
                .get_checkpoint_by_digest(digest)
                .await?
                .ok_or(CheckpointNotFoundError::digest(digest.into()))?
        }
        Some(CheckpointId::SequenceNumber(sequence_number)) => client
            .get_checkpoints(&[sequence_number])
            .await?
            .pop()
            .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?,
        None => {
            let sequence_number = client.get_latest_checkpoint().await?;
            client
                .get_checkpoints(&[sequence_number])
                .await?
                .pop()
                .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?
        }
        _ => {
            let sequence_number = client.get_latest_checkpoint().await?;
            client
                .get_checkpoints(&[sequence_number])
                .await?
                .pop()
                .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?
        }
    };
    let mut message = Checkpoint::default();
    let summary: sui_sdk_types::CheckpointSummary = checkpoint.summary.try_into()?;
    let signatures: sui_sdk_types::ValidatorAggregatedSignature = checkpoint.signatures.into();
    message.merge(&summary, &read_mask);
    message.merge(signatures, &read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
        message.merge(
            sui_sdk_types::CheckpointContents::try_from(checkpoint.contents)?,
            &read_mask,
        );
    }
    // TODO: handle Checkpoint::TRANSACTIONS_FIELD submask
    Ok(GetCheckpointResponse::new(message))
}
