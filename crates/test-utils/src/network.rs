// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::WsClient;
use jsonrpsee::ws_client::WsClientBuilder;
use prometheus::Registry;
use rand::{distributions::*, rngs::OsRng, seq::SliceRandom};
use tokio::{task::JoinHandle, time::sleep};
use tracing::info;

use mysten_metrics::RegistryService;
use sui::config::SuiEnv;
use sui::{client_commands::WalletContext, config::SuiClientConfig};
use sui_config::genesis_config::GenesisConfig;
use sui_config::{Config, SUI_CLIENT_CONFIG, SUI_NETWORK_CONFIG};
use sui_config::{FullnodeConfigBuilder, NodeConfig, PersistedConfig, SUI_KEYSTORE_FILENAME};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_node::SuiNode;
use sui_node::SuiNodeHandle;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_swarm::memory::{Swarm, SwarmBuilder};
use sui_types::base_types::{AuthorityName, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::SuiKeyPair;

const NUM_VALIDAOTR: usize = 4;

pub struct FullNodeHandle {
    pub sui_node: Arc<SuiNode>,
    pub sui_client: SuiClient,
    pub rpc_client: HttpClient,
    pub rpc_url: String,
    pub ws_client: WsClient,
    pub ws_url: String,
}

pub struct TestCluster {
    pub swarm: Swarm,
    pub accounts: Vec<SuiAddress>,
    pub wallet: WalletContext,
    pub fullnode_handle: FullNodeHandle,
}

impl TestCluster {
    pub fn rpc_client(&self) -> &HttpClient {
        &self.fullnode_handle.rpc_client
    }

    pub fn rpc_url(&self) -> &str {
        &self.fullnode_handle.rpc_url
    }

    pub fn wallet_mut(&mut self) -> &mut WalletContext {
        &mut self.wallet
    }

    // Helper function to get the 0th address in WalletContext
    pub fn get_address_0(&self) -> SuiAddress {
        self.wallet
            .config
            .keystore
            .addresses()
            .get(0)
            .cloned()
            .unwrap()
    }

    // Helper function to get the 1st address in WalletContext
    pub fn get_address_1(&self) -> SuiAddress {
        self.wallet
            .config
            .keystore
            .addresses()
            .get(1)
            .cloned()
            .unwrap()
    }

    pub fn fullnode_config_builder(&self) -> FullnodeConfigBuilder {
        self.swarm.config().fullnode_config_builder()
    }

    /// Convenience method to start a new fullnode in the test cluster.
    pub async fn start_fullnode(&self) -> Result<FullNodeHandle, anyhow::Error> {
        let config = self.fullnode_config_builder().build().unwrap();
        start_fullnode_from_config(config).await
    }

    pub fn get_validator_addresses(&self) -> Vec<AuthorityName> {
        self.swarm.validators().map(|v| v.name()).collect()
    }

    pub fn stop_validator(&self, name: AuthorityName) {
        self.swarm.validator(name).unwrap().stop();
    }

    pub async fn start_validator(&self, name: AuthorityName) {
        let node = self.swarm.validator(name).unwrap();
        if node.is_running() {
            return;
        }
        node.start().await.unwrap();
    }

    pub fn random_node_restarter(self: &Arc<Self>) -> RandomNodeRestarter {
        RandomNodeRestarter::new(self.clone())
    }
}

pub struct RandomNodeRestarter {
    test_cluster: Arc<TestCluster>,

    // How frequently should we kill nodes
    kill_interval: Uniform<Duration>,
    // How long should we wait before restarting them.
    restart_delay: Uniform<Duration>,
}

impl RandomNodeRestarter {
    fn new(test_cluster: Arc<TestCluster>) -> Self {
        Self {
            test_cluster,
            kill_interval: Uniform::new(Duration::from_secs(10), Duration::from_secs(11)),
            restart_delay: Uniform::new(Duration::from_secs(1), Duration::from_secs(2)),
        }
    }

    pub fn with_kill_interval_secs(mut self, a: u64, b: u64) -> Self {
        self.kill_interval = Uniform::new(Duration::from_secs(a), Duration::from_secs(b));
        self
    }

    pub fn with_restart_delay_secs(mut self, a: u64, b: u64) -> Self {
        self.restart_delay = Uniform::new(Duration::from_secs(a), Duration::from_secs(b));
        self
    }

    pub fn run(&self) -> JoinHandle<()> {
        let test_cluster = self.test_cluster.clone();
        let kill_interval = self.kill_interval;
        let restart_delay = self.restart_delay;
        let validators = self.test_cluster.get_validator_addresses();
        tokio::task::spawn(async move {
            loop {
                let delay = kill_interval.sample(&mut OsRng);
                info!("Sleeping {delay:?} before killing a validator");
                sleep(delay).await;

                let validator = validators.choose(&mut OsRng).unwrap();
                info!("Killing validator {:?}", validator.concise());
                test_cluster.stop_validator(*validator);

                let delay = restart_delay.sample(&mut OsRng);
                info!("Sleeping {delay:?} before restarting");
                sleep(delay).await;
                info!("Starting validator {:?}", validator.concise());
                test_cluster.start_validator(*validator).await;
            }
        })
    }
}

pub struct TestClusterBuilder {
    genesis_config: Option<GenesisConfig>,
    num_validators: Option<usize>,
    fullnode_rpc_port: Option<u16>,
    enable_fullnode_events: bool,
    checkpoints_per_epoch: Option<u64>,
}

impl TestClusterBuilder {
    pub fn new() -> Self {
        TestClusterBuilder {
            genesis_config: None,
            fullnode_rpc_port: None,
            num_validators: None,
            enable_fullnode_events: false,
            checkpoints_per_epoch: None,
        }
    }

    pub fn set_fullnode_rpc_port(mut self, rpc_port: u16) -> Self {
        self.fullnode_rpc_port = Some(rpc_port);
        self
    }

    pub fn set_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn with_num_validators(mut self, num: usize) -> Self {
        self.num_validators = Some(num);
        self
    }

    pub fn enable_fullnode_events(mut self) -> Self {
        self.enable_fullnode_events = true;
        self
    }

    pub fn with_checkpoints_per_epoch(mut self, ckpts: u64) -> Self {
        self.checkpoints_per_epoch = Some(ckpts);
        self
    }

    pub async fn build(self) -> anyhow::Result<TestCluster> {
        let cluster = self.start_test_network_with_customized_ports().await?;
        Ok(cluster)
    }

    async fn start_test_network_with_customized_ports(
        mut self,
    ) -> Result<TestCluster, anyhow::Error> {
        let swarm = self.start_swarm().await?;
        let working_dir = swarm.dir();

        let mut wallet_conf: SuiClientConfig =
            PersistedConfig::read(&working_dir.join(SUI_CLIENT_CONFIG))?;

        let fullnode_config = swarm
            .config()
            .fullnode_config_builder()
            .set_event_store(self.enable_fullnode_events)
            .set_rpc_port(self.fullnode_rpc_port)
            .build()
            .unwrap();

        let fullnode_handle = start_fullnode_from_config(fullnode_config).await?;

        wallet_conf.envs.push(SuiEnv {
            alias: "localnet".to_string(),
            rpc: fullnode_handle.rpc_url.clone(),
            ws: Some(fullnode_handle.ws_url.clone()),
        });
        wallet_conf.active_env = Some("localnet".to_string());

        let accounts = wallet_conf.keystore.addresses();

        wallet_conf
            .persisted(&working_dir.join(SUI_CLIENT_CONFIG))
            .save()?;

        let wallet_conf = swarm.dir().join(SUI_CLIENT_CONFIG);
        let wallet = WalletContext::new(&wallet_conf, None).await?;

        Ok(TestCluster {
            swarm,
            accounts,
            wallet,
            fullnode_handle,
        })
    }

    /// Start a Swarm and set up WalletConfig
    async fn start_swarm(&mut self) -> Result<Swarm, anyhow::Error> {
        let mut builder: SwarmBuilder = Swarm::builder().committee_size(
            NonZeroUsize::new(self.num_validators.unwrap_or(NUM_VALIDAOTR)).unwrap(),
        );

        if let Some(genesis_config) = self.genesis_config.take() {
            builder = builder.initial_accounts_config(genesis_config);
        }

        if let Some(checkpoints_per_epoch) = self.checkpoints_per_epoch {
            builder = builder.with_checkpoints_per_epoch(checkpoints_per_epoch);
        }

        let mut swarm = builder.build();
        swarm.launch().await?;

        let dir = swarm.dir();

        let network_path = dir.join(SUI_NETWORK_CONFIG);
        let wallet_path = dir.join(SUI_CLIENT_CONFIG);
        let keystore_path = dir.join(SUI_KEYSTORE_FILENAME);

        swarm.config().save(&network_path)?;
        let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        for key in &swarm.config().account_keys {
            keystore.add_key(SuiKeyPair::Ed25519(key.copy()))?;
        }

        let active_address = keystore.addresses().first().cloned();

        // Create wallet config with stated authorities port
        SuiClientConfig {
            keystore: Keystore::from(FileBasedKeystore::new(&keystore_path)?),
            envs: Default::default(),
            active_address,
            active_env: Default::default(),
        }
        .save(&wallet_path)?;

        // Return network handle
        Ok(swarm)
    }
}

impl Default for TestClusterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn start_fullnode_from_config(
    config: NodeConfig,
) -> Result<FullNodeHandle, anyhow::Error> {
    let registry_service = RegistryService::new(Registry::new());
    let sui_node = SuiNode::start(&config, registry_service).await?;

    let rpc_url = format!("http://{}", config.json_rpc_address);
    let rpc_client = HttpClientBuilder::default().build(&rpc_url)?;

    let ws_url = format!("ws://{}", config.json_rpc_address);
    let ws_client = WsClientBuilder::default().build(&ws_url).await?;
    let sui_client = SuiClientBuilder::default()
        .ws_url(&ws_url)
        .build(&rpc_url)
        .await?;

    Ok(FullNodeHandle {
        sui_node,
        sui_client,
        rpc_client,
        rpc_url,
        ws_client,
        ws_url,
    })
}

pub async fn wait_for_nodes_transition_to_epoch<'a>(
    nodes: impl Iterator<Item = &'a SuiNodeHandle>,
    expected_epoch: EpochId,
) {
    let handles: Vec<_> = nodes
        .map(|handle| {
            handle.with_async(|node| async move {
                let mut rx = node.subscribe_to_epoch_change();
                let epoch = node.current_epoch();
                if epoch != expected_epoch {
                    let committee = rx.recv().await.unwrap();
                    assert_eq!(committee.epoch, expected_epoch);
                }
            })
        })
        .collect();
    join_all(handles).await;
}
