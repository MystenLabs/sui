// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::language_storage::StructTag;

use sui_core::authority::AuthorityState;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{RPCTransactionRequestParams, SuiObjectDataFilter};
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponse, SuiTransactionBlockBuilderMode, SuiTypeTag,
    TransactionBlockBytes,
};
use sui_open_rpc::Module;
use sui_transaction_builder::{DataReader, TransactionBuilder};
use sui_types::base_types::ObjectInfo;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::BigInt;

use crate::api::TransactionBuilderServer;
use crate::SuiRpcModule;

pub struct TransactionBuilderApi(TransactionBuilder);

impl TransactionBuilderApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        let reader = Arc::new(AuthorityStateDataReader::new(state));
        Self(TransactionBuilder::new(reader))
    }
}

pub struct AuthorityStateDataReader(Arc<AuthorityState>);

impl AuthorityStateDataReader {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self(state)
    }
}

#[async_trait]
impl DataReader for AuthorityStateDataReader {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> Result<Vec<ObjectInfo>, anyhow::Error> {
        Ok(self
            .0
            // DataReader is used internally, don't need a limit
            .get_owner_objects_iterator(
                address,
                None,
                Some(SuiObjectDataFilter::StructType(object_type)),
            )?
            .collect())
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<SuiObjectResponse, anyhow::Error> {
        let result = self.0.get_object_read(&object_id)?;
        Ok((result, options).try_into()?)
    }

    async fn get_reference_gas_price(&self) -> Result<u64, anyhow::Error> {
        let epoch_store = self.0.load_epoch_store_one_call_per_task();
        Ok(epoch_store.reference_gas_price())
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
        let data = self
            .0
            .transfer_object(signer, object_id, gas, *gas_budget, recipient)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: BigInt<u64>,
        recipient: SuiAddress,
        amount: Option<BigInt<u64>>,
    ) -> RpcResult<TransactionBlockBytes> {
        let data = self
            .0
            .transfer_sui(
                signer,
                sui_object_id,
                *gas_budget,
                recipient,
                amount.map(|a| *a),
            )
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
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
        let data = self
            .0
            .pay(
                signer,
                input_coins,
                recipients,
                amounts.into_iter().map(|a| *a).collect(),
                gas,
                *gas_budget,
            )
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt<u64>>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let data = self
            .0
            .pay_sui(
                signer,
                input_coins,
                recipients,
                amounts.into_iter().map(|a| *a).collect(),
                *gas_budget,
            )
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let data = self
            .0
            .pay_all_sui(signer, input_coins, recipient, *gas_budget)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        dependencies: Vec<ObjectID>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let compiled_modules = compiled_modules
            .into_iter()
            .map(|data| data.to_vec().map_err(|e| anyhow::anyhow!(e)))
            .collect::<Result<Vec<_>, _>>()?;
        let data = self
            .0
            .publish(sender, compiled_modules, dependencies, gas, *gas_budget)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<BigInt<u64>>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let split_amounts = split_amounts.into_iter().map(|a| *a).collect();
        let data = self
            .0
            .split_coin(signer, coin_object_id, split_amounts, gas, *gas_budget)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: BigInt<u64>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let data = self
            .0
            .split_coin_equal(signer, coin_object_id, *split_count, gas, *gas_budget)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        let data = self
            .0
            .merge_coins(signer, primary_coin, coin_to_merge, gas, *gas_budget)
            .await?;
        Ok(TransactionBlockBytes::from_data(data)?)
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
        _txn_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes> {
        Ok(TransactionBlockBytes::from_data(
            self.0
                .move_call(
                    signer,
                    package_object_id,
                    &module,
                    &function,
                    type_arguments,
                    rpc_arguments,
                    gas,
                    *gas_budget,
                )
                .await?,
        )?)
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
        _txn_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes> {
        Ok(TransactionBlockBytes::from_data(
            self.0
                .batch_transaction(signer, params, gas, *gas_budget)
                .await?,
        )?)
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
        let amount = amount.map(|a| *a);
        Ok(TransactionBlockBytes::from_data(
            self.0
                .request_add_stake(signer, coins, amount, validator, gas, *gas_budget)
                .await?,
        )?)
    }

    async fn request_withdraw_stake(
        &self,
        signer: SuiAddress,
        staked_sui: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes> {
        Ok(TransactionBlockBytes::from_data(
            self.0
                .request_withdraw_stake(signer, staked_sui, gas, *gas_budget)
                .await?,
        )?)
    }
}

impl SuiRpcModule for TransactionBuilderApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::TransactionBuilderOpenRpc::module_doc()
    }
}
