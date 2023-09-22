// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove after the functions are implemented
#![allow(unused_variables)]
#![allow(dead_code)]

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use sui_json::SuiJsonValue;
use sui_json_rpc::api::TransactionBuilderServer;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    RPCTransactionRequestParams, SuiTransactionBlockBuilderMode, SuiTypeTag, TransactionBlockBytes,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::BigInt;

use crate::store::PgIndexerStoreV2;

pub(crate) struct TransactionBuilderApiV2 {
    pg_store: PgIndexerStoreV2,
}

impl TransactionBuilderApiV2 {
    pub fn new(pg_store: PgIndexerStoreV2) -> Self {
        Self { pg_store }
    }
}

#[async_trait]
impl TransactionBuilderServer for TransactionBuilderApiV2 {
    async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: BigInt<u64>,
        recipient: SuiAddress,
        amount: Option<BigInt<u64>>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
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
        unimplemented!()
    }

    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt<u64>>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        dep_ids: Vec<ObjectID>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<BigInt<u64>>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: BigInt<u64>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
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
        unimplemented!()
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        tx_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
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
        unimplemented!()
    }

    async fn request_withdraw_stake(
        &self,
        signer: SuiAddress,
        staked_sui: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        unimplemented!()
    }
}

impl SuiRpcModule for TransactionBuilderApiV2 {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::TransactionBuilderOpenRpc::module_doc()
    }
}
