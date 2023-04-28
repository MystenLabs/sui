// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::p2p::P2pConfig;
use crate::transaction_deny_config::TransactionDenyConfig;
use crate::Config;
use anyhow::Result;
use narwhal_config::Parameters as ConsensusParameters;
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::usize;
use sui_keys::keypair_file::{read_authority_keypair_from_file, read_keypair_from_file};
use sui_protocol_config::SupportedProtocolVersions;
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::NetworkKeyPair;
use sui_types::crypto::NetworkPublicKey;
use sui_types::crypto::SuiKeyPair;
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair};
use sui_types::multiaddr::Multiaddr;

// Default max number of concurrent requests served
pub const DEFAULT_GRPC_CONCURRENCY_LIMIT: usize = 20000000000;

/// Default gas price of 100 Mist
pub const DEFAULT_VALIDATOR_GAS_PRICE: u64 = 1000;

/// Default commission rate of 2%
pub const DEFAULT_COMMISSION_RATE: u64 = 200;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    #[serde(default = "default_authority_key_pair")]
    pub protocol_key_pair: AuthorityKeyPairWithPath,
    #[serde(default = "default_key_pair")]
    pub worker_key_pair: KeyPairWithPath,
    #[serde(default = "default_key_pair")]
    pub account_key_pair: KeyPairWithPath,
    #[serde(default = "default_key_pair")]
    pub network_key_pair: KeyPairWithPath,

    pub db_path: PathBuf,
    #[serde(default = "default_grpc_address")]
    pub network_address: Multiaddr,
    #[serde(default = "default_json_rpc_address")]
    pub json_rpc_address: SocketAddr,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_admin_interface_port")]
    pub admin_interface_port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    // TODO: Remove this as it's no longer used.
    #[serde(default)]
    pub enable_event_processing: bool,

    #[serde(default = "default_enable_index_processing")]
    pub enable_index_processing: bool,

    #[serde(default)]
    pub grpc_load_shed: Option<bool>,

    #[serde(default = "default_concurrency_limit")]
    pub grpc_concurrency_limit: Option<usize>,

    #[serde(default)]
    pub p2p_config: P2pConfig,

    pub genesis: Genesis,

    #[serde(default = "default_authority_store_pruning_config")]
    pub authority_store_pruning_config: AuthorityStorePruningConfig,

    /// Size of the broadcast channel used for notifying other systems of end of epoch.
    ///
    /// If unspecified, this will default to `128`.
    #[serde(default = "default_end_of_epoch_broadcast_channel_capacity")]
    pub end_of_epoch_broadcast_channel_capacity: usize,

    #[serde(default)]
    pub checkpoint_executor_config: CheckpointExecutorConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<MetricsConfig>,

    /// In a `sui-node` binary, this is set to SupportedProtocolVersions::SYSTEM_DEFAULT
    /// in sui-node/src/main.rs. It is present in the config so that it can be changed by tests in
    /// order to test protocol upgrades.
    #[serde(skip)]
    pub supported_protocol_versions: Option<SupportedProtocolVersions>,

    #[serde(default)]
    pub db_checkpoint_config: DBCheckpointConfig,

    #[serde(default)]
    pub indirect_objects_threshold: usize,

    #[serde(default)]
    pub expensive_safety_check_config: ExpensiveSafetyCheckConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_service_resolver_object_id: Option<ObjectID>,

    #[serde(default)]
    pub transaction_deny_config: TransactionDenyConfig,
}

fn default_authority_store_pruning_config() -> AuthorityStorePruningConfig {
    AuthorityStorePruningConfig::default()
}

pub fn default_enable_index_processing() -> bool {
    true
}

fn default_grpc_address() -> Multiaddr {
    "/ip4/0.0.0.0/tcp/8080".parse().unwrap()
}
fn default_authority_key_pair() -> AuthorityKeyPairWithPath {
    AuthorityKeyPairWithPath::new(get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut OsRng).1)
}

fn default_key_pair() -> KeyPairWithPath {
    KeyPairWithPath::new(
        get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng)
            .1
            .into(),
    )
}

fn default_metrics_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9184)
}

pub fn default_admin_interface_port() -> u16 {
    1337
}

pub fn default_json_rpc_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000)
}

pub fn default_websocket_address() -> Option<SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr};
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9001))
}

pub fn default_concurrency_limit() -> Option<usize> {
    Some(DEFAULT_GRPC_CONCURRENCY_LIMIT)
}

pub fn default_end_of_epoch_broadcast_channel_capacity() -> usize {
    128
}

pub fn bool_true() -> bool {
    true
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn protocol_key_pair(&self) -> &AuthorityKeyPair {
        self.protocol_key_pair.authority_keypair()
    }

    pub fn worker_key_pair(&self) -> &NetworkKeyPair {
        match self.worker_key_pair.keypair() {
            SuiKeyPair::Ed25519(kp) => kp,
            other => panic!(
                "Invalid keypair type: {:?}, only Ed25519 is allowed for worker key",
                other
            ),
        }
    }

    pub fn network_key_pair(&self) -> &NetworkKeyPair {
        match self.network_key_pair.keypair() {
            SuiKeyPair::Ed25519(kp) => kp,
            other => panic!(
                "Invalid keypair type: {:?}, only Ed25519 is allowed for network key",
                other
            ),
        }
    }

    pub fn protocol_public_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key_pair().public().into()
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_path.join("live")
    }

    pub fn db_checkpoint_path(&self) -> PathBuf {
        self.db_path.join("db_checkpoints")
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn consensus_config(&self) -> Option<&ConsensusConfig> {
        self.consensus_config.as_ref()
    }

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        self.genesis.genesis()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsensusConfig {
    pub address: Multiaddr,
    pub db_path: PathBuf,

    // Optional alternative address preferentially used by a primary to talk to its own worker.
    // For example, this could be used to connect to co-located workers over a private LAN address.
    pub internal_worker_address: Option<Multiaddr>,

    // Maximum number of pending transactions to submit to consensus, including those
    // in submission wait.
    // Assuming 10_000 txn tps * 10 sec consensus latency = 100_000 inflight consensus txns,
    // Default to 100_000.
    pub max_pending_transactions: Option<usize>,

    pub narwhal_config: ConsensusParameters,
}

impl ConsensusConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn max_pending_transactions(&self) -> usize {
        self.max_pending_transactions.unwrap_or(100_000)
    }

    pub fn narwhal_config(&self) -> &ConsensusParameters {
        &self.narwhal_config
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CheckpointExecutorConfig {
    /// Upper bound on the number of checkpoints that can be concurrently executed
    ///
    /// If unspecified, this will default to `200`
    #[serde(default = "default_checkpoint_execution_max_concurrency")]
    pub checkpoint_execution_max_concurrency: usize,

    /// Number of seconds to wait for effects of a batch of transactions
    /// before logging a warning. Note that we will continue to retry
    /// indefinitely
    ///
    /// If unspecified, this will default to `10`.
    #[serde(default = "default_local_execution_timeout_sec")]
    pub local_execution_timeout_sec: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExpensiveSafetyCheckConfig {
    /// If enabled, at epoch boundary, we will check that the storage
    /// fund balance is always identical to the sum of the storage
    /// rebate of all live objects, and that the total SUI in the network remains
    /// the same.
    #[serde(default)]
    enable_epoch_sui_conservation_check: bool,

    /// If enabled, we will check that the total SUI in all input objects of a tx
    /// (both the Move part and the storage rebate) matches the total SUI in all
    /// output objects of the tx + gas fees
    #[serde(default)]
    enable_deep_per_tx_sui_conservation_check: bool,

    /// Disable epoch SUI conservation check even when we are running in debug mode.
    #[serde(default)]
    force_disable_epoch_sui_conservation_check: bool,

    /// If enabled, at epoch boundary, we will check that the accumulated
    /// live object state matches the end of epoch root state digest.
    #[serde(default)]
    enable_state_consistency_check: bool,

    /// Disable state consistency check even when we are running in debug mode.
    #[serde(default)]
    force_disable_state_consistency_check: bool,

    /// If enabled, we run the Move VM in paranoid mode, which provides protection
    /// against some (but not all) potential bugs in the bytecode verifier
    #[serde(default)]
    enable_move_vm_paranoid_checks: bool,
    // TODO: Add more expensive checks here
}

impl ExpensiveSafetyCheckConfig {
    pub fn new_enable_all() -> Self {
        Self {
            enable_epoch_sui_conservation_check: true,
            enable_deep_per_tx_sui_conservation_check: true,
            force_disable_epoch_sui_conservation_check: false,
            enable_state_consistency_check: true,
            force_disable_state_consistency_check: false,
            enable_move_vm_paranoid_checks: true,
        }
    }

    pub fn enable_paranoid_checks(&mut self) {
        self.enable_move_vm_paranoid_checks = true
    }

    pub fn force_disable_epoch_sui_conservation_check(&mut self) {
        self.force_disable_epoch_sui_conservation_check = true;
    }

    pub fn enable_epoch_sui_conservation_check(&self) -> bool {
        (self.enable_epoch_sui_conservation_check || cfg!(debug_assertions))
            && !self.force_disable_epoch_sui_conservation_check
    }

    pub fn force_disable_state_consistency_check(&mut self) {
        self.force_disable_state_consistency_check = true;
    }

    pub fn enable_state_consistency_check(&self) -> bool {
        (self.enable_state_consistency_check || cfg!(debug_assertions))
            && !self.force_disable_state_consistency_check
    }

    pub fn enable_move_vm_paranoid_checks(&self) -> bool {
        self.enable_move_vm_paranoid_checks
    }

    pub fn enable_deep_per_tx_sui_conservation_check(&self) -> bool {
        self.enable_deep_per_tx_sui_conservation_check || cfg!(debug_assertions)
    }
}

fn default_checkpoint_execution_max_concurrency() -> usize {
    200
}

fn default_local_execution_timeout_sec() -> u64 {
    30
}

impl Default for CheckpointExecutorConfig {
    fn default() -> Self {
        Self {
            checkpoint_execution_max_concurrency: default_checkpoint_execution_max_concurrency(),
            local_execution_timeout_sec: default_local_execution_timeout_sec(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthorityStorePruningConfig {
    /// number of the latest epoch dbs to retain
    pub num_latest_epoch_dbs_to_retain: usize,
    /// time interval used by the pruner to determine whether there are any epoch DBs to remove
    pub epoch_db_pruning_period_secs: u64,
    /// number of epochs to keep the latest version of objects for.
    /// Note that a zero value corresponds to an aggressive pruner.
    /// This mode is experimental and needs to be used with caution.
    /// Use `u64::MAX` to disable the pruner for the objects.
    pub num_epochs_to_retain: u64,
    /// pruner's runtime interval used for aggressive mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruning_run_delay_seconds: Option<u64>,
    /// maximum number of checkpoints in the pruning batch. Can be adjusted to increase performance
    pub max_checkpoints_in_batch: usize,
    /// maximum number of transaction in the pruning batch
    pub max_transactions_in_batch: usize,
    /// pruner deletion method. If set to `true`, range deletion is utilized (recommended).
    /// Use `false` for point deletes.
    pub use_range_deletion: bool,
}

impl Default for AuthorityStorePruningConfig {
    fn default() -> Self {
        Self {
            num_latest_epoch_dbs_to_retain: usize::MAX,
            epoch_db_pruning_period_secs: u64::MAX,
            num_epochs_to_retain: 2,
            pruning_run_delay_seconds: None,
            max_checkpoints_in_batch: 10,
            max_transactions_in_batch: 1000,
            use_range_deletion: true,
        }
    }
}

impl AuthorityStorePruningConfig {
    pub fn validator_config() -> Self {
        Self {
            num_latest_epoch_dbs_to_retain: 3,
            epoch_db_pruning_period_secs: 60 * 60,
            num_epochs_to_retain: 2,
            pruning_run_delay_seconds: None,
            max_checkpoints_in_batch: 10,
            max_transactions_in_batch: 1000,
            use_range_deletion: true,
        }
    }
    pub fn fullnode_config() -> Self {
        Self {
            num_latest_epoch_dbs_to_retain: 3,
            epoch_db_pruning_period_secs: 60 * 60,
            num_epochs_to_retain: 2,
            pruning_run_delay_seconds: None,
            max_checkpoints_in_batch: 10,
            max_transactions_in_batch: 1000,
            use_range_deletion: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_interval_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_url: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DBCheckpointConfig {
    #[serde(default)]
    pub perform_db_checkpoints_at_epoch_end: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_store_config: Option<ObjectStoreConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perform_index_db_checkpoints_at_epoch_end: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_and_compact_before_upload: Option<bool>,
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfo {
    pub name: String,
    pub account_address: SuiAddress,
    pub protocol_key: AuthorityPublicKeyBytes,
    pub worker_key: NetworkPublicKey,
    pub network_key: NetworkPublicKey,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
}

impl ValidatorInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sui_address(&self) -> SuiAddress {
        self.account_address
    }

    pub fn protocol_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key
    }

    pub fn worker_key(&self) -> &NetworkPublicKey {
        &self.worker_key
    }

    pub fn network_key(&self) -> &NetworkPublicKey {
        &self.network_key
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn commission_rate(&self) -> u64 {
        self.commission_rate
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn narwhal_primary_address(&self) -> &Multiaddr {
        &self.narwhal_primary_address
    }

    pub fn narwhal_worker_address(&self) -> &Multiaddr {
        &self.narwhal_worker_address
    }

    pub fn p2p_address(&self) -> &Multiaddr {
        &self.p2p_address
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
pub struct Genesis {
    #[serde(flatten)]
    location: GenesisLocation,

    #[serde(skip)]
    genesis: once_cell::sync::OnceCell<genesis::Genesis>,
}

impl Genesis {
    pub fn new(genesis: genesis::Genesis) -> Self {
        Self {
            location: GenesisLocation::InPlace { genesis },
            genesis: Default::default(),
        }
    }

    pub fn new_from_file<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            location: GenesisLocation::File {
                genesis_file_location: path.into(),
            },
            genesis: Default::default(),
        }
    }

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        match &self.location {
            GenesisLocation::InPlace { genesis } => Ok(genesis),
            GenesisLocation::File {
                genesis_file_location,
            } => self
                .genesis
                .get_or_try_init(|| genesis::Genesis::load(genesis_file_location)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
#[serde(untagged)]
enum GenesisLocation {
    InPlace {
        genesis: genesis::Genesis,
    },
    File {
        #[serde(rename = "genesis-file-location")]
        genesis_file_location: PathBuf,
    },
}

/// Wrapper struct for SuiKeyPair that can be deserialized from a file path. Used by network, worker, and account keypair.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeyPairWithPath {
    #[serde(flatten)]
    location: KeyPairLocation,

    #[serde(skip)]
    keypair: OnceCell<Arc<SuiKeyPair>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
#[serde_as]
#[serde(untagged)]
enum KeyPairLocation {
    InPlace {
        #[serde_as(as = "Arc<KeyPairBase64>")]
        value: Arc<SuiKeyPair>,
    },
    File {
        #[serde(rename = "path")]
        path: PathBuf,
    },
}

impl KeyPairWithPath {
    pub fn new(kp: SuiKeyPair) -> Self {
        let cell: OnceCell<Arc<SuiKeyPair>> = OnceCell::new();
        let arc_kp = Arc::new(kp);
        // OK to unwrap panic because authority should not start without all keypairs loaded.
        cell.set(arc_kp.clone()).expect("Failed to set keypair");
        Self {
            location: KeyPairLocation::InPlace { value: arc_kp },
            keypair: cell,
        }
    }

    pub fn new_from_path(path: PathBuf) -> Self {
        let cell: OnceCell<Arc<SuiKeyPair>> = OnceCell::new();
        // OK to unwrap panic because authority should not start without all keypairs loaded.
        cell.set(Arc::new(read_keypair_from_file(&path).unwrap_or_else(
            |e| panic!("Invalid keypair file at path {:?}: {e}", &path),
        )))
        .expect("Failed to set keypair");
        Self {
            location: KeyPairLocation::File { path },
            keypair: cell,
        }
    }

    pub fn keypair(&self) -> &SuiKeyPair {
        self.keypair
            .get_or_init(|| match &self.location {
                KeyPairLocation::InPlace { value } => value.clone(),
                KeyPairLocation::File { path } => {
                    // OK to unwrap panic because authority should not start without all keypairs loaded.
                    Arc::new(
                        read_keypair_from_file(path).unwrap_or_else(|e| {
                            panic!("Invalid keypair file at path {:?}: {e}", path)
                        }),
                    )
                }
            })
            .as_ref()
    }
}

/// Wrapper struct for AuthorityKeyPair that can be deserialized from a file path.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AuthorityKeyPairWithPath {
    #[serde(flatten)]
    location: AuthorityKeyPairLocation,

    #[serde(skip)]
    keypair: OnceCell<Arc<AuthorityKeyPair>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
#[serde_as]
#[serde(untagged)]
enum AuthorityKeyPairLocation {
    InPlace { value: Arc<AuthorityKeyPair> },
    File { path: PathBuf },
}

impl AuthorityKeyPairWithPath {
    pub fn new(kp: AuthorityKeyPair) -> Self {
        let cell: OnceCell<Arc<AuthorityKeyPair>> = OnceCell::new();
        let arc_kp = Arc::new(kp);
        // OK to unwrap panic because authority should not start without all keypairs loaded.
        cell.set(arc_kp.clone())
            .expect("Failed to set authority keypair");
        Self {
            location: AuthorityKeyPairLocation::InPlace { value: arc_kp },
            keypair: cell,
        }
    }

    pub fn new_from_path(path: PathBuf) -> Self {
        let cell: OnceCell<Arc<AuthorityKeyPair>> = OnceCell::new();
        // OK to unwrap panic because authority should not start without all keypairs loaded.
        cell.set(Arc::new(
            read_authority_keypair_from_file(&path)
                .unwrap_or_else(|_| panic!("Invalid authority keypair file at path {:?}", &path)),
        ))
        .expect("Failed to set authority keypair");
        Self {
            location: AuthorityKeyPairLocation::File { path },
            keypair: cell,
        }
    }

    pub fn authority_keypair(&self) -> &AuthorityKeyPair {
        self.keypair
            .get_or_init(|| match &self.location {
                AuthorityKeyPairLocation::InPlace { value } => value.clone(),
                AuthorityKeyPairLocation::File { path } => {
                    // OK to unwrap panic because authority should not start without all keypairs loaded.
                    Arc::new(
                        read_authority_keypair_from_file(path).unwrap_or_else(|_| {
                            panic!("Invalid authority keypair file {:?}", &path)
                        }),
                    )
                }
            })
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fastcrypto::traits::KeyPair;
    use rand::{rngs::StdRng, SeedableRng};
    use sui_keys::keypair_file::{write_authority_keypair_to_file, write_keypair_to_file};
    use sui_types::crypto::{get_key_pair_from_rng, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair};

    use super::Genesis;
    use crate::NodeConfig;

    #[test]
    fn serialize_genesis_config_from_file() {
        let g = Genesis::new_from_file("path/to/file");

        let s = serde_yaml::to_string(&g).unwrap();
        assert_eq!("---\ngenesis-file-location: path/to/file\n", s);
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn serialize_genesis_config_in_place() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let g = Genesis::new(genesis);

        let mut s = serde_yaml::to_string(&g).unwrap();
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);

        // If both in-place and file location are provided, prefer the in-place variant
        s.push_str("\ngenesis-file-location: path/to/file");
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn load_genesis_config_from_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let genesis_config = Genesis::new_from_file(file.path());

        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;
        genesis.save(file.path()).unwrap();

        let loaded_genesis = genesis_config.genesis().unwrap();
        loaded_genesis.checkpoint_contents().digest(); // cache digest before comparing.
        assert_eq!(&genesis, loaded_genesis);
    }

    #[test]
    fn fullnode_template() {
        const TEMPLATE: &str = include_str!("../data/fullnode-template.yaml");

        let _template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
    }

    #[test]
    fn load_key_pairs_to_node_config() {
        let protocol_key_pair: AuthorityKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
        let worker_key_pair: NetworkKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
        let network_key_pair: NetworkKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;

        write_authority_keypair_to_file(&protocol_key_pair, PathBuf::from("protocol.key")).unwrap();
        write_keypair_to_file(
            &SuiKeyPair::Ed25519(worker_key_pair.copy()),
            PathBuf::from("worker.key"),
        )
        .unwrap();
        write_keypair_to_file(
            &SuiKeyPair::Ed25519(network_key_pair.copy()),
            PathBuf::from("network.key"),
        )
        .unwrap();

        const TEMPLATE: &str = include_str!("../data/fullnode-template-with-path.yaml");
        let template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
        assert_eq!(
            template.protocol_key_pair().public(),
            protocol_key_pair.public()
        );
        assert_eq!(
            template.network_key_pair().public(),
            network_key_pair.public()
        );
        assert_eq!(
            template.worker_key_pair().public(),
            worker_key_pair.public()
        );
    }
}
