// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::Page as PageResponse;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::SuiAddress;

use crate::{context::Context, error::InternalContext as _};

use super::rpc_module::RpcModule;

use self::error::Error;

mod error;
mod response;

#[open_rpc(namespace = "suix", tag = "Name Service API")]
#[rpc(server, namespace = "suix")]
trait NameServiceApi {
    /// Resolve a SuiNS name to its address
    #[method(name = "resolveNameServiceAddress")]
    async fn resolve_name_service_address(
        &self,
        /// The name to resolve
        name: String,
    ) -> RpcResult<Option<SuiAddress>>;

    /// Find the SuiNS name that points to this address.
    ///
    /// Although this method's response is paginated, it will only ever return at most one name.
    #[method(name = "resolveNameServiceNames")]
    async fn resolve_name_service_names(
        &self,
        /// The address to resolve
        address: SuiAddress,
        /// Unused pagination cursor
        cursor: Option<String>,
        /// Unused pagination limit
        limit: Option<usize>,
    ) -> RpcResult<PageResponse<String, String>>;
}

pub(crate) struct NameService(pub Context);

#[async_trait::async_trait]
impl NameServiceApiServer for NameService {
    async fn resolve_name_service_address(&self, name: String) -> RpcResult<Option<SuiAddress>> {
        let Self(ctx) = self;
        Ok(response::resolved_address(ctx, &name)
            .await
            .with_internal_context(|| format!("Resolving SuiNS name {name:?}"))?)
    }

    async fn resolve_name_service_names(
        &self,
        address: SuiAddress,
        _cursor: Option<String>,
        _limit: Option<usize>,
    ) -> RpcResult<PageResponse<String, String>> {
        let Self(ctx) = self;

        let mut page = PageResponse::empty();
        if let Some(name) = response::resolved_name(ctx, address)
            .await
            .with_internal_context(|| format!("Resolving address {address}"))?
        {
            page.data.push(name);
        }

        Ok(page)
    }
}

impl RpcModule for NameService {
    fn schema(&self) -> Module {
        NameServiceApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
