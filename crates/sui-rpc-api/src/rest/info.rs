// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::types::NodeInfo;
use crate::{Result, RpcService};
use axum::extract::State;
use axum::Json;
use documented::Documented;

/// Get basic information about the state of a Node
#[derive(Documented)]
pub struct GetNodeInfo;

impl ApiEndpoint<RpcService> for GetNodeInfo {
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

    fn handler(&self) -> crate::rest::openapi::RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_node_info)
    }
}

async fn get_node_info(State(state): State<RpcService>) -> Result<Json<NodeInfo>> {
    state.get_node_info().map(Json)
}
