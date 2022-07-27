// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::config::{ClusterTestOpt, Env};
use async_trait::async_trait;
use clap::*;
use sui_config::genesis_config::GenesisConfig;
use sui_swarm::memory::Node;
use sui_swarm::memory::Swarm;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use test_utils::network::{start_rpc_test_network_with_fullnode, TestNetwork};

const DEVNET_FAUCET_ADDR: &str = "https://faucet.devnet.sui.io:443";
const STAGING_FAUCET_ADDR: &str = "https://faucet.staging.sui.io:443";
const CONTINUOUS_FAUCET_ADDR: &str = "https://faucet.continuous.sui.io:443";
const DEVNET_GATEWAY_ADDR: &str = "https://gateway.devnet.sui.io:443";
const STAGING_GATEWAY_ADDR: &str = "https://gateway.staging.sui.io:443";
const CONTINUOUS_GATEWAY_ADDR: &str = "https://gateway.continuous.sui.io:443";
const DEVNET_FULLNODE_ADDR: &str = "https://fullnode.devnet.sui.io:443";
const STAGING_FULLNODE_ADDR: &str = "https://fullnode.staging.sui.io:443";
const CONTINUOUS_FULLNODE_ADDR: &str = "https://fullnode.continuous.sui.io:443";

pub struct ClusterFactory;

impl ClusterFactory {
    pub async fn start(
        options: &ClusterTestOpt,
    ) -> Result<Box<dyn Cluster + Sync + Send>, anyhow::Error> {
        Ok(match &options.env {
            Env::NewLocal => Box::new(LocalNewCluster::start(options).await?),
            _ => Box::new(RemoteRunningCluster::start(options).await?),
        })
    }
}

/// Cluster Abstraction
#[async_trait]
pub trait Cluster {
    async fn start(options: &ClusterTestOpt) -> Result<Self, anyhow::Error>
    where
        Self: Sized;

    fn rpc_url(&self) -> &str;
    fn faucet_url(&self) -> Option<&str>;
    fn fullnode_url(&self) -> &str;
    fn user_key(&self) -> AccountKeyPair;
}

/// Represents an up and running cluster deployed remotely.
pub struct RemoteRunningCluster {
    rpc_url: String,
    faucet_url: String,
    fullnode_url: String,
}

#[async_trait]
impl Cluster for RemoteRunningCluster {
    async fn start(options: &ClusterTestOpt) -> Result<Self, anyhow::Error> {
        let (rpc_url, faucet_url, fullnode_url) = match options.env {
            Env::DevNet => (
                String::from(DEVNET_GATEWAY_ADDR),
                String::from(DEVNET_FAUCET_ADDR),
                String::from(DEVNET_FULLNODE_ADDR),
            ),
            Env::Staging => (
                String::from(STAGING_GATEWAY_ADDR),
                String::from(STAGING_FAUCET_ADDR),
                String::from(STAGING_FULLNODE_ADDR),
            ),
            Env::Continuous => (
                String::from(CONTINUOUS_GATEWAY_ADDR),
                String::from(CONTINUOUS_FAUCET_ADDR),
                String::from(CONTINUOUS_FULLNODE_ADDR),
            ),
            Env::CustomRemote => (
                options
                    .gateway_address
                    .clone()
                    .expect("Expect 'gateway_address' for Env::Custom"),
                options
                    .faucet_address
                    .clone()
                    .expect("Expect 'faucet_address' for Env::Custom"),
                options
                    .fullnode_address
                    .clone()
                    .expect("Expect 'fullnode_address' for Env::Custom"),
            ),
            Env::NewLocal => unreachable!("NewLocal shouldn't use RemoteRunningCluster"),
        };

        // TODO: test connectivity before proceeding?

        Ok(Self {
            rpc_url,
            faucet_url,
            fullnode_url,
        })
    }
    fn rpc_url(&self) -> &str {
        &self.rpc_url
    }
    fn fullnode_url(&self) -> &str {
        &self.fullnode_url
    }
    fn faucet_url(&self) -> Option<&str> {
        Some(&self.faucet_url)
    }
    fn user_key(&self) -> AccountKeyPair {
        get_key_pair().1
    }
}

/// Represents a local Cluster which starts per cluster test run.
pub struct LocalNewCluster {
    test_network: TestNetwork,
    fullnode_url: String,
}

impl LocalNewCluster {
    fn swarm(&self) -> &Swarm {
        &self.test_network.network
    }
}

#[async_trait]
impl Cluster for LocalNewCluster {
    async fn start(_options: &ClusterTestOpt) -> Result<Self, anyhow::Error> {
        let genesis_config = GenesisConfig::for_local_testing();

        let test_network = start_rpc_test_network_with_fullnode(Some(genesis_config), 1)
            .await
            .unwrap_or_else(|e| panic!("Failed to start a local network, e: {e}"));
        let fullnode: &Node = test_network
            .network
            .fullnodes()
            .next()
            .expect("Expect one fullnode");
        let fullnode_url = format!("http://{}", fullnode.json_rpc_address());

        // Let nodes connect to one another
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // TODO: test connectivity before proceeding?
        Ok(Self {
            test_network,
            fullnode_url,
        })
    }

    fn rpc_url(&self) -> &str {
        &self.test_network.rpc_url
    }

    fn fullnode_url(&self) -> &str {
        &self.fullnode_url
    }

    // For now, a local cluster does not have faucet
    fn faucet_url(&self) -> Option<&str> {
        None
    }

    fn user_key(&self) -> AccountKeyPair {
        self.swarm().config().account_keys[0].copy()
    }
}
