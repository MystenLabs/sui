// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Query;
use axum::extract::{Path, State};
use sui_sdk2::types::{
    CheckpointData, CheckpointDigest, CheckpointSequenceNumber, SignedCheckpointSummary,
};
use sui_types::storage::ReadStore;
use tap::Pipe;

use crate::openapi::{ApiEndpoint, RouteHandler};
use crate::reader::StateReader;
use crate::Page;
use crate::{accept::AcceptFormat, response::ResponseContent, Result};
use crate::{Direction, RestService};

pub struct GetCheckpointFull;

impl ApiEndpoint<RestService> for GetCheckpointFull {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints/{checkpoint}/full"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        generator.subschema_for::<CheckpointData>();

        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_checkpoint_full)
    }
}

async fn get_checkpoint_full(
    Path(checkpoint_id): Path<CheckpointId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<CheckpointData>> {
    let verified_summary = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => state.inner().get_checkpoint_by_sequence_number(s),
        CheckpointId::Digest(d) => state.inner().get_checkpoint_by_digest(&d.into()),
    }?
    .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_contents = state
        .inner()
        .get_checkpoint_contents_by_digest(&verified_summary.content_digest)?
        .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_data = state
        .inner()
        .get_checkpoint_data(verified_summary, checkpoint_contents)?
        .into();

    match accept {
        AcceptFormat::Json => ResponseContent::Json(checkpoint_data),
        AcceptFormat::Bcs => ResponseContent::Bcs(checkpoint_data),
    }
    .pipe(Ok)
}

pub struct GetCheckpoint;

impl ApiEndpoint<RestService> for GetCheckpoint {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints/{checkpoint}"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        generator.subschema_for::<SignedCheckpointSummary>();

        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_checkpoint)
    }
}

async fn get_checkpoint(
    Path(checkpoint_id): Path<CheckpointId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<SignedCheckpointSummary>> {
    let summary = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => state.inner().get_checkpoint_by_sequence_number(s),
        CheckpointId::Digest(d) => state.inner().get_checkpoint_by_digest(&d.into()),
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

pub struct ListCheckpoints;

impl ApiEndpoint<RestService> for ListCheckpoints {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        generator.subschema_for::<SignedCheckpointSummary>();

        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), list_checkpoints)
    }
}

async fn list_checkpoints(
    Query(parameters): Query<ListCheckpointsQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Page<SignedCheckpointSummary, CheckpointSequenceNumber>> {
    let latest_checkpoint = state.inner().get_latest_checkpoint()?.sequence_number;
    let oldest_checkpoint = state.inner().get_lowest_available_checkpoint()?;
    let limit = parameters.limit();
    let start = parameters.start(latest_checkpoint);
    let direction = parameters.direction();

    if start < oldest_checkpoint {
        return Err(crate::RestError::new(
            axum::http::StatusCode::GONE,
            "Old checkpoints have been pruned",
        ));
    }

    let checkpoints = state
        .checkpoint_iter(direction, start)
        .map(|result| {
            result.map(|(checkpoint, _contents)| SignedCheckpointSummary::from(checkpoint))
        })
        .take(limit)
        .collect::<Result<Vec<_>, _>>()?;

    let cursor = checkpoints.last().and_then(|checkpoint| match direction {
        Direction::Ascending => checkpoint.checkpoint.sequence_number.checked_add(1),
        Direction::Descending => checkpoint.checkpoint.sequence_number.checked_sub(1),
    });

    match accept {
        AcceptFormat::Json => ResponseContent::Json(checkpoints),
        AcceptFormat::Bcs => ResponseContent::Bcs(checkpoints),
    }
    .pipe(|entries| Page { entries, cursor })
    .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ListCheckpointsQueryParameters {
    pub limit: Option<u32>,
    /// The checkpoint to start listing from.
    ///
    /// Defaults to the latest checkpoint if not provided.
    pub start: Option<CheckpointSequenceNumber>,
    pub direction: Option<Direction>,
}

impl ListCheckpointsQueryParameters {
    pub fn limit(&self) -> usize {
        self.limit
            .map(|l| (l as usize).clamp(1, crate::MAX_PAGE_SIZE))
            .unwrap_or(crate::DEFAULT_PAGE_SIZE)
    }

    pub fn start(&self, default: CheckpointSequenceNumber) -> CheckpointSequenceNumber {
        self.start.unwrap_or(default)
    }

    pub fn direction(&self) -> Direction {
        self.direction.unwrap_or(Direction::Descending)
    }
}
