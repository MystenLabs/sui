// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{
    DryRunTransactionBlockResponse, SuiTransactionBlock, SuiTransactionBlockEffects,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    digests::ChainIdentifier,
    quorum_driver_types::ExecuteTransactionRequestType,
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
}

pub(crate) struct Read(pub Arc<RwLock<Simulacrum>>);

impl Read {
    pub fn new(simulacrum: Arc<RwLock<Simulacrum>>) -> Self {
        Self(simulacrum)
    }
}

#[async_trait::async_trait]
impl ReadApiServer for Read {
    async fn get_chain_identifier(&self) -> RpcResult<String> {
        let simulacrum = self.0.read().unwrap();
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
}

impl RpcModule for Read {
    fn schema(&self) -> Module {
        ReadApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
