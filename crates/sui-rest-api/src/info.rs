// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::{RestService, Result};
use axum::extract::State;
use axum::Json;
use documented::Documented;
use sui_sdk_types::types::CheckpointDigest;
use tap::Pipe;

/// Get basic information about the state of a Node
#[derive(Documented)]
pub struct GetNodeInfo;

impl ApiEndpoint<RestService> for GetNodeInfo {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/"
    }

    fn stable(&self) -> bool {
        true
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("General")
            .operation_id("Get NodeInfo")
            .description(Self::DOCS)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<NodeInfo>(generator)
                    .build(),
            )
            .response(500, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_node_info)
    }
}

async fn get_node_info(State(state): State<RestService>) -> Result<Json<NodeInfo>> {
    let latest_checkpoint = state.reader.inner().get_latest_checkpoint()?;
    let lowest_available_checkpoint = state
        .reader
        .inner()
        .get_lowest_available_checkpoint()?
        .pipe(Some);
    let lowest_available_checkpoint_objects = state
        .reader
        .inner()
        .get_lowest_available_checkpoint_objects()?
        .pipe(Some);

    NodeInfo {
        checkpoint_height: latest_checkpoint.sequence_number,
        lowest_available_checkpoint,
        lowest_available_checkpoint_objects,
        timestamp_ms: latest_checkpoint.timestamp_ms,
        epoch: latest_checkpoint.epoch(),
        chain_id: CheckpointDigest::new(state.chain_id().as_bytes().to_owned()),
        chain: state.chain_id().chain().as_str().into(),
        software_version: state.software_version().into(),
    }
    .pipe(Json)
    .pipe(Ok)
}

/// Basic information about the state of a Node
#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeInfo {
    /// The chain identifier of the chain that this Node is on
    pub chain_id: CheckpointDigest,

    /// Human readable name of the chain that this Node is on
    pub chain: Cow<'static, str>,

    /// Current epoch of the Node based on its highest executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub epoch: u64,

    /// Checkpoint height of the most recently executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub checkpoint_height: u64,

    /// Unix timestamp of the most recently executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub timestamp_ms: u64,

    /// The lowest checkpoint for which checkpoints and transaction data is available
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    #[schemars(with = "Option<crate::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest_available_checkpoint: Option<u64>,

    /// The lowest checkpoint for which object data is available
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    #[schemars(with = "Option<crate::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest_available_checkpoint_objects: Option<u64>,
    pub software_version: Cow<'static, str>,
    //TODO include current protocol version
}
