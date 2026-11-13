// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcService;
use crate::ledger_history::object_versions::ObjectVersionFrames;
use crate::reader::NotServedHereCause;
use crate::reader::not_served_here;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;

/// Fullnodes keep no listing of object versions yet, so they answer `UNIMPLEMENTED`
/// (`NOT_SERVED_HERE`, cause `no_object_history`); archival kv-rpc serves `ListObjectVersions`.
pub(super) async fn list_object_versions(
    _service: RpcService,
    _request: ListObjectVersionsRequest,
) -> Result<ObjectVersionFrames> {
    Err(not_served_here(
        NotServedHereCause::NoObjectHistory,
        "fullnodes prune object version history; ListObjectVersions is served by archival \
         kv-rpc",
    ))
}
