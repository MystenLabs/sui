// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use axum::{
    extract::{Path, State},
    Json, TypedHeader,
};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::{
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber},
    storage::ReadStore,
};

use crate::{headers::Accept, AppError, Bcs};

pub const GET_LATEST_CHECKPOINT_PATH: &str = "/checkpoints";
pub const GET_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint";
pub const GET_FULL_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint/full";

pub async fn get_full_checkpoint<S: ReadStore>(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    TypedHeader(accept): TypedHeader<Accept>,
    State(state): State<S>,
) -> Result<Bcs<CheckpointData>, AppError> {
    if accept.as_str() != crate::APPLICATION_BCS {
        return Err(AppError(anyhow::anyhow!("invalid accept type")));
    }

    let verified_summary = state
        .get_checkpoint_by_sequence_number(checkpoint_id)?
        .ok_or_else(|| anyhow::anyhow!("missing checkpoint"))?;
    let checkpoint_contents = state
        .get_checkpoint_contents_by_digest(&verified_summary.content_digest)?
        .ok_or_else(|| anyhow::anyhow!("missing checkpoint contents"))?;

    let checkpoint_data = state.get_checkpoint_data(verified_summary, checkpoint_contents)?;

    Ok(Bcs(checkpoint_data))
}

pub async fn get_latest_checkpoint<S: ReadStore>(
    State(state): State<S>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let verified_summary = state.get_latest_checkpoint()?;
    Ok(Json(verified_summary.into()))
}

pub async fn get_checkpoint<S: ReadStore>(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    State(state): State<S>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let verified_summary = state
        .get_checkpoint_by_sequence_number(checkpoint_id)?
        .ok_or_else(|| anyhow::anyhow!("missing checkpoint"))?;
    Ok(Json(verified_summary.into()))
}
