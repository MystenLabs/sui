// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{
    DryRunTransactionBlockResponse, ProtocolConfigResponse, SuiObjectDataOptions,
    SuiObjectResponse, SuiTransactionBlock, SuiTransactionBlockEffects,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::ObjectID,
    digests::ChainIdentifier,
    quorum_driver_types::ExecuteTransactionRequestType,
    sui_serde::BigInt,
    supported_protocol_versions::{self, Chain, ProtocolConfig},
    transaction::{Transaction, TransactionData},
};

use sui_indexer_alt_jsonrpc::{api::rpc_module::RpcModule, error::invalid_params};
use sui_types::effects::TransactionEffectsAPI;

use simulacrum::Simulacrum;
use std::sync::{Arc, RwLock};

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

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse>;
}

pub(crate) struct Read {
    pub simulacrum: Arc<RwLock<Simulacrum>>,
    pub protocol_version: u64,
    pub chain: Chain,
}

impl Read {
    pub fn new(simulacrum: Arc<RwLock<Simulacrum>>, protocol_version: u64, chain: Chain) -> Self {
        Self {
            simulacrum,
            protocol_version,
            chain,
        }
    }
}

#[async_trait::async_trait]
impl ReadApiServer for Read {
    async fn get_chain_identifier(&self) -> RpcResult<String> {
        let simulacrum = self.simulacrum.read().unwrap();
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
            ProtocolConfig::get_for_version(self.protocol_version.into(), self.chain);
        let response = ProtocolConfigResponse::from(protocol_config);

        Ok(response)
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        println!("Trying to get object: {}", object_id);
        todo!()
        // let simulacrum = self.simulacrum.read().unwrap();
        // let object = simulacrum
        //     .store()
        //     .get_object(&object_id)
        //     .map_err(|e| invalid_params(format!("Failed to get object: {}", e)))?;
        //
        // let response = SuiObjectResponse::from_object(
        //     object,
        //     options.unwrap_or_default(),
        //     simulacrum.store().as_ref(),
        // )
        // .map_err(|e| invalid_params(format!("Failed to construct object response: {}", e)))?;
        //
        // Ok(response)
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
