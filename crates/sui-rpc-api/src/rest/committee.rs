// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ApiEndpoint, RouteHandler};
use crate::{Result, RpcService};
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
