// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{EpochInfo, EpochPage};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::EpochId;

#[open_rpc(namespace = "suix", tag = "Extended API")]
#[rpc(server, client, namespace = "suix")]
pub trait ExtendedApi {
    /// Return a list of epoch info
    #[method(name = "getEpochs")]
    async fn get_epoch(
        &self,
        /// optional paging cursor
        cursor: Option<EpochId>,
        /// maximum number of items per page
        limit: Option<usize>,
    ) -> RpcResult<EpochPage>;

    /// Return current epoch info
    #[method(name = "getCurrentEpoch")]
    async fn get_current_epoch(&self) -> RpcResult<EpochInfo>;

    /// Return total address count
    #[method(name = "getTotalAddresses")]
    async fn get_total_addresses(&self) -> RpcResult<u64>;

    /// Return total object count
    #[method(name = "getTotalObjects")]
    async fn get_total_objects(&self) -> RpcResult<u64>;

    /// Return total package count
    #[method(name = "getTotalPackages")]
    async fn get_total_packages(&self) -> RpcResult<u64>;
}
