// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::api::RpcBcsApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::GetRawObjectDataResponse;
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;

pub struct BcsApiImpl {
    client: Arc<AuthorityState>,
}

impl BcsApiImpl {
    pub fn new(client: Arc<AuthorityState>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RpcBcsApiServer for BcsApiImpl {
    async fn get_raw_object(&self, object_id: ObjectID) -> RpcResult<GetRawObjectDataResponse> {
        Ok(self
            .client
            .get_object_read(&object_id)
            .await
            .map_err(|e| anyhow!("{e}"))?
            .try_into()?)
    }
}

impl SuiRpcModule for BcsApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcBcsApiOpenRpc::module_doc()
    }
}
