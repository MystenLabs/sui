// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Cluster;
use sui::client_commands::WalletContext;
use sui::config::{Config, SuiClientConfig};
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_sdk::crypto::KeystoreType;
use sui_sdk::{ClientType, SuiClient};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{KeypairTraits, Signature};
use sui_types::messages::TransactionData;
use tracing::info;

pub struct WalletClient {
    wallet_context: WalletContext,
    address: SuiAddress,
    fullnode_client: SuiClient,
}

#[allow(clippy::borrowed_box)]
impl WalletClient {
    pub async fn new_from_cluster(cluster: &Box<dyn Cluster + Sync + Send>) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let wallet_config_path = temp_dir.path().join("client.yaml");
        let rpc_url = cluster.rpc_url();
        info!("Use gateway: {}", &rpc_url);
        let keystore_path = temp_dir.path().join(SUI_KEYSTORE_FILENAME);
        let keystore = KeystoreType::File(keystore_path);
        let key_pair = cluster.user_key();
        let address: SuiAddress = key_pair.public().into();
        keystore.init().unwrap().add_key(key_pair).unwrap();
        SuiClientConfig {
            keystore,
            gateway: ClientType::RPC(rpc_url.into()),
            active_address: Some(address),
        }
        .persisted(&wallet_config_path)
        .save()
        .unwrap();

        info!(
            "Initialize wallet from config path: {:?}",
            wallet_config_path
        );

        let wallet_context = WalletContext::new(&wallet_config_path)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to init wallet context from path {:?}, error: {e}",
                    wallet_config_path
                )
            });

        let fullnode_url = String::from(cluster.fullnode_url());
        info!("Use fullnode: {}", &fullnode_url);
        let fullnode_client = SuiClient::new_http_client(&fullnode_url).unwrap();

        Self {
            wallet_context,
            address,
            fullnode_client,
        }
    }

    pub fn get_wallet(&self) -> &WalletContext {
        &self.wallet_context
    }

    pub fn get_wallet_mut(&mut self) -> &mut WalletContext {
        &mut self.wallet_context
    }

    pub fn get_wallet_address(&self) -> SuiAddress {
        self.address
    }

    pub fn get_gateway(&self) -> &SuiClient {
        &self.wallet_context.gateway
    }

    pub fn get_fullnode(&self) -> &SuiClient {
        &self.fullnode_client
    }

    pub async fn sync_account_state(&self) -> Result<(), anyhow::Error> {
        self.get_gateway()
            .sync_account_state(self.get_wallet_address())
            .await
    }

    pub fn sign(&self, txn_data: &TransactionData, desc: &str) -> Signature {
        self.get_wallet()
            .keystore
            .sign(&self.address, &txn_data.to_bytes())
            .unwrap_or_else(|e| panic!("Failed to sign transaction for {}. {}", desc, e))
    }
}
