// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};

use sui_core::authority_client::AuthorityClient;
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayAPI, GatewayClient, GatewayState};
use sui_network::network::NetworkClient;
use sui_network::transport;
use sui_types::base_types::{encode_bytes_hex, AuthorityName, ObjectID, ObjectRef, SuiAddress};
use sui_types::committee::Committee;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::ObjectRead;

use crate::config::{AuthorityInfo, Config};
use crate::rest_gateway::requests::{CallRequest, MergeCoinRequest, SplitCoinRequest};
use crate::rest_gateway::responses::{NamedObjectRef, ObjectResponse, TransactionBytes};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayType {
    Embedded(GatewayConfig),
    Rest(String),
}

impl Display for GatewayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();

        match self {
            GatewayType::Embedded(config) => {
                writeln!(writer, "Gateway Type : Embedded")?;
                writeln!(
                    writer,
                    "Gateway state DB folder path : {:?}",
                    config.db_folder_path
                )?;
                let authorities = config
                    .authorities
                    .iter()
                    .map(|info| format!("{}:{}", info.host, info.base_port));
                writeln!(
                    writer,
                    "Authorities : {:?}",
                    authorities.collect::<Vec<_>>()
                )?;
            }
            GatewayType::Rest(url) => {
                writeln!(writer, "Gateway Type : RestAPI")?;
                writeln!(writer, "Gateway URL : {}", url)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl GatewayType {
    pub fn init(&self) -> GatewayClient {
        match self {
            GatewayType::Embedded(config) => {
                let path = config.db_folder_path.clone();
                let committee = config.make_committee();
                let authority_clients = config.make_authority_clients();
                Box::new(GatewayState::new(path, committee, authority_clients))
            }
            GatewayType::Rest(url) => Box::new(RestGatewayClient { url: url.clone() }),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub authorities: Vec<AuthorityInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,
}

impl Config for GatewayConfig {}

impl GatewayConfig {
    pub fn make_committee(&self) -> Committee {
        let voting_rights = self
            .authorities
            .iter()
            .map(|authority| (authority.name, 1))
            .collect();
        Committee::new(voting_rights)
    }

    pub fn make_authority_clients(&self) -> BTreeMap<AuthorityName, AuthorityClient> {
        let mut authority_clients = BTreeMap::new();
        for authority in &self.authorities {
            let client = AuthorityClient::new(NetworkClient::new(
                authority.host.clone(),
                authority.base_port,
                self.buffer_size,
                self.send_timeout,
                self.recv_timeout,
            ));
            authority_clients.insert(authority.name, client);
        }
        authority_clients
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            authorities: vec![],
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE,
            db_folder_path: Default::default(),
        }
    }
}

struct RestGatewayClient {
    url: String,
}
#[async_trait]
#[allow(unused_variables)]
impl GatewayAPI for RestGatewayClient {
    async fn execute_transaction(&mut self, tx: Transaction) -> Result<TransactionResponse, Error> {
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
    ) -> Result<TransactionData, Error> {
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

    async fn sync_account_state(&mut self, account_addr: SuiAddress) -> Result<(), Error> {
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
    ) -> Result<TransactionData, Error> {
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
    ) -> Result<TransactionData, Error> {
        todo!()
    }

    async fn split_coin(
        &mut self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
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
    ) -> Result<TransactionData, Error> {
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

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, Error> {
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
            .collect();
        Ok(objects)
    }

    async fn download_owned_objects_not_in_db(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<BTreeSet<ObjectRef>, anyhow::Error> {
        todo!()
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
