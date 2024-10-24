// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Query;
use axum::extract::{Path, State};
use sui_sdk_types::types::{
    CheckpointContents, CheckpointDigest, CheckpointSequenceNumber, CheckpointSummary,
    SignedCheckpointSummary, ValidatorAggregatedSignature,
};
use sui_types::storage::ReadStore;
use tap::Pipe;

use crate::accept::AcceptJsonProtobufBcs;
use crate::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::proto::CheckpointPage;
use crate::reader::StateReader;
use crate::response::{Bcs, JsonProtobufBcs};
use crate::{accept::AcceptFormat, response::ResponseContent, RestError, Result};
use crate::{Direction, RestService};
use crate::{Page, PageCursor};
use documented::Documented;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CheckpointResponse {
    pub summary: CheckpointSummary,
    pub signature: ValidatorAggregatedSignature,
    pub contents: Option<CheckpointContents>,
}

/// Fetch a Checkpoint
///
/// Fetch a checkpoint either by `CheckpointSequenceNumber` (checkpoint height) or by
/// `CheckpointDigest` and optionally request its contents.
///
/// If the checkpoint has been pruned and is not available, a 410 will be returned.
#[derive(Documented)]
pub struct GetCheckpoint;

impl ApiEndpoint<RestService> for GetCheckpoint {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints/{checkpoint}"
    }

    fn stable(&self) -> bool {
        true
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Checkpoint")
            .operation_id("Get Checkpoint")
            .description(Self::DOCS)
            .path_parameter::<CheckpointId>("checkpoint", generator)
            .query_parameters::<GetCheckpointQueryParameters>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<CheckpointResponse>(generator)
                    .bcs_content()
                    .build(),
            )
            .response(404, ResponseBuilder::new().build())
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_checkpoint)
    }
}

async fn get_checkpoint(
    Path(checkpoint_id): Path<CheckpointId>,
    Query(parameters): Query<GetCheckpointQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<CheckpointResponse>> {
    let SignedCheckpointSummary {
        checkpoint,
        signature,
    } = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => {
            let oldest_checkpoint = state.inner().get_lowest_available_checkpoint()?;
            if s < oldest_checkpoint {
                return Err(crate::RestError::new(
                    axum::http::StatusCode::GONE,
                    "Old checkpoints have been pruned",
                ));
            }

            state.inner().get_checkpoint_by_sequence_number(s)
        }
        CheckpointId::Digest(d) => state.inner().get_checkpoint_by_digest(&d.into()),
    }?
    .ok_or(CheckpointNotFoundError(checkpoint_id))?
    .into_inner()
    .try_into()?;

    let contents = if parameters.contents {
        Some(
            state
                .inner()
                .get_checkpoint_contents_by_sequence_number(checkpoint.sequence_number)?
                .ok_or(CheckpointNotFoundError(checkpoint_id))?
                .try_into()?,
        )
    } else {
        None
    };

    let response = CheckpointResponse {
        summary: checkpoint,
        signature,
        contents,
    };

    match accept {
        AcceptFormat::Json => ResponseContent::Json(response),
        AcceptFormat::Bcs => ResponseContent::Bcs(response),
    }
    .pipe(Ok)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, schemars::JsonSchema)]
#[schemars(untagged)]
pub enum CheckpointId {
    #[schemars(
        title = "SequenceNumber",
        example = "CheckpointSequenceNumber::default"
    )]
    /// Sequence number or height of a Checkpoint
    SequenceNumber(#[schemars(with = "crate::_schemars::U64")] CheckpointSequenceNumber),
    #[schemars(title = "Digest", example = "example_digest")]
    /// Base58 encoded 32-byte digest of a Checkpoint
    Digest(CheckpointDigest),
}

fn example_digest() -> CheckpointDigest {
    "4btiuiMPvEENsttpZC7CZ53DruC3MAgfznDbASZ7DR6S"
        .parse()
        .unwrap()
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

/// Query parameters for the GetCheckpoint endpoint
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetCheckpointQueryParameters {
    /// Request `CheckpointContents` be included in the response
    #[serde(default)]
    pub contents: bool,
}

/// List Checkpoints
///
/// Request a page of checkpoints, and optionally their contents, ordered by
/// `CheckpointSequenceNumber`.
///
/// If the requested page is below the Node's `lowest_available_checkpoint`, a 410 will be
/// returned.
#[derive(Documented)]
pub struct ListCheckpoints;

impl ApiEndpoint<RestService> for ListCheckpoints {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints"
    }

    fn stable(&self) -> bool {
        true
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Checkpoint")
            .operation_id("List Checkpoints")
            .description(Self::DOCS)
            .query_parameters::<ListCheckpointsQueryParameters>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Vec<CheckpointResponse>>(generator)
                    .bcs_content()
                    .protobuf_content()
                    .header::<String>(crate::types::X_SUI_CURSOR, generator)
                    .build(),
            )
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), list_checkpoints)
    }
}

async fn list_checkpoints(
    Query(parameters): Query<ListCheckpointsQueryParameters>,
    accept: AcceptJsonProtobufBcs,
    State(state): State<StateReader>,
) -> Result<(
    PageCursor<CheckpointSequenceNumber>,
    JsonProtobufBcs<Vec<CheckpointResponse>, CheckpointPage, Vec<SignedCheckpointSummary>>,
)> {
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
        .take(limit)
        .map(|result| {
            result
                .map_err(Into::into)
                .and_then(|(checkpoint, contents)| {
                    let SignedCheckpointSummary {
                        checkpoint,
                        signature,
                    } = checkpoint.try_into()?;
                    let contents = if parameters.contents {
                        Some(contents.try_into()?)
                    } else {
                        None
                    };
                    Ok(CheckpointResponse {
                        summary: checkpoint,
                        signature,
                        contents,
                    })
                })
        })
        .collect::<Result<Vec<_>>>()?;

    let cursor = checkpoints.last().and_then(|checkpoint| match direction {
        Direction::Ascending => checkpoint.summary.sequence_number.checked_add(1),
        Direction::Descending => {
            let cursor = checkpoint.summary.sequence_number.checked_sub(1);
            // If we've exhausted our available checkpoint range then there are no more pages left
            if cursor < Some(oldest_checkpoint) {
                None
            } else {
                cursor
            }
        }
    });

    match accept {
        AcceptJsonProtobufBcs::Json => JsonProtobufBcs::Json(checkpoints),
        AcceptJsonProtobufBcs::Protobuf => JsonProtobufBcs::Protobuf(checkpoints.try_into()?),
        // In order to work around compatibility issues with existing clients, keep the BCS form as
        // the old format without contents
        AcceptJsonProtobufBcs::Bcs => {
            let checkpoints = checkpoints
                .into_iter()
                .map(|c| SignedCheckpointSummary {
                    checkpoint: c.summary,
                    signature: c.signature,
                })
                .collect();
            JsonProtobufBcs::Bcs(checkpoints)
        }
    }
    .pipe(|entries| (PageCursor(cursor), entries))
    .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ListCheckpointsQueryParameters {
    /// Page size limit for the response.
    ///
    /// Defaults to `50` if not provided with a maximum page size of `100`.
    pub limit: Option<u32>,
    /// The checkpoint to start listing from.
    ///
    /// Defaults to the latest checkpoint if not provided.
    pub start: Option<CheckpointSequenceNumber>,
    /// The direction to paginate in.
    ///
    /// Defaults to `descending` if not provided.
    pub direction: Option<Direction>,
    /// Request `CheckpointContents` be included in the response
    #[serde(default)]
    pub contents: bool,
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

/// Fetch a Full Checkpoint
///
/// Request a checkpoint and all data associated with it including:
/// - CheckpointSummary
/// - Validator Signature
/// - CheckpointContents
/// - Transactions, Effects, Events, as well as all input and output objects
///
/// If the requested checkpoint is below the Node's `lowest_available_checkpoint_objects`, a 410
/// will be returned.
#[derive(Documented)]
pub struct GetFullCheckpoint;

impl ApiEndpoint<RestService> for GetFullCheckpoint {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints/{checkpoint}/full"
    }

    fn stable(&self) -> bool {
        // TODO transactions are serialized with an intent message, do we want to change this
        // format to remove it (and remove user signature duplication) prior to stabalizing the
        // format?
        false
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Checkpoint")
            .operation_id("Get Full Checkpoint")
            .description(Self::DOCS)
            .path_parameter::<CheckpointId>("checkpoint", generator)
            .response(200, ResponseBuilder::new().bcs_content().build())
            .response(404, ResponseBuilder::new().build())
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_full_checkpoint)
    }
}

async fn get_full_checkpoint(
    Path(checkpoint_id): Path<CheckpointId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Bcs<sui_types::full_checkpoint_content::CheckpointData>> {
    match accept {
        AcceptFormat::Bcs => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type; only 'application/bcs' is supported",
            ))
        }
    }

    let verified_summary = match checkpoint_id {
        CheckpointId::SequenceNumber(s) => {
            // Since we need object contents we need to check for the lowest available checkpoint
            // with objects that hasn't been pruned
            let oldest_checkpoint = state.inner().get_lowest_available_checkpoint_objects()?;
            if s < oldest_checkpoint {
                return Err(crate::RestError::new(
                    axum::http::StatusCode::GONE,
                    "Old checkpoints have been pruned",
                ));
            }

            state.inner().get_checkpoint_by_sequence_number(s)
        }
        CheckpointId::Digest(d) => state.inner().get_checkpoint_by_digest(&d.into()),
    }?
    .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_contents = state
        .inner()
        .get_checkpoint_contents_by_digest(&verified_summary.content_digest)?
        .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_data = state
        .inner()
        .get_checkpoint_data(verified_summary, checkpoint_contents)?;

    Ok(Bcs(checkpoint_data))
}

/// List Full Checkpoints
///
/// Request a page of checkpoints and all data associated with them including:
/// - CheckpointSummary
/// - Validator Signature
/// - CheckpointContents
/// - Transactions, Effects, Events, as well as all input and output objects
///
/// If the requested page is below the Node's `lowest_available_checkpoint_objects`, a 410 will be
/// returned.
#[derive(Documented)]
pub struct ListFullCheckpoints;

impl ApiEndpoint<RestService> for ListFullCheckpoints {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints/full"
    }

    fn stable(&self) -> bool {
        // TODO transactions are serialized with an intent message, do we want to change this
        // format to remove it (and remove user signature duplication) prior to stabalizing the
        // format?
        false
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Checkpoint")
            .operation_id("List Full Checkpoints")
            .description(Self::DOCS)
            .query_parameters::<ListFullCheckpointsQueryParameters>(generator)
            .response(200, ResponseBuilder::new().bcs_content().build())
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), list_full_checkpoints)
    }
}

async fn list_full_checkpoints(
    Query(parameters): Query<ListFullCheckpointsQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Page<sui_types::full_checkpoint_content::CheckpointData, CheckpointSequenceNumber>> {
    match accept {
        AcceptFormat::Bcs => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type; only 'application/bcs' is supported",
            ))
        }
    }

    let latest_checkpoint = state.inner().get_latest_checkpoint()?.sequence_number;
    let oldest_checkpoint = state.inner().get_lowest_available_checkpoint_objects()?;
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
        .take(
            // only iterate until we've reached the edge of our objects available window
            if direction.is_descending() {
                std::cmp::min(limit, start.saturating_sub(oldest_checkpoint) as usize)
            } else {
                limit
            },
        )
        .map(|result| {
            result
                .map_err(Into::into)
                .and_then(|(checkpoint, contents)| {
                    state
                        .inner()
                        .get_checkpoint_data(
                            sui_types::messages_checkpoint::VerifiedCheckpoint::new_from_verified(
                                checkpoint,
                            ),
                            contents,
                        )
                        .map_err(Into::into)
                })
        })
        .collect::<Result<Vec<_>>>()?;

    let cursor = checkpoints.last().and_then(|checkpoint| match direction {
        Direction::Ascending => checkpoint.checkpoint_summary.sequence_number.checked_add(1),
        Direction::Descending => {
            let cursor = checkpoint.checkpoint_summary.sequence_number.checked_sub(1);
            // If we've exhausted our available object range then there are no more pages left
            if cursor < Some(oldest_checkpoint) {
                None
            } else {
                cursor
            }
        }
    });

    ResponseContent::Bcs(checkpoints)
        .pipe(|entries| Page { entries, cursor })
        .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ListFullCheckpointsQueryParameters {
    /// Page size limit for the response.
    ///
    /// Defaults to `5` if not provided with a maximum page size of `10`.
    pub limit: Option<u32>,
    /// The checkpoint to start listing from.
    ///
    /// Defaults to the latest checkpoint if not provided.
    pub start: Option<CheckpointSequenceNumber>,
    /// The direction to paginate in.
    ///
    /// Defaults to `descending` if not provided.
    pub direction: Option<Direction>,
}

impl ListFullCheckpointsQueryParameters {
    pub fn limit(&self) -> usize {
        self.limit.map(|l| (l as usize).clamp(1, 10)).unwrap_or(5)
    }

    pub fn start(&self, default: CheckpointSequenceNumber) -> CheckpointSequenceNumber {
        self.start.unwrap_or(default)
    }

    pub fn direction(&self) -> Direction {
        self.direction.unwrap_or(Direction::Descending)
    }
}
