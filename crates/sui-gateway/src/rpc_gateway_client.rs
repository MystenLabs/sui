// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use async_trait::async_trait;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use move_core_types::language_storage::TypeTag;
use tokio::runtime::Handle;

use sui_core::gateway_state::{GatewayAPI, GatewayTxSeqNumber};
use sui_core::gateway_types::{
    GetObjectDataResponse, SuiObjectInfo, TransactionEffectsResponse, TransactionResponse,
};
use sui_json::SuiJsonValue;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::messages::{Transaction, TransactionData};
use sui_types::sui_serde::Base64;

use crate::api::RpcReadApiClient;
use crate::api::RpcTransactionBuilderClient;
use crate::api::{RpcGatewayApiClient, TransactionBytes};

pub struct RpcGatewayClient {
    client: HttpClient,
}

impl RpcGatewayClient {
    pub fn new(server_url: String) -> Result<Self, anyhow::Error> {
        let client = HttpClientBuilder::default().build(&server_url)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl GatewayAPI for RpcGatewayClient {
    async fn execute_transaction(&self, tx: Transaction) -> Result<TransactionResponse, Error> {
        let signature = tx.tx_signature;
        let tx_bytes = Base64::from_bytes(&tx.data.to_bytes());
        let signature_bytes = Base64::from_bytes(signature.signature_bytes());
        let pub_key = Base64::from_bytes(signature.public_key_bytes());

        Ok(self
            .client
            .execute_transaction(tx_bytes, signature_bytes, pub_key)
            .await?)
    }

    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .transfer_coin(signer, object_id, gas, gas_budget, recipient)
            .await?;
        bytes.to_data()
    }

    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), Error> {
        self.client.sync_account_state(account_addr).await?;
        Ok(())
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .move_call(
                signer,
                package_object_id,
                module,
                function,
                type_arguments
                    .into_iter()
                    .map(|tag| tag.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
                arguments,
                gas,
                gas_budget,
            )
            .await?;
        bytes.to_data()
    }

    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let package_bytes = package_bytes
            .iter()
            .map(|bytes| Base64::from_bytes(bytes))
            .collect();
        let bytes: TransactionBytes = self
            .client
            .publish(signer, package_bytes, gas, gas_budget)
            .await?;
        bytes.to_data()
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
            .await?;
        bytes.to_data()
    }

    async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .merge_coin(signer, primary_coin, coin_to_merge, gas, gas_budget)
            .await?;
        bytes.to_data()
    }

    async fn get_object(&self, object_id: ObjectID) -> Result<GetObjectDataResponse, Error> {
        Ok(self.client.get_object_info(object_id).await?)
    }

    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, Error> {
        Ok(self.client.get_objects_owned_by_address(address).await?)
    }

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> Result<Vec<SuiObjectInfo>, Error> {
        Ok(self.client.get_objects_owned_by_object(object_id).await?)
    }

    fn get_total_transaction_number(&self) -> Result<u64, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_total_transaction_number(),
        )?)
    }

    fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_transactions_in_range(start, end),
        )?)
    }

    fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_recent_transactions(count),
        )?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, Error> {
        Ok(self.client.get_transaction(digest).await?)
    }
}
