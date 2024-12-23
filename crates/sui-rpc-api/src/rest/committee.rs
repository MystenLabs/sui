// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    Result, RpcService,
};
use axum::{
    extract::{Path, State},
    Json,
};
use sui_sdk_types::{EpochId, ValidatorCommittee};

pub struct GetLatestCommittee;

impl ApiEndpoint<RpcService> for GetLatestCommittee {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system/committee"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("System")
            .operation_id("GetLatestCommittee")
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<ValidatorCommittee>(generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_latest_committee)
    }
}

async fn get_latest_committee(State(state): State<RpcService>) -> Result<Json<ValidatorCommittee>> {
    state.get_committee(None).map(Json)
}

pub struct GetCommittee;

impl ApiEndpoint<RpcService> for GetCommittee {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system/committee/{epoch}"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("System")
            .operation_id("GetCommittee")
            .path_parameter::<EpochId>("epoch", generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<ValidatorCommittee>(generator)
                    .build(),
            )
            .response(404, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_committee)
    }
}

async fn get_committee(
    Path(epoch): Path<EpochId>,
    State(state): State<RpcService>,
) -> Result<Json<ValidatorCommittee>> {
    state.get_committee(Some(epoch)).map(Json)
}
