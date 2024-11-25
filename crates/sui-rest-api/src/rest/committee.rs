// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    proto,
    reader::StateReader,
    response::JsonProtobufBcs,
    rest::accept::AcceptJsonProtobufBcs,
    rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    RestService, Result,
};
use axum::extract::{Path, State};
use sui_sdk_types::types::{EpochId, ValidatorCommittee};
use sui_types::storage::ReadStore;
use tap::Pipe;

pub struct GetLatestCommittee;

impl ApiEndpoint<RestService> for GetLatestCommittee {
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

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_latest_committee)
    }
}

async fn get_latest_committee(
    accept: AcceptJsonProtobufBcs,
    State(state): State<StateReader>,
) -> Result<JsonProtobufBcs<ValidatorCommittee, proto::ValidatorCommittee, ValidatorCommittee>> {
    let current_epoch = state.inner().get_latest_checkpoint()?.epoch();
    let committee = state
        .get_committee(current_epoch)
        .ok_or_else(|| CommitteeNotFoundError::new(current_epoch))?;

    match accept {
        AcceptJsonProtobufBcs::Json => JsonProtobufBcs::Json(committee),
        AcceptJsonProtobufBcs::Protobuf => JsonProtobufBcs::Protobuf(committee.into()),
        AcceptJsonProtobufBcs::Bcs => JsonProtobufBcs::Bcs(committee),
    }
    .pipe(Ok)
}

pub struct GetCommittee;

impl ApiEndpoint<RestService> for GetCommittee {
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

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_committee)
    }
}

async fn get_committee(
    Path(epoch): Path<EpochId>,
    accept: AcceptJsonProtobufBcs,
    State(state): State<StateReader>,
) -> Result<JsonProtobufBcs<ValidatorCommittee, proto::ValidatorCommittee, ValidatorCommittee>> {
    let committee = state
        .get_committee(epoch)
        .ok_or_else(|| CommitteeNotFoundError::new(epoch))?;

    match accept {
        AcceptJsonProtobufBcs::Json => JsonProtobufBcs::Json(committee),
        AcceptJsonProtobufBcs::Protobuf => JsonProtobufBcs::Protobuf(committee.into()),
        AcceptJsonProtobufBcs::Bcs => JsonProtobufBcs::Bcs(committee),
    }
    .pipe(Ok)
}

#[derive(Debug)]
pub struct CommitteeNotFoundError {
    epoch: EpochId,
}

impl CommitteeNotFoundError {
    pub fn new(epoch: EpochId) -> Self {
        Self { epoch }
    }
}

impl std::fmt::Display for CommitteeNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Committee for epoch {} not found", self.epoch)
    }
}

impl std::error::Error for CommitteeNotFoundError {}

impl From<CommitteeNotFoundError> for crate::RestError {
    fn from(value: CommitteeNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}
