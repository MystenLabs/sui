// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::CheckpointData;
use sui_kvstore::tables::checkpoints::col;
use sui_kvstore::{BigTableClient, CHECKPOINTS_PIPELINE, KeyValueStoreReader};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, GetCheckpointRequest, GetCheckpointResponse};
use sui_rpc_api::{
    CheckpointNotFoundError, ErrorReason, RpcError, proto::google::rpc::bad_request::FieldViolation,
};
use sui_storage::object_store::util::{build_object_store, fetch_checkpoint};
use sui_types::digests::CheckpointDigest;

pub const READ_MASK_DEFAULT: &str = "sequence_number,digest";

pub async fn get_checkpoint(
    mut client: BigTableClient,
    request: GetCheckpointRequest,
    checkpoint_bucket: Option<String>,
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
    let columns = checkpoint_columns(&read_mask);
    let checkpoint = match request.checkpoint_id {
        Some(CheckpointId::Digest(digest)) => {
            let digest = digest.parse::<CheckpointDigest>().map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
            client
                .get_checkpoint_by_digest_filtered(digest, Some(&columns))
                .await?
                .ok_or(CheckpointNotFoundError::digest(digest.into()))?
        }
        Some(CheckpointId::SequenceNumber(sequence_number)) => client
            .get_checkpoints_filtered(&[sequence_number], Some(&columns))
            .await?
            .pop()
            .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?,
        _ => {
            let sequence_number = client
                .get_watermark_for_pipelines(&[CHECKPOINTS_PIPELINE])
                .await?
                .and_then(|wm| wm.checkpoint_hi_inclusive)
                .ok_or(CheckpointNotFoundError::sequence_number(0))?;
            client
                .get_checkpoints_filtered(&[sequence_number], Some(&columns))
                .await?
                .pop()
                .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?
        }
    };

    let message =
        checkpoint_to_response(checkpoint, &read_mask, checkpoint_bucket.as_deref()).await?;
    Ok(GetCheckpointResponse::new(message))
}

/// Render a `CheckpointData` into the proto `Checkpoint`, populating fields
/// according to `read_mask`. Shared by `get_checkpoint` and the
/// v2alpha list-checkpoints handler.
pub(crate) async fn checkpoint_to_response(
    checkpoint: CheckpointData,
    read_mask: &FieldMaskTree,
    checkpoint_bucket: Option<&str>,
) -> Result<Checkpoint, RpcError> {
    let summary = checkpoint
        .summary
        .ok_or_else(|| anyhow::anyhow!("checkpoint summary missing"))?;
    let sequence_number = summary.sequence_number;
    let mut message = Checkpoint::default();
    let summary: sui_sdk_types::CheckpointSummary = summary.try_into()?;
    message.merge(&summary, read_mask);

    if read_mask.contains(Checkpoint::SIGNATURE_FIELD) {
        let signatures = checkpoint
            .signatures
            .ok_or_else(|| anyhow::anyhow!("checkpoint signatures missing"))?;
        let signatures: sui_sdk_types::ValidatorAggregatedSignature = signatures.into();
        message.merge(signatures, read_mask);
    }

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
        let contents = checkpoint
            .contents
            .ok_or_else(|| anyhow::anyhow!("checkpoint contents missing"))?;
        message.merge(
            sui_sdk_types::CheckpointContents::try_from(contents)?,
            read_mask,
        );
    }

    if (read_mask.contains(Checkpoint::TRANSACTIONS_FIELD)
        || read_mask.contains(Checkpoint::OBJECTS_FIELD))
        && let Some(url) = checkpoint_bucket
    {
        let store = build_object_store(url, vec![]);
        let checkpoint = fetch_checkpoint(&store, sequence_number).await?;

        message.merge(&checkpoint, read_mask);
    }

    Ok(message)
}

/// Compute the set of BigTable columns needed for the given read mask.
/// Always includes `s` (summary) since it provides sequence_number, digest, etc.
/// Only includes `sg` (signatures) and `c` (contents) when needed.
pub(crate) fn checkpoint_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    let mut columns = vec![col::SUMMARY];

    if mask.contains(Checkpoint::SIGNATURE_FIELD) {
        columns.push(col::SIGNATURES);
    }
    if mask.contains(Checkpoint::CONTENTS_FIELD) {
        columns.push(col::CONTENTS);
    }

    columns
}
