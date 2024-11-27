// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    proto,
    response::JsonProtobufBcs,
    rest::accept::AcceptJsonProtobufBcs,
    rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    Result, RpcService,
};
use axum::extract::{Path, State};
use sui_sdk_types::types::{EpochId, ValidatorCommittee};
use tap::Pipe;

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
                    .protobuf_content()
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
    accept: AcceptJsonProtobufBcs,
    State(state): State<RpcService>,
) -> Result<JsonProtobufBcs<ValidatorCommittee, proto::ValidatorCommittee, ValidatorCommittee>> {
    let committee = state.get_committee(None)?;

    match accept {
        AcceptJsonProtobufBcs::Json => JsonProtobufBcs::Json(committee),
        AcceptJsonProtobufBcs::Protobuf => JsonProtobufBcs::Protobuf(committee.into()),
        AcceptJsonProtobufBcs::Bcs => JsonProtobufBcs::Bcs(committee),
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
                    .protobuf_content()
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
    accept: AcceptJsonProtobufBcs,
    State(state): State<RpcService>,
) -> Result<JsonProtobufBcs<ValidatorCommittee, proto::ValidatorCommittee, ValidatorCommittee>> {
    let committee = state.get_committee(Some(epoch))?;

    match accept {
        AcceptJsonProtobufBcs::Json => JsonProtobufBcs::Json(committee),
        AcceptJsonProtobufBcs::Protobuf => JsonProtobufBcs::Protobuf(committee.into()),
        AcceptJsonProtobufBcs::Bcs => JsonProtobufBcs::Bcs(committee),
    }
    .pipe(Ok)
}
