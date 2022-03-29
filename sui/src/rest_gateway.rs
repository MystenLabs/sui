// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::GatewayAPI;
use sui_types::base_types::{encode_bytes_hex, ObjectID, ObjectRef, SuiAddress};
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::ObjectRead;

use crate::rest_gateway::requests::{
    CallRequest, MergeCoinRequest, PublishRequest, SplitCoinRequest,
};
use crate::rest_gateway::responses::{NamedObjectRef, ObjectResponse, TransactionBytes};
use async_trait::async_trait;
pub mod requests;
pub mod responses;

pub struct RestGatewayClient {
    pub url: String,
}
#[async_trait]
#[allow(unused_variables)]
impl GatewayAPI for RestGatewayClient {
    async fn execute_transaction(
        &mut self,
        tx: Transaction,
    ) -> Result<TransactionResponse, anyhow::Error> {
        let url = format!("{}/api/execute_transaction", self.url);
        let data = tx.data.to_base64();
        let sig_and_pub_key = format!("{:?}", tx.tx_signature);
        let split = sig_and_pub_key.split('@').collect::<Vec<_>>();
        let signature = split[0];
        let pub_key = split[1];

        let body = json!({
            "unsigned_tx_base64" : data,
            "signature" : signature,
            "pub_key" : pub_key
        });
        Ok(Self::post(url, body).await?)
    }

    async fn transfer_coin(
        &mut self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: SuiAddress,
    ) -> Result<TransactionData, anyhow::Error> {
        let url = format!("{}/api/new_transfer", self.url);

        let object_id = object_id.to_hex();
        let gas_payment = gas_payment.to_hex();
        let value = json!({
            "toAddress" : recipient.to_string(),
            "fromAddress" : signer.to_string(),
            "objectId" : object_id,
            "gasObjectId" : gas_payment
        });
        let tx: TransactionBytes = Self::post(url, value).await?;
        Ok(tx.to_data()?)
    }

    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), anyhow::Error> {
        let url = format!("{}/api/sync_account_state", self.url);
        let client = reqwest::Client::new();
        let address = account_addr.to_string();
        let body = json!({ "address": address });

        client.post(url).body(body.to_string()).send().await?;
        Ok(())
    }

    async fn move_call(
        &mut self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        shared_object_arguments: Vec<ObjectID>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let url = format!("{}/api/move_call", self.url);
        let type_arg = type_arguments
            .iter()
            .map(|arg| arg.to_string())
            .collect::<Vec<_>>();

        let request = CallRequest {
            signer: encode_bytes_hex(&signer),
            package_object_id: package_object_ref.0.to_hex(),
            module: module.into_string(),
            function: function.into_string(),
            type_arguments: Some(type_arg),
            pure_arguments,
            gas_object_id: gas_object_ref.0.to_hex(),
            gas_budget,
            object_arguments: vec![],
            shared_object_arguments: vec![],
        };
        let tx: TransactionBytes = Self::post(url, serde_json::to_value(request)?).await?;
        Ok(tx.to_data()?)
    }

    async fn publish(
        &mut self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let url = format!("{}/api/publish", self.url);
        let package_bytes = package_bytes.iter().map(base64::encode).collect::<Vec<_>>();
        let request = PublishRequest {
            sender: encode_bytes_hex(&signer),
            compiled_modules: package_bytes,
            gas_object_id: gas_object_ref.0.to_hex(),
            gas_budget,
        };
        let tx: TransactionBytes = Self::post(url, serde_json::to_value(request)?).await?;
        Ok(tx.to_data()?)
    }

    async fn split_coin(
        &mut self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let url = format!("{}/api/split_coin", self.url);
        let request = SplitCoinRequest {
            signer: encode_bytes_hex(&signer),
            coin_object_id: coin_object_id.to_hex(),
            split_amounts,
            gas_payment: gas_payment.to_hex(),
            gas_budget,
        };
        let tx: TransactionBytes = Self::post(url, serde_json::to_value(request)?).await?;
        Ok(tx.to_data()?)
    }

    async fn merge_coins(
        &mut self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let url = format!("{}/api/merge_coin", self.url);
        let request = MergeCoinRequest {
            signer: encode_bytes_hex(&signer),
            primary_coin: primary_coin.to_hex(),
            coin_to_merge: coin_to_merge.to_hex(),
            gas_payment: gas_payment.to_hex(),
            gas_budget,
        };
        let tx: TransactionBytes = Self::post(url, serde_json::to_value(request)?).await?;
        Ok(tx.to_data()?)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        let url = format!("{}/api/object_info?objectId={}", self.url, object_id);
        Ok(Self::get(url).await?)
    }

    fn get_owned_objects(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<Vec<ObjectRef>, anyhow::Error> {
        let url = format!("{}/api/objects?address={}", self.url, account_addr);
        let response = reqwest::blocking::get(url)?;
        let response: ObjectResponse = response.json()?;
        let objects = response
            .objects
            .into_iter()
            .map(NamedObjectRef::to_object_ref)
            .collect::<Result<Vec<_>, anyhow::Error>>()?;
        Ok(objects)
    }
}

impl RestGatewayClient {
    async fn post<T: DeserializeOwned>(url: String, body: Value) -> Result<T, anyhow::Error> {
        let client = reqwest::Client::new();
        let response = client.post(url).body(body.to_string()).send().await?;
        Ok(response.error_for_status()?.json().await?)
    }

    async fn get<T: DeserializeOwned>(url: String) -> Result<T, anyhow::Error> {
        let response = reqwest::get(url).await?;
        Ok(response.error_for_status()?.json().await?)
    }
}
