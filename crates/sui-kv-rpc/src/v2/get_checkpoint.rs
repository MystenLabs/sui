// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, CHECKPOINTS_PIPELINE, CheckpointData, KeyValueStoreReader};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, GetCheckpointRequest, GetCheckpointResponse};
use sui_rpc_api::{
    CheckpointNotFoundError, ErrorReason, RpcError, proto::google::rpc::bad_request::FieldViolation,
};
use sui_types::digests::CheckpointDigest;

use crate::bigtable_client::BigTableClient as LimitedBigTableClient;
use crate::config::{PipelineStage, ResolvedStageConfig, StagesConfig};
use crate::render;
use crate::resolve;

pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::CHECKPOINT;

pub async fn get_checkpoint(
    mut client: BigTableClient,
    limited_client: LimitedBigTableClient,
    stages: &StagesConfig,
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
    let needs_full = resolve::needs_transactions_or_objects(&read_mask);
    let columns = resolve::list_checkpoint_columns(&read_mask, needs_full);
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

    let message = if needs_full {
        let transactions_stage = stages.stage(PipelineStage::Transactions);
        let objects_stage = stages.stage(PipelineStage::Objects);
        resolve_full_checkpoint(
            limited_client,
            checkpoint,
            &read_mask,
            transactions_stage,
            objects_stage,
        )
        .await?
    } else {
        render::checkpoint_to_response(checkpoint, &read_mask)?
    };
    Ok(GetCheckpointResponse::new(message))
}

/// Heavy path: resolve the checkpoint's transactions and (when requested)
/// objects from BigTable, then render the full proto `Checkpoint`.
async fn resolve_full_checkpoint(
    limited_client: LimitedBigTableClient,
    checkpoint: CheckpointData,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
) -> Result<Checkpoint, RpcError> {
    let cp_seq = checkpoint
        .summary
        .as_ref()
        .map(|s| s.sequence_number)
        .ok_or_else(|| RpcError::new(tonic::Code::Internal, "checkpoint summary column missing"))?;

    let (_, cp_data, txs, objects) = resolve::resolve_checkpoint(
        limited_client,
        read_mask,
        transactions_stage,
        objects_stage,
        cp_seq,
        checkpoint,
    )
    .await?;

    render::render_full_checkpoint(cp_data, txs, objects, read_mask)
}
