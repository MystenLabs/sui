// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{
    config::{ClusterTestOpt, Env},
    helper::verify_gas_coin,
    wallet_client::WalletClient,
};
use anyhow::bail;
use async_trait::async_trait;
use clap::*;
use std::collections::HashMap;
use std::sync::Arc;
use sui_faucet::FaucetResponse;
use sui_types::{base_types::encode_bytes_hex, gas_coin::GasCoin, object::Owner};
use tokio::time::{sleep, Duration};
use tracing::{debug, info};

pub struct FaucetFactory;

impl FaucetFactory {
    pub fn create(
        options: &ClusterTestOpt,
        faucet_url: Option<String>,
    ) -> Arc<dyn Faucet + Sync + Send> {
        match (&options.env, faucet_url) {
            (Env::NewLocal, None) => Arc::new(DummyFaucet::new()),
            (Env::Prod, Some(url)) => Arc::new(RemoteFaucet::new(url)),
            (Env::Staging, Some(url)) => Arc::new(RemoteFaucet::new(url)),
            (Env::CustomRemote, Some(url)) => Arc::new(RemoteFaucet::new(url)),
            _ => panic!("Unallowed combination of parameters."),
        }
    }
}

/// Faucet Abstraction for cluster test
#[async_trait]
pub trait Faucet {
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
    ) -> Result<Vec<GasCoin>, anyhow::Error>;
}

/// Client for a remote faucet that is accessible by
/// POST requests
pub struct RemoteFaucet {
    remote_url: String,
}

impl RemoteFaucet {
    fn new(url: String) -> Self {
        info!("Use remote faucet: {}", url);
        Self { remote_url: url }
    }
}

#[async_trait]
impl Faucet for RemoteFaucet {
    /// Request test SUI coins from facuet.
    /// It also verifies the effects are observed by gateway/fullnode.
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
    ) -> Result<Vec<GasCoin>, anyhow::Error> {
        let gas_url = format!("{}/gas", self.remote_url);
        debug!("Getting coin from remote faucet {}", gas_url);
        let address = client.get_wallet_address();
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

        // Let fullnode sync
        sleep(Duration::from_secs(2)).await;

        let gas_coins = futures::future::join_all(
            response
                .transferred_gas_objects
                .iter()
                .map(|coin_info| {
                    verify_gas_coin(
                        client.get_fullnode(),
                        coin_info.id,
                        Owner::AddressOwner(address),
                        false,
                        true,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .map(|o| o.unwrap().expect("Expect object to be active but deleted."))
        .collect::<Vec<_>>();

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

/// A dummy faucet that does nothing, suitable for local cluster testing
/// where gas coins are prepared in genesis
pub struct DummyFaucet {}

impl DummyFaucet {
    fn new() -> Self {
        info!("Use dummy faucet");
        Self {}
    }
}
#[async_trait]
impl Faucet for DummyFaucet {
    /// Dummy faucet client does not request coins from a real faucet.
    /// Instead it just syncs all gas objects for the address.
    async fn request_sui_coins(
        &self,
        client: &WalletClient,
        minimum_coins: Option<usize>,
    ) -> Result<Vec<GasCoin>, anyhow::Error> {
        let wallet = client.get_wallet();
        let address = client.get_wallet_address();
        client.sync_account_state().await?;
        let gas_coins = wallet
            .gas_objects(address)
            .await?
            .iter()
            .map(|(_amount, o)| GasCoin::try_from(o).unwrap())
            .collect::<Vec<_>>();

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
