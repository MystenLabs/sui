// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use sui_json_rpc::api::CoinReadApiClient;
use sui_json_rpc::api::CoinReadApiServer;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{Balance, CoinPage, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, SuiAddress};

pub(crate) struct CoinReadApi {
    fullnode: HttpClient,
}

impl CoinReadApi {
    pub fn new(fullnode_client: HttpClient) -> Self {
        Self {
            fullnode: fullnode_client,
        }
    }
}

#[async_trait]
impl CoinReadApiServer for CoinReadApi {
    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        self.fullnode
            .get_coins(owner, coin_type, cursor, limit)
            .await
    }

    async fn get_all_coins(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        self.fullnode.get_all_coins(owner, cursor, limit).await
    }

    async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Balance> {
        self.fullnode.get_balance(owner, coin_type).await
    }

    async fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        self.fullnode.get_all_balances(owner).await
    }

    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<Option<SuiCoinMetadata>> {
        self.fullnode.get_coin_metadata(coin_type).await
    }

    async fn get_total_supply(&self, coin_type: String) -> RpcResult<Supply> {
        self.fullnode.get_total_supply(coin_type).await
    }
}

impl SuiRpcModule for CoinReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::CoinReadApiOpenRpc::module_doc()
    }
}
