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
use serde::Deserialize;
use serde::Serialize;

use sui_core::authority_client::AuthorityClient;
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayAPI, GatewayClient, GatewayState};
use sui_network::network::NetworkClient;
use sui_network::transport;
use sui_types::base_types::{AuthorityName, ObjectID, ObjectRef, SuiAddress};
use sui_types::committee::Committee;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::ObjectRead;

use crate::config::{AuthorityInfo, Config};
use crate::rest_server_response::{NamedObjectRef, ObjectResponse};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayType {
    Embedded(EmbeddedGatewayConfig),
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
pub struct EmbeddedGatewayConfig {
    pub authorities: Vec<AuthorityInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,
}

impl Config for EmbeddedGatewayConfig {}

impl EmbeddedGatewayConfig {
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

impl Default for EmbeddedGatewayConfig {
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

#[allow(dead_code, unused_variables)]
struct RestGatewayClient {
    url: String,
}

#[async_trait]
#[allow(dead_code, unused_variables)]
impl GatewayAPI for RestGatewayClient {
    async fn execute_transaction(&mut self, tx: Transaction) -> Result<TransactionResponse, Error> {
        todo!()
    }

    async fn transfer_coin(
        &mut self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: SuiAddress,
    ) -> Result<TransactionData, Error> {
        todo!()
    }

    async fn sync_account_state(&mut self, account_addr: SuiAddress) -> Result<(), Error> {
        let url = format!("{}/sync?address={}", self.url, account_addr);
        reqwest::get(url).await?;
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
        todo!()
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
        todo!()
    }

    async fn merge_coins(
        &mut self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        todo!()
    }

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, Error> {
        todo!()
    }

    async fn get_owned_objects(
        &mut self,
        account_addr: SuiAddress,
    ) -> Result<Vec<ObjectRef>, anyhow::Error> {
        let url = format!("{}/objects?address={}", self.url, account_addr);
        let response = reqwest::get(url).await?;
        let response: ObjectResponse = response.json().await?;
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
