// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ConsistentService::service_config` — returns the
//! [`PaginationConfig`](crate::config::PaginationConfig)
//! defaults so clients know what page sizes / batch sizes to
//! expect.

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::consistent_service::State;

pub(super) fn service_config(state: &State) -> grpc::ServiceConfigResponse {
    let cfg = &*state.pagination;
    grpc::ServiceConfigResponse {
        default_page_size: Some(cfg.default_page_size),
        max_batch_size: Some(cfg.max_batch_size),
        max_page_size: Some(cfg.max_page_size),
    }
}
