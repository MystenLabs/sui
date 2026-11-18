// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcService;
use crate::ledger_history::object_versions::ObjectVersionFrames;
use crate::reader::NotServedHereCause;
use crate::reader::not_served_here;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;

/// Fullnodes do not list object versions from their rpc indexes yet, so they answer
/// `UNIMPLEMENTED` (`NOT_SERVED_HERE`, cause `no_rpc_indexes`); archival kv-rpc serves
/// `ListObjectVersions`.
pub(super) async fn list_object_versions(
    _service: RpcService,
    _request: ListObjectVersionsRequest,
) -> Result<ObjectVersionFrames> {
    Err(not_served_here(
        NotServedHereCause::NoRpcIndexes,
        "fullnodes do not list object versions yet; ListObjectVersions is served by archival \
         kv-rpc",
    ))
}
