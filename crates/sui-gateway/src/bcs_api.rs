// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee_core::server::rpc_module::RpcModule;
use jsonrpsee_core::RpcResult;

use sui_core::authority::AuthorityState;
use sui_core::gateway_state::GatewayClient;
use sui_core::gateway_types::GetRawObjectDataResponse;
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;

use crate::api::RpcBcsApiServer;
use crate::api::SuiRpcModule;

pub struct BcsApiImpl {
    client: ClientStateAdaptor,
}

impl BcsApiImpl {
    pub fn new_with_gateway(client: GatewayClient) -> Self {
        Self {
            client: ClientStateAdaptor::Gateway(client),
        }
    }

    pub fn new(client: Arc<AuthorityState>) -> Self {
        Self {
            client: ClientStateAdaptor::FullNode(client),
        }
    }
}

enum ClientStateAdaptor {
    Gateway(GatewayClient),
    FullNode(Arc<AuthorityState>),
}

impl ClientStateAdaptor {
    async fn get_raw_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        match self {
            ClientStateAdaptor::Gateway(client) => client.get_raw_object(object_id).await,
            ClientStateAdaptor::FullNode(client) => client
                .get_object_read(&object_id)
                .await
                .map_err(|e| anyhow!("{e}"))?
                .try_into(),
        }
    }
}

#[async_trait]
impl RpcBcsApiServer for BcsApiImpl {
    async fn get_raw_object(&self, object_id: ObjectID) -> RpcResult<GetRawObjectDataResponse> {
        Ok(self.client.get_raw_object(object_id).await?)
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
