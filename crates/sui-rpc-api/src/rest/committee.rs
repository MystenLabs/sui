// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    response::ResponseContent,
    rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    Result, RpcService,
};
use axum::extract::{Path, State};
use sui_sdk_types::types::{EpochId, ValidatorCommittee};
use tap::Pipe;

use super::accept::AcceptFormat;

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
                    .bcs_content()
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_latest_committee)
    }
}

async fn get_latest_committee(
    accept: AcceptFormat,
    State(state): State<RpcService>,
) -> Result<ResponseContent<ValidatorCommittee, ValidatorCommittee>> {
    let committee = state.get_committee(None)?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(committee),
        AcceptFormat::Bcs => ResponseContent::Bcs(committee),
    }
    .pipe(Ok)
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
                    .bcs_content()
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
    accept: AcceptFormat,
    State(state): State<RpcService>,
) -> Result<ResponseContent<ValidatorCommittee, ValidatorCommittee>> {
    let committee = state.get_committee(Some(epoch))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(committee),
        AcceptFormat::Bcs => ResponseContent::Bcs(committee),
    }
    .pipe(Ok)
}
