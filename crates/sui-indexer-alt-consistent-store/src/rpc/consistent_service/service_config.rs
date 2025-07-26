// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::rpc::error::RpcError;

use super::State;

pub(super) fn service_config(
    state: &State,
    grpc::ServiceConfigRequest {}: grpc::ServiceConfigRequest,
) -> Result<grpc::ServiceConfigResponse, RpcError> {
    let config = &state.config.pagination;
    Ok(grpc::ServiceConfigResponse {
        default_page_size: Some(config.default_page_size),
        max_batch_size: Some(config.max_batch_size),
        max_page_size: Some(config.max_page_size),
    })
}
