// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::rpc_gateway_client::RpcGatewayClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter, Write},
    path::PathBuf,
    time::Duration,
};
use sui_config::Config;
use sui_config::ValidatorInfo;
use sui_core::gateway_state::GatewayMetrics;
use sui_core::{
    authority_client::NetworkAuthorityClient,
    gateway_state::{GatewayClient, GatewayState},
};
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, EpochId},
    error::SuiResult,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayType {
    Embedded(GatewayConfig),
    RPC(String),
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
                    .validator_set
                    .iter()
                    .map(|info| info.network_address());
                writeln!(
                    writer,
                    "Authorities : {:?}",
                    authorities.collect::<Vec<_>>()
                )?;
            }
            GatewayType::RPC(url) => {
                writeln!(writer, "Gateway Type : JSON-RPC")?;
                writeln!(writer, "Gateway URL : {}", url)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl GatewayType {
    pub fn init(&self) -> Result<GatewayClient, anyhow::Error> {
        Ok(match self {
            GatewayType::Embedded(config) => {
                let path = config.db_folder_path.clone();
                let committee = config.make_committee()?;
                let authority_clients = config.make_authority_clients();
                let metrics = GatewayMetrics::new(&prometheus::Registry::new());
                Arc::new(GatewayState::new(
                    path,
                    committee,
                    authority_clients,
                    metrics,
                )?)
            }
            GatewayType::RPC(url) => Arc::new(RpcGatewayClient::new(url.clone())?),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub epoch: EpochId,
    pub validator_set: Vec<ValidatorInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,
}

impl Config for GatewayConfig {}

impl GatewayConfig {
    pub fn make_committee(&self) -> SuiResult<Committee> {
        let voting_rights = self
            .validator_set
            .iter()
            .map(|validator| (validator.public_key(), validator.stake()))
            .collect();
        Committee::new(self.epoch, voting_rights)
    }

    pub fn make_authority_clients(&self) -> BTreeMap<AuthorityName, NetworkAuthorityClient> {
        let mut authority_clients = BTreeMap::new();
        let mut config = mysten_network::config::Config::new();
        config.connect_timeout = Some(self.send_timeout);
        config.request_timeout = Some(self.recv_timeout);
        for authority in &self.validator_set {
            let channel = config.connect_lazy(authority.network_address()).unwrap();
            let client = NetworkAuthorityClient::new(channel);
            authority_clients.insert(authority.public_key(), client);
        }
        authority_clients
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            epoch: 0,
            validator_set: vec![],
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: 650000,
            db_folder_path: Default::default(),
        }
    }
}
