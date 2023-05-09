// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::WsClient;
use jsonrpsee::ws_client::WsClientBuilder;
use mysten_metrics::RegistryService;
use prometheus::Registry;
use rand::{distributions::*, rngs::OsRng, seq::SliceRandom};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::{DBCheckpointConfig, DEFAULT_VALIDATOR_GAS_PRICE};
use sui_config::{sui_cluster_test_config_dir, Config, SUI_CLIENT_CONFIG, SUI_NETWORK_CONFIG};
use sui_config::{NodeConfig, PersistedConfig, SUI_KEYSTORE_FILENAME};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::{SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_node::SuiNode;
use sui_node::SuiNodeHandle;
use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
use sui_sdk::error::SuiRpcResult;
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_swarm::memory::{Swarm, SwarmBuilder};
use sui_swarm_config::genesis_config::{
    AccountConfig, GenesisConfig, ValidatorGenesisConfig, DEFAULT_GAS_AMOUNT,
};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::{
    ConfigBuilder, FullnodeConfigBuilder, ProtocolVersionsConfig, SupportedProtocolVersionsCallback,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{AuthorityName, ObjectID, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, KeypairTraits, ToFromBytes,
};
use sui_types::crypto::{AuthorityKeyPair, SuiKeyPair};
use sui_types::governance::MIN_VALIDATOR_JOINING_STAKE_MIST;
use sui_types::object::Object;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::transaction::{CallArg, VerifiedTransaction};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use tokio::time::{timeout, Instant};
use tokio::{task::JoinHandle, time::sleep};
use tracing::info;

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
    pub next_node_index: usize,
    pub validator_candidates: Vec<(usize, ValidatorGenesisConfig)>,
}

impl TestCluster {
    pub fn rpc_client(&self) -> &HttpClient {
        &self.fullnode_handle.rpc_client
    }

    pub fn sui_client(&self) -> &SuiClient {
        &self.fullnode_handle.sui_client
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

    // Helper function to get the 2nd address in WalletContext
    pub fn get_address_2(&self) -> SuiAddress {
        self.wallet
            .config
            .keystore
            .addresses()
            .get(2)
            .cloned()
            .unwrap()
    }

    pub fn fullnode_config_builder(&self) -> FullnodeConfigBuilder {
        FullnodeConfigBuilder::new(self.swarm.config())
    }

    /// Convenience method to start a new fullnode in the test cluster.
    pub async fn start_fullnode(&self) -> Result<FullNodeHandle, anyhow::Error> {
        let config = self.fullnode_config_builder().build().unwrap();
        start_fullnode_from_config(config).await
    }

    pub fn all_node_handles(&self) -> impl Iterator<Item = SuiNodeHandle> {
        self.swarm
            .validator_node_handles()
            .into_iter()
            .chain(std::iter::once(SuiNodeHandle::new(
                self.fullnode_handle.sui_node.clone(),
            )))
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

    pub async fn get_reference_gas_price(&self) -> u64 {
        self.sui_client()
            .governance_api()
            .get_reference_gas_price()
            .await
            .expect("failed to get reference gas price")
    }

    pub async fn get_object_from_fullnode_store(&self, object_id: &ObjectID) -> Option<Object> {
        self.fullnode_handle
            .sui_node
            .state()
            .get_object(object_id)
            .await
            .unwrap()
    }

    /// To detect whether the network has reached such state, we use the fullnode as the
    /// source of truth, since a fullnode only does epoch transition when the network has
    /// done so.
    /// If target_epoch is specified, wait until the cluster reaches that epoch.
    /// If target_epoch is None, wait until the cluster reaches the next epoch.
    /// Note that this function does not guarantee that every node is at the target epoch.
    pub async fn wait_for_epoch(&self, target_epoch: Option<EpochId>) -> SuiSystemState {
        self.wait_for_epoch_with_timeout(target_epoch, Duration::from_secs(60))
            .await
    }

    pub async fn wait_for_epoch_with_timeout(
        &self,
        target_epoch: Option<EpochId>,
        timeout_dur: Duration,
    ) -> SuiSystemState {
        let mut epoch_rx = self.fullnode_handle.sui_node.subscribe_to_epoch_change();
        timeout(timeout_dur, async move {
            while let Ok(system_state) = epoch_rx.recv().await {
                info!("received epoch {}", system_state.epoch());
                match target_epoch {
                    Some(target_epoch) if system_state.epoch() >= target_epoch => {
                        return system_state;
                    }
                    None => {
                        return system_state;
                    }
                    _ => (),
                }
            }
            unreachable!("Broken reconfig channel");
        })
        .await
        .expect("Timed out waiting for cluster to target epoch")
    }

    /// Ask 2f+1 validators to close epoch actively, and wait for the entire network to reach the next
    /// epoch. This requires waiting for both the fullnode and all validators to reach the next epoch.
    pub async fn trigger_reconfiguration(&self) {
        info!("Starting reconfiguration");
        let start = Instant::now();

        // Close epoch on 2f+1 validators.
        let cur_committee = self
            .fullnode_handle
            .sui_node
            .state()
            .clone_committee_for_testing();
        let mut cur_stake = 0;
        for handle in self.swarm.validator_node_handles() {
            handle
                .with_async(|node| async {
                    node.close_epoch_for_testing().await.unwrap();
                    cur_stake += cur_committee.weight(&node.state().name);
                })
                .await;
            if cur_stake >= cur_committee.quorum_threshold() {
                break;
            }
        }
        info!("close_epoch complete after {:?}", start.elapsed());

        self.wait_for_epoch(Some(cur_committee.epoch + 1)).await;
        self.wait_for_epoch_all_validators(cur_committee.epoch + 1)
            .await;

        info!("reconfiguration complete after {:?}", start.elapsed());
    }

    pub async fn wait_for_epoch_all_validators(&self, target_epoch: EpochId) {
        let handles = self.swarm.validator_node_handles();
        let tasks: Vec<_> = handles.iter()
            .map(|handle| {
                handle.with_async(|node| async {
                    let mut retries = 0;
                    loop {
                        if node.state().epoch_store_for_testing().epoch() == target_epoch {
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        retries += 1;
                        if retries % 5 == 0 {
                            tracing::warn!(validator=?node.state().name.concise(), "Waiting for {:?} seconds for epoch change", retries);
                        }
                    }
                })
            })
            .collect();

        timeout(Duration::from_secs(40), join_all(tasks))
            .await
            .expect("timed out waiting for reconfiguration to complete");
    }

    /// Upgrade the network protocol version, by restarting every validator with a new
    /// supported versions.
    /// Note that we don't restart the fullnode here, and it is assumed that the fulnode supports
    /// the entire version range.
    pub async fn update_validator_supported_versions(
        &mut self,
        new_supported_versions: SupportedProtocolVersions,
    ) {
        for authority in self.get_validator_addresses().into_iter() {
            self.stop_validator(authority);
            tokio::time::sleep(Duration::from_millis(1000)).await;
            self.swarm
                .validator_mut(authority)
                .unwrap()
                .config
                .supported_protocol_versions = Some(new_supported_versions);
            self.start_validator(authority).await;
            info!("Restarted validator {}", authority);
        }
    }

    /// Wait for all nodes in the network to upgrade to `protocol_version`.
    pub async fn wait_for_all_nodes_upgrade_to(&self, protocol_version: u64) {
        for h in self.all_node_handles() {
            h.with_async(|node| async {
                while node
                    .state()
                    .epoch_store_for_testing()
                    .epoch_start_state()
                    .protocol_version()
                    .as_u64()
                    != protocol_version
                {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            })
            .await;
        }
    }

    pub async fn execute_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiRpcResult<SuiTransactionBlockResponse> {
        self.fullnode_handle
            .sui_client
            .quorum_driver_api()
            .execute_transaction_block(
                transaction,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                None,
            )
            .await
    }

    pub fn get_validator_account_addresses(&self) -> Vec<SuiAddress> {
        self.swarm
            .validators()
            .map(|v| (&v.config.account_key_pair.keypair().public()).into())
            .collect()
    }

    pub async fn send_validator_leaving_request(
        &self,
        address: SuiAddress,
    ) -> SuiTransactionBlockResponse {
        let keypair = self.get_validator_account_key(address);

        let gas_object = self
            .wallet
            .get_one_gas_object_owned_by_address(address)
            .await
            .unwrap()
            .unwrap();
        let tx =
            TestTransactionBuilder::new(address, gas_object, self.get_reference_gas_price().await)
                .move_call(
                    SUI_SYSTEM_PACKAGE_ID,
                    "sui_system",
                    "request_remove_validator",
                    vec![CallArg::SUI_SYSTEM_MUT],
                )
                .build_and_sign(keypair);
        let response = self.execute_transaction(tx).await.unwrap();
        assert_eq!(response.status_ok(), Some(true));
        response
    }

    pub async fn send_validator_joining_request(&mut self) -> SuiAddress {
        let (idx, candidate) = self.validator_candidates.pop().unwrap();
        let vandidator_info = candidate.to_validator_info(format!("validator{}", idx));
        let sender: SuiAddress = vandidator_info.account_address;
        let proof_of_possession = generate_proof_of_possession(&candidate.key_pair, sender);
        let rgp = self.get_reference_gas_price().await;
        let gas = self
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let candidate_tx_data = TestTransactionBuilder::new(sender, gas, rgp)
            .move_call(
                SUI_SYSTEM_PACKAGE_ID,
                "sui_system",
                "request_add_validator_candidate",
                vec![
                    CallArg::SUI_SYSTEM_MUT,
                    CallArg::Pure(bcs::to_bytes(vandidator_info.protocol_key.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.network_key.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.worker_key.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(proof_of_possession.as_ref()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.name.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.description.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.image_url.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(vandidator_info.project_url.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&vandidator_info.network_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&vandidator_info.p2p_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&vandidator_info.narwhal_primary_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&vandidator_info.narwhal_worker_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&DEFAULT_VALIDATOR_GAS_PRICE).unwrap()), // gas_price
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()), // commission_rate
                ],
            )
            .build();
        let transaction = self.wallet.sign_transaction(&candidate_tx_data);
        let effects = self
            .execute_transaction(transaction)
            .await
            .unwrap()
            .effects
            .unwrap();
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        sender
    }

    fn get_validator_account_key(&self, address: SuiAddress) -> &SuiKeyPair {
        for validator in self.swarm.validators() {
            let keypair = validator.config.account_key_pair.keypair();
            let addr: SuiAddress = (&keypair.public()).into();
            if addr == address {
                return keypair;
            }
        }
        unreachable!("Cannot find the validator with address {}", address);
    }

    #[cfg(msim)]
    pub fn set_safe_mode_expected(&self, value: bool) {
        for n in self.all_node_handles() {
            n.with(|node| node.set_safe_mode_expected(value));
        }
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
    additional_objects: Vec<Object>,
    num_validators: Option<usize>,
    fullnode_rpc_port: Option<u16>,
    enable_fullnode_events: bool,
    validator_supported_protocol_versions_config: ProtocolVersionsConfig,
    // Default to validator_supported_protocol_versions_config, but can be overridden.
    fullnode_supported_protocol_versions_config: Option<ProtocolVersionsConfig>,
    db_checkpoint_config_validators: DBCheckpointConfig,
    db_checkpoint_config_fullnodes: DBCheckpointConfig,
    validator_candidates_account_keys: Vec<AccountKeyPair>,
}

impl TestClusterBuilder {
    pub fn new() -> Self {
        TestClusterBuilder {
            genesis_config: None,
            additional_objects: vec![],
            fullnode_rpc_port: None,
            num_validators: None,
            enable_fullnode_events: false,
            validator_supported_protocol_versions_config: ProtocolVersionsConfig::Default,
            fullnode_supported_protocol_versions_config: None,
            db_checkpoint_config_validators: DBCheckpointConfig::default(),
            db_checkpoint_config_fullnodes: DBCheckpointConfig::default(),
            validator_candidates_account_keys: vec![],
        }
    }

    pub fn set_fullnode_rpc_port(mut self, rpc_port: u16) -> Self {
        self.fullnode_rpc_port = Some(rpc_port);
        self
    }

    pub fn set_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        assert!(self.genesis_config.is_none());
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn with_objects<I: IntoIterator<Item = Object>>(mut self, objects: I) -> Self {
        self.additional_objects.extend(objects);
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

    pub fn with_enable_db_checkpoints_validators(mut self) -> Self {
        self.db_checkpoint_config_validators = DBCheckpointConfig {
            perform_db_checkpoints_at_epoch_end: true,
            checkpoint_path: None,
            object_store_config: None,
            perform_index_db_checkpoints_at_epoch_end: None,
            prune_and_compact_before_upload: None,
        };
        self
    }

    pub fn with_enable_db_checkpoints_fullnodes(mut self) -> Self {
        self.db_checkpoint_config_fullnodes = DBCheckpointConfig {
            perform_db_checkpoints_at_epoch_end: true,
            checkpoint_path: None,
            object_store_config: None,
            perform_index_db_checkpoints_at_epoch_end: None,
            prune_and_compact_before_upload: Some(true),
        };
        self
    }

    pub fn with_epoch_duration_ms(mut self, epoch_duration_ms: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .epoch_duration_ms = epoch_duration_ms;
        self
    }

    pub fn with_stake_subsidy_start_epoch(mut self, stake_subsidy_start_epoch: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .stake_subsidy_start_epoch = stake_subsidy_start_epoch;
        self
    }

    pub fn with_supported_protocol_versions(mut self, c: SupportedProtocolVersions) -> Self {
        self.validator_supported_protocol_versions_config = ProtocolVersionsConfig::Global(c);
        self
    }

    pub fn with_fullnode_supported_protocol_versions_config(
        mut self,
        c: SupportedProtocolVersions,
    ) -> Self {
        self.fullnode_supported_protocol_versions_config = Some(ProtocolVersionsConfig::Global(c));
        self
    }

    pub fn with_protocol_version(mut self, v: ProtocolVersion) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .protocol_version = v;
        self
    }

    pub fn with_supported_protocol_version_callback(
        mut self,
        func: SupportedProtocolVersionsCallback,
    ) -> Self {
        self.validator_supported_protocol_versions_config =
            ProtocolVersionsConfig::PerValidator(func);
        self
    }

    pub fn with_accounts(mut self, accounts: Vec<AccountConfig>) -> Self {
        self.get_or_init_genesis_config().accounts.extend(accounts);
        self
    }

    pub fn with_validator_candidates_count(mut self, count: usize) -> Self {
        self.validator_candidates_account_keys = (0..count)
            .map(|_| get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng).1)
            .collect();
        let validator_candidate_accounts = self
            .validator_candidates_account_keys
            .iter()
            .map(|key| AccountConfig {
                address: Some(key.public().into()),
                gas_amounts: vec![DEFAULT_GAS_AMOUNT, MIN_VALIDATOR_JOINING_STAKE_MIST],
            })
            .collect();
        self.with_accounts(validator_candidate_accounts)
    }

    pub async fn build(self) -> anyhow::Result<TestCluster> {
        let cluster = self.start_test_network_with_customized_ports(None).await?;
        Ok(cluster)
    }

    pub async fn build_with_network_config(
        self,
        config: Option<PathBuf>,
    ) -> anyhow::Result<TestCluster> {
        let cluster = self
            .start_test_network_with_customized_ports(config)
            .await?;
        Ok(cluster)
    }

    async fn start_test_network_with_customized_ports(
        mut self,
        config: Option<PathBuf>,
    ) -> Result<TestCluster, anyhow::Error> {
        let swarm = if let Some(config) = config {
            info!("Building swarm from previous network config");
            self.start_swarm_with_network_config(config).await?
        } else {
            self.start_swarm().await?
        };

        let working_dir = swarm.dir();

        let mut wallet_conf: SuiClientConfig =
            PersistedConfig::read(&working_dir.join(SUI_CLIENT_CONFIG))?;

        let fullnode_config = FullnodeConfigBuilder::new(swarm.config())
            .with_supported_protocol_versions_config(
                self.fullnode_supported_protocol_versions_config
                    .clone()
                    .unwrap_or_else(|| self.validator_supported_protocol_versions_config.clone()),
            )
            .with_db_checkpoint_config(self.db_checkpoint_config_fullnodes)
            .set_event_store(self.enable_fullnode_events)
            .set_rpc_port(self.fullnode_rpc_port)
            .build()
            .unwrap();

        let fullnode_handle = start_fullnode_from_config(fullnode_config).await?;

        let validator_count = swarm.validator_node_handles().len();
        let validator_candidates: Vec<_> = self
            .validator_candidates_account_keys
            .into_iter()
            .enumerate()
            .map(|(i, account_key_pair)| {
                let idx = validator_count + i;
                let key_pair = get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut OsRng).1;
                let validator = ConfigBuilder::build_validator_with_account_key(
                    idx,
                    key_pair,
                    account_key_pair,
                    DEFAULT_VALIDATOR_GAS_PRICE,
                    &mut OsRng,
                );
                (idx, validator)
            })
            .collect();

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
        let wallet = WalletContext::new(&wallet_conf, None, None).await?;

        Ok(TestCluster {
            swarm,
            accounts,
            wallet,
            fullnode_handle,
            next_node_index: validator_count + validator_candidates.len(),
            validator_candidates,
        })
    }

    /// Start a swarm from a network config and set up WalletConfig
    async fn start_swarm_with_network_config(
        &mut self,
        network_config_path: std::path::PathBuf,
    ) -> Result<Swarm, anyhow::Error> {
        let mut builder: SwarmBuilder = Swarm::builder()
            .committee_size(
                NonZeroUsize::new(self.num_validators.unwrap_or(NUM_VALIDAOTR)).unwrap(),
            )
            .with_objects(self.additional_objects.clone())
            .with_db_checkpoint_config(self.db_checkpoint_config_validators.clone())
            .with_supported_protocol_versions_config(
                self.validator_supported_protocol_versions_config.clone(),
            );

        if let Some(genesis_config) = self.genesis_config.take() {
            builder = builder.with_genesis_config(genesis_config);
        }

        // Load the config of the Sui authority.
        let network_config: NetworkConfig =
            PersistedConfig::read(&network_config_path).map_err(|err| {
                err.context(format!(
                    "Cannot open Sui network config file at {:?}",
                    network_config_path
                ))
            })?;
        let mut swarm = builder.from_network_config(sui_cluster_test_config_dir()?, network_config);
        swarm.launch().await?;

        let dir = swarm.dir();

        // This uses the sui config directory provided and stores the wallet path in the SUI_CLUSTER_CONFIG_DIR
        let network_path = dir.join(SUI_NETWORK_CONFIG);
        let wallet_path = dir.join(SUI_CLIENT_CONFIG);
        let keystore_path = dir.join(SUI_KEYSTORE_FILENAME);
        swarm.config().save(network_path)?;

        // We don't need to add keystore since we have local keystore for this.
        // Add a key from the keystore for wallet.
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let active_address = keystore.addresses().first().cloned();

        // Create wallet config with stated authorities port
        SuiClientConfig {
            keystore: Keystore::from(FileBasedKeystore::new(&keystore_path)?),
            envs: Default::default(),
            active_address,
            active_env: Default::default(),
        }
        .save(wallet_path)?;

        // Return network handle
        Ok(swarm)
    }

    /// Start a Swarm and set up WalletConfig
    async fn start_swarm(&mut self) -> Result<Swarm, anyhow::Error> {
        self.get_or_init_genesis_config();
        let genesis_config = self.genesis_config.take().unwrap();

        let builder: SwarmBuilder = Swarm::builder()
            .committee_size(
                NonZeroUsize::new(self.num_validators.unwrap_or(NUM_VALIDAOTR)).unwrap(),
            )
            .with_objects(self.additional_objects.clone())
            .with_db_checkpoint_config(self.db_checkpoint_config_validators.clone())
            .with_supported_protocol_versions_config(
                self.validator_supported_protocol_versions_config.clone(),
            )
            .with_genesis_config(genesis_config);

        let mut swarm = builder.build();
        swarm.launch().await?;

        let dir = swarm.dir();

        let network_path = dir.join(SUI_NETWORK_CONFIG);
        let wallet_path = dir.join(SUI_CLIENT_CONFIG);
        let keystore_path = dir.join(SUI_KEYSTORE_FILENAME);

        swarm.config().save(&network_path)?;
        let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        for key in swarm
            .config()
            .account_keys
            .iter()
            .chain(&self.validator_candidates_account_keys)
        {
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
        .save(wallet_path)?;

        // Return network handle
        Ok(swarm)
    }

    fn get_or_init_genesis_config(&mut self) -> &mut GenesisConfig {
        if self.genesis_config.is_none() {
            self.genesis_config = Some(GenesisConfig::for_local_testing());
        }
        self.genesis_config.as_mut().unwrap()
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
    let sui_node = SuiNode::start(&config, registry_service, None).await?;

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

// TODO: Merge the following functions with the ones inside TestCluster.
pub async fn wait_for_node_transition_to_epoch(node: &SuiNodeHandle, expected_epoch: EpochId) {
    node.with_async(|node| async move {
        let mut rx = node.subscribe_to_epoch_change();
        let epoch = node.current_epoch_for_testing();
        if epoch != expected_epoch {
            let system_state = rx.recv().await.unwrap();
            assert_eq!(system_state.epoch(), expected_epoch);
        }
    })
    .await
}

pub async fn wait_for_nodes_transition_to_epoch<'a>(
    nodes: impl Iterator<Item = &'a SuiNodeHandle>,
    expected_epoch: EpochId,
) {
    let handles: Vec<_> = nodes
        .map(|handle| wait_for_node_transition_to_epoch(handle, expected_epoch))
        .collect();
    join_all(handles).await;
}
