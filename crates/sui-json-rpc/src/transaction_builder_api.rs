// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::TransactionBuilderServer;
use crate::SuiRpcModule;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    BigInt, CheckpointId, ObjectsPage, Page, SuiObjectDataOptions, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionBuilderMode, SuiTypeTag, TransactionBytes,
};
use sui_open_rpc::Module;
use sui_transaction_builder::{DataReader, TransactionBuilder};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    messages::TransactionData,
};

use fastcrypto::encoding::Base64;
use jsonrpsee::RpcModule;
use sui_adapter::execution_mode::{DevInspect, Normal};

use crate::api::cap_page_objects_limit;
use crate::error::Error;
use anyhow::anyhow;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::RPCTransactionRequestParams;

pub struct TransactionBuilderApi {
    builder: TransactionBuilder<Normal>,
    dev_inspect_builder: TransactionBuilder<DevInspect>,
}

impl TransactionBuilderApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        let reader = Arc::new(AuthorityStateDataReader::new(state));
        Self {
            builder: TransactionBuilder::new(reader.clone()),
            dev_inspect_builder: TransactionBuilder::new(reader),
        }
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
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> Result<ObjectsPage, anyhow::Error> {
        if at_checkpoint.is_some() {
            return Err(anyhow!("at_checkpoint param currently not supported"));
        }

        let limit = cap_page_objects_limit(limit)?;
        let SuiObjectResponseQuery { filter, options } = query.unwrap_or_default();

        let options = options.unwrap_or_default();

        let mut objects = self
            .0
            .get_owner_objects(address, cursor, limit + 1, filter)?;

        // objects here are of size (limit + 1), where the last one is the cursor for the next page
        let has_next_page = objects.len() > limit;
        objects.truncate(limit);
        let next_cursor = objects
            .last()
            .cloned()
            .map_or(cursor, |o_info| Some(o_info.object_id));

        let data = objects.into_iter().try_fold(vec![], |mut acc, o_info| {
            let o_resp = SuiObjectResponse::try_from((o_info, options.clone()))?;
            acc.push(o_resp);
            Ok::<Vec<SuiObjectResponse>, Error>(acc)
        })?;
        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<SuiObjectResponse, anyhow::Error> {
        let result = self.0.get_object_read(&object_id).await?;
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
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .transfer_object(signer, object_id, gas, gas_budget, recipient)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .transfer_sui(signer, sui_object_id, gas_budget, recipient, amount)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .pay(
                signer,
                input_coins,
                recipients,
                amounts.into_iter().map(|a| a.into()).collect(),
                gas,
                gas_budget,
            )
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<BigInt>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .pay_sui(
                signer,
                input_coins,
                recipients,
                amounts.into_iter().map(|a| a.into()).collect(),
                gas_budget,
            )
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .pay_all_sui(signer, input_coins, recipient, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        dependencies: Vec<ObjectID>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let compiled_modules = compiled_modules
            .into_iter()
            .map(|data| data.to_vec().map_err(|e| anyhow::anyhow!(e)))
            .collect::<Result<Vec<_>, _>>()?;
        let data = self
            .builder
            .publish(sender, compiled_modules, dependencies, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .split_coin_equal(signer, coin_object_id, split_count, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .builder
            .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
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
        gas_budget: u64,
        txn_builder_mode: Option<SuiTransactionBuilderMode>,
    ) -> RpcResult<TransactionBytes> {
        let mode = txn_builder_mode.unwrap_or(SuiTransactionBuilderMode::Commit);
        let data: TransactionData = match mode {
            SuiTransactionBuilderMode::DevInspect => {
                self.dev_inspect_builder
                    .move_call(
                        signer,
                        package_object_id,
                        &module,
                        &function,
                        type_arguments,
                        rpc_arguments,
                        gas,
                        gas_budget,
                    )
                    .await?
            }
            SuiTransactionBuilderMode::Commit => {
                self.builder
                    .move_call(
                        signer,
                        package_object_id,
                        &module,
                        &function,
                        type_arguments,
                        rpc_arguments,
                        gas,
                        gas_budget,
                    )
                    .await?
            }
        };
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
        txn_builder_mode: Option<SuiTransactionBuilderMode>,
    ) -> RpcResult<TransactionBytes> {
        let mode = txn_builder_mode.unwrap_or(SuiTransactionBuilderMode::Commit);
        let data = match mode {
            SuiTransactionBuilderMode::DevInspect => {
                self.dev_inspect_builder
                    .batch_transaction(signer, params, gas, gas_budget)
                    .await?
            }
            SuiTransactionBuilderMode::Commit => {
                self.builder
                    .batch_transaction(signer, params, gas, gas_budget)
                    .await?
            }
        };
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn request_add_stake(
        &self,
        signer: SuiAddress,
        coins: Vec<ObjectID>,
        amount: Option<u64>,
        validator: SuiAddress,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Ok(TransactionBytes::from_data(
            self.builder
                .request_add_stake(signer, coins, amount, validator, gas, gas_budget)
                .await?,
        )?)
    }

    async fn request_withdraw_stake(
        &self,
        signer: SuiAddress,
        staked_sui: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Ok(TransactionBytes::from_data(
            self.builder
                .request_withdraw_stake(signer, staked_sui, gas, gas_budget)
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
