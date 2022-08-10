// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{
    cluster::{new_wallet_context_from_cluster, Cluster},
    helper::ObjectChecker,
    wallet_client::WalletClient,
};
use anyhow::bail;
use async_trait::async_trait;
use clap::*;
use std::collections::HashMap;
use std::sync::Arc;
use sui_faucet::{CoinInfo, Faucet, FaucetResponse, SimpleFaucet};
use sui_types::crypto::KeypairTraits;
use sui_types::{
    base_types::{encode_bytes_hex, SuiAddress},
    gas_coin::GasCoin,
    object::Owner,
};
use tokio::time::{sleep, Duration};
use tracing::{debug, info, info_span, Instrument};
use uuid::Uuid;

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
                    .instrument(info_span!("init_wallet_context_for_faucet"))
                    .await;

                let prom_registry = prometheus::Registry::new();
                let simple_faucet = SimpleFaucet::new(wallet_context, &prom_registry)
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
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
        request_address: Option<SuiAddress>,
    ) -> Result<Vec<GasCoin>, anyhow::Error>;
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
    /// It also verifies the effects are observed by gateway/fullnode.
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
        request_address: Option<SuiAddress>,
    ) -> Result<Vec<GasCoin>, anyhow::Error> {
        let gas_url = format!("{}/gas", self.remote_url);
        debug!("Getting coin from remote faucet {}", gas_url);
        let address = request_address.unwrap_or_else(|| client.get_wallet_address());
        let data = HashMap::from([("recipient", encode_bytes_hex(&address))]);
        let map = HashMap::from([("FixedAmountRequest", data)]);

        let response = reqwest::Client::new()
            .post(&gas_url)
            .json(&map)
            .send()
            .await
            .unwrap()
            .json::<FaucetResponse>()
            .await
            .unwrap();

        if let Some(error) = response.error {
            panic!("Failed to get gas tokens with error: {}", error)
        }

        sleep(Duration::from_secs(2)).await;

        let gas_coins =
            into_gas_coin_with_owner_check(response.transferred_gas_objects, address, client).await;

        let minimum_coins = minimum_coins.unwrap_or(5);

        if gas_coins.len() < minimum_coins {
            bail!(
                "Expect to get at least {minimum_coins} Sui Coins for address {address}, but only got {}",
                gas_coins.len()
            )
        }

        Ok(gas_coins)
    }
}

/// A local faucet that holds some coins since genesis
pub struct LocalFaucetClient {
    simple_faucet: SimpleFaucet,
}

impl LocalFaucetClient {
    fn new(simple_faucet: SimpleFaucet) -> Self {
        info!("Use local faucet");
        Self { simple_faucet }
    }
}
#[async_trait]
impl FaucetClient for LocalFaucetClient {
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
        request_address: Option<SuiAddress>,
    ) -> Result<Vec<GasCoin>, anyhow::Error> {
        let address = request_address.unwrap_or_else(|| client.get_wallet_address());
        let receipt = self
            .simple_faucet
            .send(Uuid::new_v4(), address, &[50000; 5])
            .await
            .unwrap_or_else(|err| panic!("Failed to get gas tokens with error: {}", err));

        sleep(Duration::from_secs(2)).await;

        let gas_coins = into_gas_coin_with_owner_check(receipt.sent, address, client).await;

        let minimum_coins = minimum_coins.unwrap_or(5);

        if gas_coins.len() < minimum_coins {
            bail!(
                "Expect to get at least {minimum_coins} Sui Coins for address {address}, but only got {}. Try minting more coins on genesis.",
                gas_coins.len()
            )
        }

        Ok(gas_coins)
    }
}

async fn into_gas_coin_with_owner_check(
    coin_info: Vec<CoinInfo>,
    owner: SuiAddress,
    client: &WalletClient,
) -> Vec<GasCoin> {
    futures::future::join_all(
        coin_info
            .iter()
            .map(|coin_info| {
                ObjectChecker::new(coin_info.id)
                    .owner(Owner::AddressOwner(owner))
                    .check_into_gas_coin(client.get_fullnode())
            })
            .collect::<Vec<_>>(),
    )
    .await
    .into_iter()
    .collect::<Vec<_>>()
}
