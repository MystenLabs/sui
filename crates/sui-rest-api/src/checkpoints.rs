// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::{Path, State};
use sui_sdk2::types::{
    CheckpointData, CheckpointDigest, CheckpointSequenceNumber, SignedCheckpointSummary,
};
use sui_types::storage::ReadStore;
use tap::Pipe;

use crate::{accept::AcceptFormat, response::ResponseContent, Result};

pub const GET_LATEST_CHECKPOINT_PATH: &str = "/checkpoints";
pub const GET_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint";
pub const GET_FULL_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint/full";

pub async fn get_full_checkpoint<S: ReadStore>(
    Path(checkpoint_id): Path<CheckpointId>,
    accept: AcceptFormat,
    State(state): State<S>,
) -> Result<ResponseContent<CheckpointData>> {
    let verified_summary = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => state.get_checkpoint_by_sequence_number(s),
        CheckpointId::Digest(d) => state.get_checkpoint_by_digest(&d.into()),
    }?
    .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_contents = state
        .get_checkpoint_contents_by_digest(&verified_summary.content_digest)?
        .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_data = state
        .get_checkpoint_data(verified_summary, checkpoint_contents)?
        .into();

    match accept {
        AcceptFormat::Json => ResponseContent::Json(checkpoint_data),
        AcceptFormat::Bcs => ResponseContent::Bcs(checkpoint_data),
    }
    .pipe(Ok)
}

pub async fn get_latest_checkpoint<S: ReadStore>(
    accept: AcceptFormat,
    State(state): State<S>,
) -> Result<ResponseContent<SignedCheckpointSummary>> {
    let summary = state.get_latest_checkpoint()?.into_inner().into();

    match accept {
        AcceptFormat::Json => ResponseContent::Json(summary),
        AcceptFormat::Bcs => ResponseContent::Bcs(summary),
    }
    .pipe(Ok)
}

pub async fn get_checkpoint<S: ReadStore>(
    Path(checkpoint_id): Path<CheckpointId>,
    accept: AcceptFormat,
    State(state): State<S>,
) -> Result<ResponseContent<SignedCheckpointSummary>> {
    let summary = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => state.get_checkpoint_by_sequence_number(s),
        CheckpointId::Digest(d) => state.get_checkpoint_by_digest(&d.into()),
    }?
    .ok_or(CheckpointNotFoundError(checkpoint_id))?
    .into_inner()
    .into();

    match accept {
        AcceptFormat::Json => ResponseContent::Json(summary),
        AcceptFormat::Bcs => ResponseContent::Bcs(summary),
    }
    .pipe(Ok)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CheckpointId {
    SequenceNumber(CheckpointSequenceNumber),
    Digest(CheckpointDigest),
}

impl<'de> serde::Deserialize<'de> for CheckpointId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;

        if let Ok(s) = raw.parse::<CheckpointSequenceNumber>() {
            Ok(Self::SequenceNumber(s))
        } else if let Ok(d) = raw.parse::<CheckpointDigest>() {
            Ok(Self::Digest(d))
        } else {
            Err(serde::de::Error::custom(format!(
                "unrecognized checkpoint-id {raw}"
            )))
        }
    }
}

impl serde::Serialize for CheckpointId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            CheckpointId::SequenceNumber(s) => serializer.serialize_str(&s.to_string()),
            CheckpointId::Digest(d) => serializer.serialize_str(&d.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct CheckpointNotFoundError(CheckpointId);

impl std::fmt::Display for CheckpointNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Checkpoint ")?;

        match self.0 {
            CheckpointId::SequenceNumber(n) => write!(f, "{n}")?,
            CheckpointId::Digest(d) => write!(f, "{d}")?,
        }

        write!(f, " not found")
    }
}

impl std::error::Error for CheckpointNotFoundError {}

impl From<CheckpointNotFoundError> for crate::RestError {
    fn from(value: CheckpointNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}
