// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cluster::new_wallet_context_from_cluster;

use super::Cluster;
use sui::client_commands::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{KeypairTraits, Signature};
use sui_types::messages::TransactionData;
use tracing::{info, info_span, Instrument};

pub struct WalletClient {
    wallet_context: WalletContext,
    address: SuiAddress,
    fullnode_client: SuiClient,
}

#[allow(clippy::borrowed_box)]
impl WalletClient {
    pub async fn new_from_cluster(cluster: &(dyn Cluster + Sync + Send)) -> Self {
        let key = cluster.user_key();
        let address: SuiAddress = key.public().into();
        let wallet_context = new_wallet_context_from_cluster(cluster, key)
            .instrument(info_span!("init_wallet_context_for_test_user"))
            .await;

        let fullnode_url = String::from(cluster.fullnode_url());
        info!("Use fullnode: {}", &fullnode_url);
        let fullnode_client = SuiClient::new_rpc_client(&fullnode_url, None)
            .await
            .unwrap();

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
            .wallet_sync_api()
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
