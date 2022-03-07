use crate::config::AuthorityInfo;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{ClientAddressManager, GatewayClient};
use sui_network::network::NetworkClient;
use sui_network::transport;
use sui_types::base_types::AuthorityName;
use sui_types::committee::Committee;

#[derive(Serialize, Deserialize)]
pub enum GatewayType {
    Local(LocalGatewayConfig),
    Rest(String),
}

impl GatewayType {
    pub fn init(&self) -> GatewayClient {
        match self {
            GatewayType::Local(config) => {
                let path = config.db_folder_path.clone();
                let committee = config.make_committee();
                let authority_clients = config.make_authority_clients();
                Box::new(ClientAddressManager::new(
                    path,
                    committee,
                    authority_clients,
                ))
            }
            _ => {
                panic!("Unsupported gatway type")
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LocalGatewayConfig {
    pub authorities: Vec<AuthorityInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,
}

impl LocalGatewayConfig {
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

impl Default for LocalGatewayConfig {
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
