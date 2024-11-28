// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Query;
use axum::extract::{Path, State};
use axum::Json;
use sui_sdk_types::types::{CheckpointSequenceNumber, SignedCheckpointSummary};
use sui_types::storage::ReadStore;
use tap::Pipe;

use crate::reader::StateReader;
use crate::response::{Bcs, ResponseContent};
use crate::rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::rest::PageCursor;
use crate::service::checkpoints::{CheckpointId, CheckpointNotFoundError};
use crate::types::{CheckpointResponse, GetCheckpointOptions};
use crate::{Direction, RpcService};
use crate::{Result, RpcServiceError};
use documented::Documented;

use super::accept::AcceptFormat;

/// Fetch a Checkpoint
///
/// Fetch a checkpoint either by `CheckpointSequenceNumber` (checkpoint height) or by
/// `CheckpointDigest` and optionally request its contents.
///
/// If the checkpoint has been pruned and is not available, a 410 will be returned.
#[derive(Documented)]
pub struct GetCheckpoint;

impl ApiEndpoint<RpcService> for GetCheckpoint {
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
            .query_parameters::<GetCheckpointOptions>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<CheckpointResponse>(generator)
                    .build(),
            )
            .response(404, ResponseBuilder::new().build())
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_checkpoint)
    }
}

async fn get_checkpoint(
    Path(checkpoint_id): Path<CheckpointId>,
    Query(options): Query<GetCheckpointOptions>,
    State(state): State<RpcService>,
) -> Result<Json<CheckpointResponse>> {
    state.get_checkpoint(Some(checkpoint_id), options).map(Json)
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

impl ApiEndpoint<RpcService> for ListCheckpoints {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/checkpoints"
    }

    fn stable(&self) -> bool {
        // Before making this api stable we'll need to properly handle the options that can be
        // provided as inputs.
        false
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Checkpoint")
            .operation_id("List Checkpoints")
            .description(Self::DOCS)
            .query_parameters::<ListCheckpointsPaginationParameters>(generator)
            .query_parameters::<GetCheckpointOptions>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Vec<CheckpointResponse>>(generator)
                    .bcs_content()
                    .header::<String>(crate::types::X_SUI_CURSOR, generator)
                    .build(),
            )
            .response(410, ResponseBuilder::new().build())
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), list_checkpoints)
    }
}

async fn list_checkpoints(
    Query(parameters): Query<ListCheckpointsPaginationParameters>,
    Query(options): Query<GetCheckpointOptions>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<(
    PageCursor<CheckpointSequenceNumber>,
    ResponseContent<Vec<SignedCheckpointSummary>, Vec<CheckpointResponse>>,
)> {
    let latest_checkpoint = state.inner().get_latest_checkpoint()?.sequence_number;
    let oldest_checkpoint = state.inner().get_lowest_available_checkpoint()?;
    let limit = parameters.limit();
    let start = parameters.start(latest_checkpoint);
    let direction = parameters.direction();

    if start < oldest_checkpoint {
        return Err(crate::RpcServiceError::new(
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
                    let contents = if options.include_contents() {
                        Some(contents.try_into()?)
                    } else {
                        None
                    };
                    Ok(CheckpointResponse {
                        sequence_number: checkpoint.sequence_number,
                        digest: checkpoint.digest(),
                        summary: Some(checkpoint),
                        signature: Some(signature),
                        contents,
                        summary_bcs: None,
                        contents_bcs: None,
                    })
                })
        })
        .collect::<Result<Vec<_>>>()?;

    let cursor = checkpoints.last().and_then(|checkpoint| match direction {
        Direction::Ascending => checkpoint.sequence_number.checked_add(1),
        Direction::Descending => {
            let cursor = checkpoint.sequence_number.checked_sub(1);
            // If we've exhausted our available checkpoint range then there are no more pages left
            if cursor < Some(oldest_checkpoint) {
                None
            } else {
                cursor
            }
        }
    });

    match accept {
        AcceptFormat::Json => ResponseContent::Json(checkpoints),
        // In order to work around compatibility issues with existing clients, keep the BCS form as
        // the old format without contents
        AcceptFormat::Bcs => {
            let checkpoints = checkpoints
                .into_iter()
                .map(|c| SignedCheckpointSummary {
                    checkpoint: c.summary.unwrap(),
                    signature: c.signature.unwrap(),
                })
                .collect();
            ResponseContent::Bcs(checkpoints)
        }
    }
    .pipe(|entries| (PageCursor(cursor), entries))
    .pipe(Ok)
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ListCheckpointsPaginationParameters {
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
}

impl ListCheckpointsPaginationParameters {
    pub fn limit(&self) -> usize {
        self.limit
            .map(|l| (l as usize).clamp(1, crate::rest::MAX_PAGE_SIZE))
            .unwrap_or(crate::rest::DEFAULT_PAGE_SIZE)
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

impl ApiEndpoint<RpcService> for GetFullCheckpoint {
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

    fn handler(&self) -> RouteHandler<RpcService> {
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
            return Err(RpcServiceError::new(
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
                return Err(crate::RpcServiceError::new(
                    axum::http::StatusCode::GONE,
                    "Old checkpoints have been pruned",
                ));
            }

            state.inner().get_checkpoint_by_sequence_number(s)
        }
        CheckpointId::Digest(d) => state.inner().get_checkpoint_by_digest(&d.into()),
    }
    .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_contents = state
        .inner()
        .get_checkpoint_contents_by_digest(&verified_summary.content_digest)
        .ok_or(CheckpointNotFoundError(checkpoint_id))?;

    let checkpoint_data = state
        .inner()
        .get_checkpoint_data(verified_summary, checkpoint_contents)?;

    Ok(Bcs(checkpoint_data))
}
