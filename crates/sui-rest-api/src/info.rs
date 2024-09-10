// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::{RestService, Result};
use axum::extract::State;
use axum::Json;
use sui_sdk2::types::CheckpointDigest;
use tap::Pipe;

pub struct GetNodeInfo;

impl ApiEndpoint<RestService> for GetNodeInfo {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("General")
            .operation_id("GetNodeInfo")
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<NodeInfo>(generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_node_info)
    }
}

async fn get_node_info(State(state): State<RestService>) -> Result<Json<NodeInfo>> {
    let latest_checkpoint = state.reader.inner().get_latest_checkpoint()?;
    let lowest_available_checkpoint = state.reader.inner().get_lowest_available_checkpoint()?;
    let lowest_available_checkpoint_objects = state
        .reader
        .inner()
        .get_lowest_available_checkpoint_objects()?;

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

#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeInfo {
    pub chain_id: CheckpointDigest,
    pub chain: Cow<'static, str>,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub epoch: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub checkpoint_height: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub timestamp_ms: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub lowest_available_checkpoint: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub lowest_available_checkpoint_objects: u64,
    pub software_version: Cow<'static, str>,
    //TODO include current protocol version
}
