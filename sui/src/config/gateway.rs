// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{AuthorityInfo, Config},
    rpc_gateway_client::RpcGatewayClient,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter, Write},
    path::PathBuf,
    time::Duration,
};
use sui_core::{
    authority_client::NetworkAuthorityClient,
    gateway_state::{GatewayClient, GatewayState},
};
use sui_network::network::NetworkClient;
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, EpochId},
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
                    .authorities
                    .iter()
                    .map(|info| format!("{}:{}", info.host, info.port));
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
                let committee = config.make_committee();
                let authority_clients = config.make_authority_clients();
                Box::new(GatewayState::new(path, committee, authority_clients)?)
            }
            GatewayType::RPC(url) => Box::new(RpcGatewayClient::new(url.clone())?),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub epoch: EpochId,
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
            .map(|authority| (authority.public_key, 1))
            .collect();
        Committee::new(self.epoch, voting_rights)
    }

    pub fn make_authority_clients(&self) -> BTreeMap<AuthorityName, NetworkAuthorityClient> {
        let mut authority_clients = BTreeMap::new();
        for authority in &self.authorities {
            let client = NetworkAuthorityClient::new(NetworkClient::new(
                authority.host.clone(),
                authority.port,
                self.buffer_size,
                self.send_timeout,
                self.recv_timeout,
            ));
            authority_clients.insert(authority.public_key, client);
        }
        authority_clients
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            epoch: 0,
            authorities: vec![],
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: 650000,
            db_folder_path: Default::default(),
        }
    }
}
