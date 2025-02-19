// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::SuiMoveNormalizedFunction;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::ObjectID;

use crate::context::Context;

use super::rpc_module::RpcModule;

mod error;
mod response;

#[open_rpc(namespace = "sui", tag = "Move APIs")]
#[rpc(server, namespace = "sui")]
trait MoveApi {
    #[method(name = "getNormalizedMoveFunction")]
    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction>;
}

pub(crate) struct MoveUtils(pub Context);

#[async_trait::async_trait]
impl MoveApiServer for MoveUtils {
    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction> {
        let Self(ctx) = self;
        Ok(response::function(ctx, package, &module_name, &function_name).await?)
    }
}

impl RpcModule for MoveUtils {
    fn schema(&self) -> Module {
        MoveApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
