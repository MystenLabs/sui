// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::cluster::{Cluster, new_wallet_context_from_cluster};
use async_trait::async_trait;
use fastcrypto::encoding::{Encoding, Hex};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use sui_faucet::{FaucetConfig, FaucetResponse, LocalFaucet, RequestStatus};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::KeypairTraits;
use tracing::{Instrument, debug, info, info_span};

pub struct FaucetClientFactory;

impl FaucetClientFactory {
    pub async fn new_from_cluster(
        cluster: &(dyn Cluster + Sync + Send),
    ) -> Arc<dyn FaucetClient + Sync + Send> {
        match cluster.remote_faucet_url() {
            Some(url) => Arc::new(RemoteFaucetClient::new(url.into())),
            // If faucet_url is none, it's a local cluster
            None => {
                let key = cluster
                    .local_faucet_key()
                    .expect("Expect local faucet key for local cluster")
                    .copy();
                let wallet_context = new_wallet_context_from_cluster(cluster, key)
                    .await
                    .instrument(info_span!("init_wallet_context_for_faucet"));

                let config = FaucetConfig::default();
                let simple_faucet = LocalFaucet::new(wallet_context.into_inner(), config)
                    .await
                    .unwrap();

                Arc::new(LocalFaucetClient::new(simple_faucet))
            }
        }
    }
}

/// Faucet Client abstraction
#[async_trait]
pub trait FaucetClient {
    async fn request_sui_coins(&self, request_address: SuiAddress) -> FaucetResponse;
}

/// Client for a remote faucet that is accessible by POST requests
pub struct RemoteFaucetClient {
    remote_url: String,
}

impl RemoteFaucetClient {
    fn new(url: String) -> Self {
        info!("Use remote faucet: {}", url);
        Self { remote_url: url }
    }
}

#[async_trait]
impl FaucetClient for RemoteFaucetClient {
    /// Request test SUI coins from faucet.
    /// It also verifies the effects are observed by fullnode.
    async fn request_sui_coins(&self, request_address: SuiAddress) -> FaucetResponse {
        let gas_url = format!("{}/v2/gas", self.remote_url);
        debug!("Getting coin from remote faucet {}", gas_url);
        let data = HashMap::from([("recipient", Hex::encode(request_address))]);
        let map = HashMap::from([("FixedAmountRequest", data)]);

        let auth_header = match env::var("FAUCET_AUTH_HEADER") {
            Ok(val) => val,
            _ => "".to_string(),
        };

        let response = reqwest::Client::new()
            .post(&gas_url)
            .header("Authorization", auth_header)
            .json(&map)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to talk to remote faucet {:?}: {:?}", gas_url, e));
        let full_bytes = response.bytes().await.unwrap();
        let faucet_response: FaucetResponse = serde_json::from_slice(&full_bytes)
            .map_err(|e| anyhow::anyhow!("json deser failed with bytes {:?}: {e}", full_bytes))
            .unwrap();

        if let RequestStatus::Failure(error) = &faucet_response.status {
            panic!("Failed to get gas tokens with error: {}", error)
        };

        faucet_response
    }
}

/// A local faucet that holds some coins since genesis
pub struct LocalFaucetClient {
    simple_faucet: Arc<LocalFaucet>,
}

impl LocalFaucetClient {
    fn new(simple_faucet: Arc<LocalFaucet>) -> Self {
        info!("Use local faucet");
        Self { simple_faucet }
    }
}
#[async_trait]
impl FaucetClient for LocalFaucetClient {
    async fn request_sui_coins(&self, request_address: SuiAddress) -> FaucetResponse {
        let coins = self
            .simple_faucet
            .local_request_execute_tx(request_address)
            .await
            .unwrap_or_else(|err| panic!("Failed to get gas tokens with error: {}", err));

        FaucetResponse {
            status: RequestStatus::Success,
            coins_sent: Some(coins),
        }
    }
}
