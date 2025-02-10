// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_name_service::NameServiceConfig;
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
}

pub(crate) struct NameService(pub Context, pub NameServiceConfig);

#[async_trait::async_trait]
impl NameServiceApiServer for NameService {
    async fn resolve_name_service_address(&self, name: String) -> RpcResult<Option<SuiAddress>> {
        let Self(ctx, config) = self;
        Ok(response::resolved_address(ctx, config, &name)
            .await
            .with_internal_context(|| format!("Resolving SuiNS name {name:?}"))?)
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
