// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use sui_indexer_alt_jsonrpc::api::rpc_module::RpcModule;
use sui_json_rpc_types::ProtocolConfigResponse;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    digests::ChainIdentifier,
    sui_serde::BigInt,
    supported_protocol_versions::ProtocolConfig,
};


#[open_rpc(namespace = "sui", tag = "Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait ReadApi {
    #[method(name = "getChainIdentifier")]
    async fn get_chain_identifier(&self) -> RpcResult<String>;

    #[method(name = "getProtocolConfig")]
    async fn get_protocol_config(
        &self,
        /// An optional protocol version specifier. If omitted, the latest protocol config table for the node will be returned.
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse>;
}

pub(crate) struct Read(pub crate::context::Context);

#[async_trait::async_trait]
impl ReadApiServer for Read {
    async fn get_chain_identifier(&self) -> RpcResult<String> {
        let simulacrum = self.0.simulacrum.read().await;
        let chain_id: ChainIdentifier = simulacrum
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest()
            .to_owned()
            .into();
        let chain_id = chain_id.to_string();
        Ok(chain_id)
    }

    async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse> {
        let protocol_config =
            ProtocolConfig::get_for_version(self.0.protocol_version.into(), self.0.chain);
        let response = ProtocolConfigResponse::from(protocol_config);

        Ok(response)
    }
}

impl RpcModule for Read {
    fn schema(&self) -> Module {
        ReadApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
