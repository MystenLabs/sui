// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::{accept::AcceptFormat, response::ResponseContent};
use crate::{RestService, Result};
use axum::extract::State;
use sui_types::digests::ChainIdentifier;
use tap::Pipe;

pub async fn node_info(
    accept: AcceptFormat,
    State(state): State<RestService>,
) -> Result<ResponseContent<NodeInfo>> {
    let latest_checkpoint = state.store.get_latest_checkpoint()?;
    let oldest_checkpoint = state.store.get_lowest_available_checkpoint()?;

    let response = NodeInfo {
        checkpoint_height: latest_checkpoint.sequence_number,
        oldest_checkpoint_height: oldest_checkpoint,
        timestamp_ms: latest_checkpoint.timestamp_ms,
        epoch: latest_checkpoint.epoch(),
        chain_id: state.chain_id(),
        software_version: state.software_version().into(),
    };

    match accept {
        AcceptFormat::Json => ResponseContent::Json(response),
        AcceptFormat::Bcs => ResponseContent::Bcs(response),
    }
    .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NodeInfo {
    pub chain_id: ChainIdentifier,
    pub epoch: u64,
    pub checkpoint_height: u64,
    pub timestamp_ms: u64,
    pub oldest_checkpoint_height: u64,
    pub software_version: Cow<'static, str>,
    //TODO include current protocol version
}
