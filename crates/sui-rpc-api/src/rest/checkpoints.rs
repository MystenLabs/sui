// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Query;
use axum::extract::{Path, State};
use axum::Json;
use sui_sdk_types::types::{CheckpointSequenceNumber, SignedCheckpointSummary};
use sui_types::storage::ReadStore;

use crate::reader::StateReader;
use crate::rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::rest::PageCursor;
use crate::service::checkpoints::CheckpointId;
use crate::types::{CheckpointResponse, GetCheckpointOptions};
use crate::Result;
use crate::{Direction, RpcService};
use documented::Documented;

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
    State(state): State<StateReader>,
) -> Result<(
    PageCursor<CheckpointSequenceNumber>,
    Json<Vec<CheckpointResponse>>,
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

    Ok((PageCursor(cursor), Json(checkpoints)))
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
