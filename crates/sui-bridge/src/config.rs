// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::crypto::BridgeAuthorityKeyPair;
use crate::eth_client::EthClient;
use crate::sui_client::SuiClient;
use anyhow::anyhow;
use fastcrypto::traits::EncodeDecodeBase64;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::Config;
use sui_sdk::SuiClient as SuiSdkClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::SuiKeyPair;
use sui_types::object::Owner;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeNodeConfig {
    /// The port that the server listens on.
    pub server_listen_port: u16,
    /// The port that for metrics server.
    pub metrics_port: u16,
    /// Path of the file where bridge authority key (Secp256k1) is stored as Base64 encoded `privkey`.
    pub bridge_authority_key_path_base64_raw: PathBuf,
    /// Path of the file where bridge client key (any SuiKeyPair) is stored as Base64 encoded `flag || privkey`.
    /// If `run_client` is true, and this is None, then use `bridge_authority_key_path_base64_raw` as client key.
    pub bridge_client_key_path_base64_sui_key: Option<PathBuf>,
    /// Whether to run client. If true, `bridge_client_key_path_base64_sui_key` and
    /// `bridge_client_gas_object` needs to be provided.
    pub run_client: bool,
    /// The gas object to use for paying for gas fees for the client. It needs to
    /// be owned by the address associated with bridge client key.
    // Why is this Option? When should it be None?
    pub bridge_client_gas_object: Option<ObjectID>,
    /// Rpc url for Sui fullnode, used for query stuff and submit transactions.
    pub sui_rpc_url: String,
    /// Rpc url for Eth fullnode, used for query stuff.
    pub eth_rpc_url: String,
}

impl Config for BridgeNodeConfig {}

impl BridgeNodeConfig {
    pub async fn validate(
        &self,
    ) -> anyhow::Result<(BridgeServerConfig, Option<BridgeClientConfig>)> {
        let bridge_authority_key =
            read_bridge_authority_key(&self.bridge_authority_key_path_base64_raw)?;

        // TODO: verify it's part of bridge committee

        let sui_client = Arc::new(SuiClient::<SuiSdkClient>::new(&self.sui_rpc_url).await?);
        let eth_client =
            Arc::new(EthClient::<ethers::providers::Http>::new(&self.eth_rpc_url).await?);

        let bridge_server_config = BridgeServerConfig {
            key: bridge_authority_key,
            metrics_port: self.metrics_port,
            server_listen_port: self.server_listen_port,
            sui_client: sui_client.clone(),
            eth_client: eth_client.clone(),
        };

        if !self.run_client {
            return Ok((bridge_server_config, None));
        }
        // If client is enabled, prepare client config
        let bridge_client_key = if self.bridge_client_key_path_base64_sui_key.is_none() {
            let bridge_client_key =
                read_bridge_authority_key(&self.bridge_authority_key_path_base64_raw)?;
            Ok(SuiKeyPair::from(bridge_client_key))
        } else {
            read_bridge_client_key(self.bridge_client_key_path_base64_sui_key.as_ref().unwrap())
        }?;

        let client_sui_address = SuiAddress::from(&bridge_client_key.public());
        let gas_object_id = self.bridge_client_gas_object.ok_or(anyhow!(
            "`bridge_client_gas_object` is required when `run_client` is true"
        ))?;

        // TODO log gas balance
        let (gas_object_ref, owner) = sui_client.get_gas_object_ref_and_owner(gas_object_id).await;
        if owner != Owner::AddressOwner(client_sui_address) {
            return Err(anyhow!("Gas object {:?} is not owned by bridge client key's associated sui address {:?}, but {:?}", gas_object_id, client_sui_address, owner));
        }
        let bridge_client_config = BridgeClientConfig {
            sui_address: client_sui_address,
            key: bridge_client_key,
            gas_object_ref,
            metrics_port: self.metrics_port,
            sui_client: sui_client.clone(),
            eth_client: eth_client.clone(),
        };

        Ok((bridge_server_config, Some(bridge_client_config)))
    }
}

pub struct BridgeServerConfig {
    pub key: BridgeAuthorityKeyPair,
    pub server_listen_port: u16,
    pub metrics_port: u16,
    pub sui_client: Arc<SuiClient<SuiSdkClient>>,
    pub eth_client: Arc<EthClient<ethers::providers::Http>>,
}

// TODO: add gas balance alert threshold
pub struct BridgeClientConfig {
    pub sui_address: SuiAddress,
    pub key: SuiKeyPair,
    pub gas_object_ref: ObjectRef,
    pub metrics_port: u16,
    pub sui_client: Arc<SuiClient<SuiSdkClient>>,
    pub eth_client: Arc<EthClient<ethers::providers::Http>>,
}

/// Read Bridge Authority key (Secp256k1KeyPair) from a file.
/// BridgeAuthority key is stored as base64 encoded `privkey`.
pub fn read_bridge_authority_key(path: &PathBuf) -> Result<BridgeAuthorityKeyPair, anyhow::Error> {
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Bridge authority key file not found at path: {:?}",
            path
        ));
    }
    let contents = std::fs::read_to_string(path)?;

    BridgeAuthorityKeyPair::decode_base64(contents.as_str().trim())
        .map_err(|e| anyhow!("Error decoding authority key: {:?}", e))
}

/// Read Bridge client key (any SuiKeyPair) from a file.
/// Read from file as Base64 encoded `flag || privkey`.
pub fn read_bridge_client_key(path: &PathBuf) -> Result<SuiKeyPair, anyhow::Error> {
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Bridge client key file not found at path: {:?}",
            path
        ));
    }
    let contents = std::fs::read_to_string(path)?;

    SuiKeyPair::decode_base64(contents.as_str().trim())
        .map_err(|e| anyhow!("Error decoding authority key: {:?}", e))
}
