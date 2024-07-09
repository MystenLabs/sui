// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    accept::AcceptFormat,
    openapi::{ApiEndpoint, RouteHandler},
    reader::StateReader,
    response::ResponseContent,
    RestService, Result,
};
use axum::extract::{Path, State};
use sui_sdk2::types::{EpochId, ValidatorCommittee};
use sui_types::storage::ReadStore;
use tap::Pipe;

pub struct GetLatestCommittee;

impl ApiEndpoint<RestService> for GetLatestCommittee {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/committee"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        generator.subschema_for::<ValidatorCommittee>();
        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_latest_committee)
    }
}

async fn get_latest_committee(
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<ValidatorCommittee>> {
    let current_epoch = state.inner().get_latest_checkpoint()?.epoch();
    let committee = state
        .get_committee(current_epoch)?
        .ok_or_else(|| CommitteeNotFoundError::new(current_epoch))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(committee),
        AcceptFormat::Bcs => ResponseContent::Bcs(committee),
    }
    .pipe(Ok)
}

pub struct GetCommittee;

impl ApiEndpoint<RestService> for GetCommittee {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/committee/{epoch}"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        generator.subschema_for::<ValidatorCommittee>();
        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_committee)
    }
}

async fn get_committee(
    Path(epoch): Path<EpochId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<ValidatorCommittee>> {
    let committee = state
        .get_committee(epoch)?
        .ok_or_else(|| CommitteeNotFoundError::new(epoch))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(committee),
        AcceptFormat::Bcs => ResponseContent::Bcs(committee),
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
