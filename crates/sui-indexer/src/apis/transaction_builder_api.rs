// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;

use sui_json::SuiJsonValue;
use sui_json_rpc::api::{TransactionBuilderClient, TransactionBuilderServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    RPCTransactionRequestParams, SuiTransactionBlockBuilderMode, SuiTypeTag, TransactionBlockBytes,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::BigInt;

pub(crate) struct TransactionBuilderApi {
    fullnode: HttpClient,
}

impl TransactionBuilderApi {
    pub fn new(fullnode_client: HttpClient) -> Self {
        Self {
            fullnode: fullnode_client,
        }
    }
}

#[async_trait]
impl TransactionBuilderServer for TransactionBuilderApi {
    async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .transfer_object(signer, object_id, gas, gas_budget, recipient)
            .await
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: BigInt<u64>,
        recipient: SuiAddress,
        amount: Option<BigInt<u64>>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .transfer_sui(signer, sui_object_id, gas_budget, recipient, amount)
            .await
    }

    async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt<u64>>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .pay(signer, input_coins, recipients, amounts, gas, gas_budget)
            .await
    }

    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt<u64>>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .pay_sui(signer, input_coins, recipients, amounts, gas_budget)
            .await
    }

    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .pay_all_sui(signer, input_coins, recipient, gas_budget)
            .await
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        dep_ids: Vec<ObjectID>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .publish(sender, compiled_modules, dep_ids, gas, gas_budget)
            .await
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<BigInt<u64>>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
            .await
    }

    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: BigInt<u64>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .split_coin_equal(signer, coin_object_id, split_count, gas, gas_budget)
            .await
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .merge_coin(signer, primary_coin, coin_to_merge, gas, gas_budget)
            .await
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        rpc_arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        tx_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .move_call(
                signer,
                package_object_id,
                module,
                function,
                type_arguments,
                rpc_arguments,
                gas,
                gas_budget,
                tx_builder_mode,
            )
            .await
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        tx_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .batch_transaction(signer, params, gas, gas_budget, tx_builder_mode)
            .await
    }

    async fn request_add_stake(
        &self,
        signer: SuiAddress,
        coins: Vec<ObjectID>,
        amount: Option<BigInt<u64>>,
        validator: SuiAddress,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .request_add_stake(signer, coins, amount, validator, gas, gas_budget)
            .await
    }

    async fn request_withdraw_stake(
        &self,
        signer: SuiAddress,
        staked_sui: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        self.fullnode
            .request_withdraw_stake(signer, staked_sui, gas, gas_budget)
            .await
    }
}

impl SuiRpcModule for TransactionBuilderApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::TransactionBuilderOpenRpc::module_doc()
    }
}
