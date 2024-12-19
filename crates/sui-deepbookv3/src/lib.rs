// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_json_rpc_types::Coin;
use sui_json_rpc_types::SuiObjectData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_json_rpc_types::SuiTypeTag;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::transaction::ObjectArg;
use sui_sdk::types::transaction::TransactionKind;
use sui_sdk::{types::base_types::ObjectID, SuiClient};

pub mod client;
pub mod transactions;
pub mod utils;

#[async_trait]
pub trait DataReader {
    async fn get_coin_object(
        &self,
        sender: SuiAddress,
        coin_type: String,
        amount: u64,
    ) -> anyhow::Result<Coin>;
    async fn get_coin_objects(
        &self,
        sender: SuiAddress,
        coin_type: String,
        amount: u64,
    ) -> anyhow::Result<Vec<Coin>>;
    async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<SuiObjectData>;
    async fn coin_object(&self, coin: Coin) -> anyhow::Result<ObjectArg>;
    async fn share_object(&self, object_id: ObjectID) -> anyhow::Result<ObjectArg>;
    async fn share_object_mutable(&self, object_id: ObjectID) -> anyhow::Result<ObjectArg>;
    async fn dev_inspect_transaction(
        &self,
        sender: SuiAddress,
        ptb: ProgrammableTransactionBuilder,
    ) -> anyhow::Result<Vec<(Vec<u8>, SuiTypeTag)>>;
}

#[async_trait]
impl DataReader for SuiClient {
    async fn get_coin_object(
        &self,
        sender: SuiAddress,
        coin_type: String,
        amount: u64,
    ) -> anyhow::Result<Coin> {
        Ok(self
            .get_coin_objects(sender, coin_type, amount)
            .await?
            .first()
            .ok_or_else(|| anyhow::anyhow!("Failed to get base coin"))?
            .clone())
    }

    async fn get_coin_objects(
        &self,
        sender: SuiAddress,
        coin_type: String,
        amount: u64,
    ) -> anyhow::Result<Vec<Coin>> {
        Ok(self
            .coin_read_api()
            .select_coins(sender, Some(coin_type), amount as u128, vec![])
            .await?)
    }

    async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<SuiObjectData> {
        self.read_api()
            .get_object_with_options(object_id, SuiObjectDataOptions::full_content())
            .await?
            .data
            .ok_or(anyhow::anyhow!("Object {} not found", object_id))
    }

    async fn coin_object(&self, coin: Coin) -> anyhow::Result<ObjectArg> {
        Ok(ObjectArg::ImmOrOwnedObject((
            coin.coin_object_id,
            coin.version,
            coin.digest,
        )))
    }

    async fn share_object(&self, object_id: ObjectID) -> anyhow::Result<ObjectArg> {
        let object = self.get_object(object_id).await?;
        Ok(ObjectArg::SharedObject {
            id: object_id,
            initial_shared_version: object.version,
            mutable: false,
        })
    }

    async fn share_object_mutable(&self, object_id: ObjectID) -> anyhow::Result<ObjectArg> {
        let object = self.get_object(object_id).await?;
        Ok(ObjectArg::SharedObject {
            id: object_id,
            initial_shared_version: object.version,
            mutable: true,
        })
    }

    async fn dev_inspect_transaction(
        &self,
        sender: SuiAddress,
        ptb: ProgrammableTransactionBuilder,
    ) -> anyhow::Result<Vec<(Vec<u8>, SuiTypeTag)>> {
        let builder = ptb.finish();
        let dry_run_response = self
            .read_api()
            .dev_inspect_transaction_block(
                sender,
                TransactionKind::ProgrammableTransaction(builder),
                None,
                None,
                None,
            )
            .await?;
        Ok(dry_run_response
            .results
            .ok_or_else(|| anyhow::anyhow!("Failed to get results"))?
            .first()
            .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?
            .return_values
            .clone())
    }
}
