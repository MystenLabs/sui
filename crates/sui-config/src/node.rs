// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::Config;
use crate::certificate_deny_config::CertificateDenyConfig;
use crate::genesis;
use crate::object_storage_config::ObjectStoreConfig;
use crate::p2p::P2pConfig;
use crate::transaction_deny_config::TransactionDenyConfig;
use crate::validator_client_monitor_config::ValidatorClientMonitorConfig;
use crate::verifier_signing_config::VerifierSigningConfig;
use anyhow::Result;
use consensus_config::Parameters as ConsensusParameters;
use mysten_common::fatal;
use nonzero_ext::nonzero;
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sui_keys::keypair_file::{read_authority_keypair_from_file, read_keypair_from_file};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::NetworkKeyPair;
use sui_types::crypto::SuiKeyPair;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::supported_protocol_versions::{Chain, SupportedProtocolVersions};
use sui_types::traffic_control::{PolicyConfig, RemoteFirewallConfig};

use sui_types::crypto::{AccountKeyPair, AuthorityKeyPair, get_key_pair_from_rng};
use sui_types::multiaddr::Multiaddr;
use tracing::info;

// Default max number of concurrent requests served
pub const DEFAULT_GRPC_CONCURRENCY_LIMIT: usize = 20000000000;

/// Default gas price of 100 Mist
pub const DEFAULT_VALIDATOR_GAS_PRICE: u64 = sui_types::transaction::DEFAULT_VALIDATOR_GAS_PRICE;

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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc: Option<crate::RpcConfig>,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_admin_interface_port")]
    pub admin_interface_port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    #[serde(default = "default_enable_index_processing")]
    pub enable_index_processing: bool,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove_deprecated_tables: bool,

    #[serde(default)]
    /// Determines the jsonrpc server type as either:
    /// - 'websocket' for a websocket based service (deprecated)
    /// - 'http' for an http based service
    /// - 'both' for both a websocket and http based service (deprecated)
    pub jsonrpc_server_type: Option<ServerType>,

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
    pub expensive_safety_check_config: ExpensiveSafetyCheckConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_service_package_address: Option<SuiAddress>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_service_registry_id: Option<ObjectID>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_service_reverse_registry_id: Option<ObjectID>,

    #[serde(default)]
    pub transaction_deny_config: TransactionDenyConfig,

    #[serde(default)]
    pub certificate_deny_config: CertificateDenyConfig,

    #[serde(default)]
    pub state_debug_dump_config: StateDebugDumpConfig,

    #[serde(default)]
    pub state_archive_read_config: Vec<StateArchiveConfig>,

    #[serde(default)]
    pub state_snapshot_write_config: StateSnapshotConfig,

    #[serde(default)]
    pub indexer_max_subscriptions: Option<usize>,

    #[serde(default = "default_transaction_kv_store_config")]
    pub transaction_kv_store_read_config: TransactionKeyValueStoreReadConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_kv_store_write_config: Option<TransactionKeyValueStoreWriteConfig>,

    #[serde(default = "default_jwk_fetch_interval_seconds")]
    pub jwk_fetch_interval_seconds: u64,

    #[serde(default = "default_zklogin_oauth_providers")]
    pub zklogin_oauth_providers: BTreeMap<Chain, BTreeSet<String>>,

    #[serde(default = "default_authority_overload_config")]
    pub authority_overload_config: AuthorityOverloadConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_with_range: Option<RunWithRange>,

    // For killswitch use None
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_traffic_controller_policy_config"
    )]
    pub policy_config: Option<PolicyConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub firewall_config: Option<RemoteFirewallConfig>,

    #[serde(default)]
    pub execution_cache: ExecutionCacheConfig,

    // step 1 in removing the old state accumulator
    #[serde(skip)]
    #[serde(default = "bool_true")]
    pub state_accumulator_v2: bool,

    #[serde(default = "bool_true")]
    pub enable_soft_bundle: bool,

    #[serde(default)]
    pub verifier_signing_config: VerifierSigningConfig,

    /// If a value is set, it determines if writes to DB can stall, which can halt the whole process.
    /// By default, write stall is enabled on validators but not on fullnodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_db_write_stall: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_observer_config: Option<ExecutionTimeObserverConfig>,

    /// Allow overriding the chain for testing purposes. For instance, it allows you to
    /// create a test network that believes it is mainnet or testnet. Attempting to
    /// override this value on production networks will result in an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_override_for_testing: Option<Chain>,

    /// Configuration for validator client monitoring from the client perspective.
    /// When enabled, tracks client-observed performance metrics for validators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator_client_monitor_config: Option<ValidatorClientMonitorConfig>,

    /// Fork recovery configuration for handling validator equivocation after forks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_recovery: Option<ForkRecoveryConfig>,

    /// Configuration for the transaction driver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_driver_config: Option<TransactionDriverConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransactionDriverConfig {
    /// The list of validators that are allowed to submit MFP transactions to (via the transaction driver).
    /// Each entry is a validator display name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_submission_validators: Vec<String>,

    /// The list of validators that are blocked from submitting block transactions to (via the transaction driver).
    /// Each entry is a validator display name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_submission_validators: Vec<String>,

    /// Enable early transaction validation before submission to consensus.
    /// This checks for non-retriable errors (like old object versions) and rejects
    /// transactions early to provide fast feedback to clients.
    /// Note: Currently used in TransactionOrchestrator, but may be moved to TransactionDriver in future.
    #[serde(default = "bool_true")]
    pub enable_early_validation: bool,
}

impl Default for TransactionDriverConfig {
    fn default() -> Self {
        Self {
            allowed_submission_validators: vec![],
            blocked_submission_validators: vec![],
            enable_early_validation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ForkCrashBehavior {
    #[serde(rename = "await-fork-recovery")]
    #[default]
    AwaitForkRecovery,
    /// Return an error instead of blocking forever. This is primarily for testing.
    #[serde(rename = "return-error")]
    ReturnError,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ForkRecoveryConfig {
    /// Map of transaction digest to effects digest overrides
    /// Used to repoint transactions to correct effects after a fork
    #[serde(default)]
    pub transaction_overrides: BTreeMap<String, String>,

    /// Map of checkpoint sequence number to checkpoint digest overrides
    /// On node start, if we have a locally computed checkpoint with a
    /// digest mismatch with this table, we will clear any associated local state.
    #[serde(default)]
    pub checkpoint_overrides: BTreeMap<u64, String>,

    /// Behavior when a fork is detected after recovery attempts
    #[serde(default)]
    pub fork_crash_behavior: ForkCrashBehavior,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExecutionTimeObserverConfig {
    /// Size of the channel used for buffering local execution time observations.
    ///
    /// If unspecified, this will default to `1_024`.
    pub observation_channel_capacity: Option<NonZeroUsize>,

    /// Size of the LRU cache used for storing local execution time observations.
    ///
    /// If unspecified, this will default to `10_000`.
    pub observation_cache_size: Option<NonZeroUsize>,

    /// Size of the channel used for buffering object debt updates from consensus handler.
    ///
    /// If unspecified, this will default to `128`.
    pub object_debt_channel_capacity: Option<NonZeroUsize>,

    /// Size of the LRU cache used for tracking object utilization.
    ///
    /// If unspecified, this will default to `50_000`.
    pub object_utilization_cache_size: Option<NonZeroUsize>,

    /// If true, the execution time observer will report per-object utilization metrics
    /// with full object IDs. When set, the metric can have a high cardinality, so this
    /// should not be used except in controlled tests where there are a small number of
    /// objects.
    ///
    /// If false, object utilization is reported using hash(object_id) % 32 as the key,
    /// which still allows observation of utilization when there are small numbers of
    /// over-utilized objects.
    ///
    /// If unspecified, this will default to `false`.
    pub report_object_utilization_metric_with_full_id: Option<bool>,

    /// Unless target object utilization is exceeded by at least this amount, no observation
    /// will be shared with consensus.
    ///
    /// If unspecified, this will default to `500` milliseconds.
    pub observation_sharing_object_utilization_threshold: Option<Duration>,

    /// Unless the current local observation differs from the last one we shared by at least this
    /// percentage, no observation will be shared with consensus.
    ///
    /// If unspecified, this will default to `0.1`.
    pub observation_sharing_diff_threshold: Option<f64>,

    /// Minimum interval between sharing multiple observations of the same key.
    ///
    /// If unspecified, this will default to `5` seconds.
    pub observation_sharing_min_interval: Option<Duration>,

    /// Global per-second rate limit for sharing observations. This is a safety valve and
    /// should not trigger during normal operation.
    ///
    /// If unspecified, this will default to `10` observations per second.
    pub observation_sharing_rate_limit: Option<NonZeroU32>,

    /// Global burst limit for sharing observations.
    ///
    /// If unspecified, this will default to `100` observations.
    pub observation_sharing_burst_limit: Option<NonZeroU32>,

    /// Whether to use gas price weighting in execution time estimates.
    /// When enabled, samples with higher gas prices have more influence on the
    /// execution time estimates, providing protection against volume-based
    /// manipulation attacks.
    ///
    /// If unspecified, this will default to `false`.
    pub enable_gas_price_weighting: Option<bool>,

    /// Size of the weighted moving average window for execution time observations.
    /// This determines how many recent observations are kept in the weighted moving average
    /// calculation for each execution time observation key.
    /// Note that this is independent of the window size for the simple moving average.
    ///
    /// If unspecified, this will default to `20`.
    pub weighted_moving_average_window_size: Option<usize>,

    /// Whether to inject synthetic execution time for testing in simtest.
    /// When enabled, synthetic timings will be generated for execution time observations
    /// to enable deterministic testing of congestion control features.
    ///
    /// If unspecified, this will default to `false`.
    #[cfg(msim)]
    pub inject_synthetic_execution_time: Option<bool>,
}

impl ExecutionTimeObserverConfig {
    pub fn observation_channel_capacity(&self) -> NonZeroUsize {
        self.observation_channel_capacity
            .unwrap_or(nonzero!(1_024usize))
    }

    pub fn observation_cache_size(&self) -> NonZeroUsize {
        self.observation_cache_size.unwrap_or(nonzero!(10_000usize))
    }

    pub fn object_debt_channel_capacity(&self) -> NonZeroUsize {
        self.object_debt_channel_capacity
            .unwrap_or(nonzero!(128usize))
    }

    pub fn object_utilization_cache_size(&self) -> NonZeroUsize {
        self.object_utilization_cache_size
            .unwrap_or(nonzero!(50_000usize))
    }

    pub fn report_object_utilization_metric_with_full_id(&self) -> bool {
        self.report_object_utilization_metric_with_full_id
            .unwrap_or(false)
    }

    pub fn observation_sharing_object_utilization_threshold(&self) -> Duration {
        self.observation_sharing_object_utilization_threshold
            .unwrap_or(Duration::from_millis(500))
    }

    pub fn observation_sharing_diff_threshold(&self) -> f64 {
        self.observation_sharing_diff_threshold.unwrap_or(0.1)
    }

    pub fn observation_sharing_min_interval(&self) -> Duration {
        self.observation_sharing_min_interval
            .unwrap_or(Duration::from_secs(5))
    }

    pub fn observation_sharing_rate_limit(&self) -> NonZeroU32 {
        self.observation_sharing_rate_limit
            .unwrap_or(nonzero!(10u32))
    }

    pub fn observation_sharing_burst_limit(&self) -> NonZeroU32 {
        self.observation_sharing_burst_limit
            .unwrap_or(nonzero!(100u32))
    }

    pub fn enable_gas_price_weighting(&self) -> bool {
        self.enable_gas_price_weighting.unwrap_or(false)
    }

    pub fn weighted_moving_average_window_size(&self) -> usize {
        self.weighted_moving_average_window_size.unwrap_or(20)
    }

    #[cfg(msim)]
    pub fn inject_synthetic_execution_time(&self) -> bool {
        self.inject_synthetic_execution_time.unwrap_or(false)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionCacheConfig {
    PassthroughCache,
    WritebackCache {
        /// Maximum number of entries in each cache. (There are several different caches).
        /// If None, the default of 10000 is used.
        max_cache_size: Option<u64>,

        package_cache_size: Option<u64>, // defaults to 1000

        object_cache_size: Option<u64>, // defaults to max_cache_size
        marker_cache_size: Option<u64>, // defaults to object_cache_size
        object_by_id_cache_size: Option<u64>, // defaults to object_cache_size

        transaction_cache_size: Option<u64>, // defaults to max_cache_size
        executed_effect_cache_size: Option<u64>, // defaults to transaction_cache_size
        effect_cache_size: Option<u64>,      // defaults to executed_effect_cache_size

        events_cache_size: Option<u64>, // defaults to transaction_cache_size

        transaction_objects_cache_size: Option<u64>, // defaults to 1000

        /// Number of uncommitted transactions at which to pause consensus handler.
        backpressure_threshold: Option<u64>,

        /// Number of uncommitted transactions at which to refuse new transaction
        /// submissions. Defaults to backpressure_threshold if unset.
        backpressure_threshold_for_rpc: Option<u64>,

        fastpath_transaction_outputs_cache_size: Option<u64>,
    },
}

impl Default for ExecutionCacheConfig {
    fn default() -> Self {
        ExecutionCacheConfig::WritebackCache {
            max_cache_size: None,
            backpressure_threshold: None,
            backpressure_threshold_for_rpc: None,
            package_cache_size: None,
            object_cache_size: None,
            marker_cache_size: None,
            object_by_id_cache_size: None,
            transaction_cache_size: None,
            executed_effect_cache_size: None,
            effect_cache_size: None,
            events_cache_size: None,
            transaction_objects_cache_size: None,
            fastpath_transaction_outputs_cache_size: None,
        }
    }
}

impl ExecutionCacheConfig {
    pub fn max_cache_size(&self) -> u64 {
        std::env::var("SUI_MAX_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache { max_cache_size, .. } => {
                    max_cache_size.unwrap_or(100000)
                }
            })
    }

    pub fn package_cache_size(&self) -> u64 {
        std::env::var("SUI_PACKAGE_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    package_cache_size, ..
                } => package_cache_size.unwrap_or(1000),
            })
    }

    pub fn object_cache_size(&self) -> u64 {
        std::env::var("SUI_OBJECT_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    object_cache_size, ..
                } => object_cache_size.unwrap_or(self.max_cache_size()),
            })
    }

    pub fn marker_cache_size(&self) -> u64 {
        std::env::var("SUI_MARKER_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    marker_cache_size, ..
                } => marker_cache_size.unwrap_or(self.object_cache_size()),
            })
    }

    pub fn object_by_id_cache_size(&self) -> u64 {
        std::env::var("SUI_OBJECT_BY_ID_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    object_by_id_cache_size,
                    ..
                } => object_by_id_cache_size.unwrap_or(self.object_cache_size()),
            })
    }

    pub fn transaction_cache_size(&self) -> u64 {
        std::env::var("SUI_TRANSACTION_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    transaction_cache_size,
                    ..
                } => transaction_cache_size.unwrap_or(self.max_cache_size()),
            })
    }

    pub fn executed_effect_cache_size(&self) -> u64 {
        std::env::var("SUI_EXECUTED_EFFECT_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    executed_effect_cache_size,
                    ..
                } => executed_effect_cache_size.unwrap_or(self.transaction_cache_size()),
            })
    }

    pub fn effect_cache_size(&self) -> u64 {
        std::env::var("SUI_EFFECT_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    effect_cache_size, ..
                } => effect_cache_size.unwrap_or(self.executed_effect_cache_size()),
            })
    }

    pub fn events_cache_size(&self) -> u64 {
        std::env::var("SUI_EVENTS_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    events_cache_size, ..
                } => events_cache_size.unwrap_or(self.transaction_cache_size()),
            })
    }

    pub fn transaction_objects_cache_size(&self) -> u64 {
        std::env::var("SUI_TRANSACTION_OBJECTS_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    transaction_objects_cache_size,
                    ..
                } => transaction_objects_cache_size.unwrap_or(1000),
            })
    }

    pub fn backpressure_threshold(&self) -> u64 {
        std::env::var("SUI_BACKPRESSURE_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    backpressure_threshold,
                    ..
                } => backpressure_threshold.unwrap_or(100_000),
            })
    }

    pub fn backpressure_threshold_for_rpc(&self) -> u64 {
        std::env::var("SUI_BACKPRESSURE_THRESHOLD_FOR_RPC")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    backpressure_threshold_for_rpc,
                    ..
                } => backpressure_threshold_for_rpc.unwrap_or(self.backpressure_threshold()),
            })
    }

    pub fn fastpath_transaction_outputs_cache_size(&self) -> u64 {
        std::env::var("SUI_FASTPATH_TRANSACTION_OUTPUTS_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| match self {
                ExecutionCacheConfig::PassthroughCache => fatal!("invalid cache config"),
                ExecutionCacheConfig::WritebackCache {
                    fastpath_transaction_outputs_cache_size,
                    ..
                } => fastpath_transaction_outputs_cache_size.unwrap_or(10_000),
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerType {
    WebSocket,
    Http,
    Both,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransactionKeyValueStoreReadConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,

    #[serde(default = "default_cache_size")]
    pub cache_size: u64,
}

impl Default for TransactionKeyValueStoreReadConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            cache_size: default_cache_size(),
        }
    }
}

fn default_base_url() -> String {
    "https://transactions.sui.io/".to_string()
}

fn default_cache_size() -> u64 {
    100_000
}

fn default_jwk_fetch_interval_seconds() -> u64 {
    3600
}

pub fn default_zklogin_oauth_providers() -> BTreeMap<Chain, BTreeSet<String>> {
    let mut map = BTreeMap::new();

    // providers that are available on devnet only.
    let experimental_providers = BTreeSet::from([
        "Google".to_string(),
        "Facebook".to_string(),
        "Twitch".to_string(),
        "Kakao".to_string(),
        "Apple".to_string(),
        "Slack".to_string(),
        "TestIssuer".to_string(),
        "Microsoft".to_string(),
        "KarrierOne".to_string(),
        "Credenza3".to_string(),
        "Playtron".to_string(),
        "Threedos".to_string(),
        "Onefc".to_string(),
        "FanTV".to_string(),
        "AwsTenant-region:us-east-1-tenant_id:us-east-1_qPsZxYqd8".to_string(), // Ambrus, external partner
        "Arden".to_string(),                                                    // Arden partner
        "AwsTenant-region:eu-west-3-tenant_id:eu-west-3_gGVCx53Es".to_string(), // Trace, external partner
        "EveFrontier".to_string(),
        "TestEveFrontier".to_string(),
        "AwsTenant-region:ap-southeast-1-tenant_id:ap-southeast-1_2QQPyQXDz".to_string(), // Decot, external partner
    ]);

    // providers that are available for mainnet and testnet.
    let providers = BTreeSet::from([
        "Google".to_string(),
        "Facebook".to_string(),
        "Twitch".to_string(),
        "Apple".to_string(),
        "AwsTenant-region:us-east-1-tenant_id:us-east-1_qPsZxYqd8".to_string(), // Ambrus, external partner
        "KarrierOne".to_string(),
        "Credenza3".to_string(),
        "Playtron".to_string(),
        "Onefc".to_string(),
        "Threedos".to_string(),
        "AwsTenant-region:eu-west-3-tenant_id:eu-west-3_gGVCx53Es".to_string(), // Trace, external partner
        "Arden".to_string(),
        "FanTV".to_string(),
        "EveFrontier".to_string(),
        "TestEveFrontier".to_string(),
        "AwsTenant-region:ap-southeast-1-tenant_id:ap-southeast-1_2QQPyQXDz".to_string(), // Decot, external partner
    ]);
    map.insert(Chain::Mainnet, providers.clone());
    map.insert(Chain::Testnet, providers);
    map.insert(Chain::Unknown, experimental_providers);
    map
}

fn default_transaction_kv_store_config() -> TransactionKeyValueStoreReadConfig {
    TransactionKeyValueStoreReadConfig::default()
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

pub fn default_concurrency_limit() -> Option<usize> {
    Some(DEFAULT_GRPC_CONCURRENCY_LIMIT)
}

pub fn default_end_of_epoch_broadcast_channel_capacity() -> usize {
    128
}

pub fn bool_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
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

    pub fn archive_path(&self) -> PathBuf {
        self.db_path.join("archive")
    }

    pub fn snapshot_path(&self) -> PathBuf {
        self.db_path.join("snapshot")
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

    pub fn sui_address(&self) -> SuiAddress {
        (&self.account_key_pair.keypair().public()).into()
    }

    pub fn archive_reader_config(&self) -> Option<ArchiveReaderConfig> {
        self.state_archive_read_config
            .first()
            .map(|config| ArchiveReaderConfig {
                ingestion_url: config.ingestion_url.clone(),
                remote_store_options: config.remote_store_options.clone(),
                download_concurrency: NonZeroUsize::new(config.concurrency)
                    .unwrap_or(NonZeroUsize::new(5).unwrap()),
                remote_store_config: ObjectStoreConfig::default(),
            })
    }

    pub fn jsonrpc_server_type(&self) -> ServerType {
        self.jsonrpc_server_type.unwrap_or(ServerType::Http)
    }

    pub fn rpc(&self) -> Option<&crate::RpcConfig> {
        self.rpc.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ConsensusProtocol {
    #[serde(rename = "narwhal")]
    Narwhal,
    #[serde(rename = "mysticeti")]
    Mysticeti,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsensusConfig {
    // Base consensus DB path for all epochs.
    pub db_path: PathBuf,

    // The number of epochs for which to retain the consensus DBs. Setting it to 0 will make a consensus DB getting
    // dropped as soon as system is switched to a new epoch.
    pub db_retention_epochs: Option<u64>,

    // Pruner will run on every epoch change but it will also check periodically on every `db_pruner_period_secs`
    // seconds to see if there are any epoch DBs to remove.
    pub db_pruner_period_secs: Option<u64>,

    /// Maximum number of pending transactions to submit to consensus, including those
    /// in submission wait.
    /// Default to 20_000 inflight limit, assuming 20_000 txn tps * 1 sec consensus latency.
    pub max_pending_transactions: Option<usize>,

    /// When defined caps the calculated submission position to the max_submit_position. Even if the
    /// is elected to submit from a higher position than this, it will "reset" to the max_submit_position.
    pub max_submit_position: Option<usize>,

    /// The submit delay step to consensus defined in milliseconds. When provided it will
    /// override the current back off logic otherwise the default backoff logic will be applied based
    /// on consensus latency estimates.
    pub submit_delay_step_override_millis: Option<u64>,

    pub parameters: Option<ConsensusParameters>,
}

impl ConsensusConfig {
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn max_pending_transactions(&self) -> usize {
        self.max_pending_transactions.unwrap_or(20_000)
    }

    pub fn submit_delay_step_override(&self) -> Option<Duration> {
        self.submit_delay_step_override_millis
            .map(Duration::from_millis)
    }

    pub fn db_retention_epochs(&self) -> u64 {
        self.db_retention_epochs.unwrap_or(0)
    }

    pub fn db_pruner_period(&self) -> Duration {
        // Default to 1 hour
        self.db_pruner_period_secs
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(3_600))
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

    /// Optional directory used for data ingestion pipeline
    /// When specified, each executed checkpoint will be saved in a local directory for post processing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_ingestion_dir: Option<PathBuf>,
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

    #[serde(default)]
    enable_secondary_index_checks: bool,
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
            enable_secondary_index_checks: false, // Disable by default for now
        }
    }

    pub fn new_disable_all() -> Self {
        Self {
            enable_epoch_sui_conservation_check: false,
            enable_deep_per_tx_sui_conservation_check: false,
            force_disable_epoch_sui_conservation_check: true,
            enable_state_consistency_check: false,
            force_disable_state_consistency_check: true,
            enable_secondary_index_checks: false,
        }
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

    pub fn enable_deep_per_tx_sui_conservation_check(&self) -> bool {
        self.enable_deep_per_tx_sui_conservation_check || cfg!(debug_assertions)
    }

    pub fn enable_secondary_index_checks(&self) -> bool {
        self.enable_secondary_index_checks
    }
}

fn default_checkpoint_execution_max_concurrency() -> usize {
    4
}

fn default_local_execution_timeout_sec() -> u64 {
    30
}

impl Default for CheckpointExecutorConfig {
    fn default() -> Self {
        Self {
            checkpoint_execution_max_concurrency: default_checkpoint_execution_max_concurrency(),
            local_execution_timeout_sec: default_local_execution_timeout_sec(),
            data_ingestion_dir: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthorityStorePruningConfig {
    /// number of the latest epoch dbs to retain
    #[serde(default = "default_num_latest_epoch_dbs_to_retain")]
    pub num_latest_epoch_dbs_to_retain: usize,
    /// time interval used by the pruner to determine whether there are any epoch DBs to remove
    #[serde(default = "default_epoch_db_pruning_period_secs")]
    pub epoch_db_pruning_period_secs: u64,
    /// number of epochs to keep the latest version of objects for.
    /// Note that a zero value corresponds to an aggressive pruner.
    /// This mode is experimental and needs to be used with caution.
    /// Use `u64::MAX` to disable the pruner for the objects.
    #[serde(default)]
    pub num_epochs_to_retain: u64,
    /// pruner's runtime interval used for aggressive mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruning_run_delay_seconds: Option<u64>,
    /// maximum number of checkpoints in the pruning batch. Can be adjusted to increase performance
    #[serde(default = "default_max_checkpoints_in_batch")]
    pub max_checkpoints_in_batch: usize,
    /// maximum number of transaction in the pruning batch
    #[serde(default = "default_max_transactions_in_batch")]
    pub max_transactions_in_batch: usize,
    /// enables periodic background compaction for old SST files whose last modified time is
    /// older than `periodic_compaction_threshold_days` days.
    /// That ensures that all sst files eventually go through the compaction process
    #[serde(
        default = "default_periodic_compaction_threshold_days",
        skip_serializing_if = "Option::is_none"
    )]
    pub periodic_compaction_threshold_days: Option<usize>,
    /// number of epochs to keep the latest version of transactions and effects for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_epochs_to_retain_for_checkpoints: Option<u64>,
    /// disables object tombstone pruning. We don't serialize it if it is the default value, false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub killswitch_tombstone_pruning: bool,
    #[serde(default = "default_smoothing", skip_serializing_if = "is_true")]
    pub smooth: bool,
    /// Enables the compaction filter for pruning the objects table.
    /// If disabled, a range deletion approach is used instead.
    /// While it is generally safe to switch between the two modes,
    /// switching from the compaction filter approach back to range deletion
    /// may result in some old versions that will never be pruned.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enable_compaction_filter: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_epochs_to_retain_for_indexes: Option<u64>,
}

fn default_num_latest_epoch_dbs_to_retain() -> usize {
    3
}

fn default_epoch_db_pruning_period_secs() -> u64 {
    3600
}

fn default_max_transactions_in_batch() -> usize {
    1000
}

fn default_max_checkpoints_in_batch() -> usize {
    10
}

fn default_smoothing() -> bool {
    cfg!(not(test))
}

fn default_periodic_compaction_threshold_days() -> Option<usize> {
    Some(1)
}

impl Default for AuthorityStorePruningConfig {
    fn default() -> Self {
        Self {
            num_latest_epoch_dbs_to_retain: default_num_latest_epoch_dbs_to_retain(),
            epoch_db_pruning_period_secs: default_epoch_db_pruning_period_secs(),
            num_epochs_to_retain: 0,
            pruning_run_delay_seconds: if cfg!(msim) { Some(2) } else { None },
            max_checkpoints_in_batch: default_max_checkpoints_in_batch(),
            max_transactions_in_batch: default_max_transactions_in_batch(),
            periodic_compaction_threshold_days: None,
            num_epochs_to_retain_for_checkpoints: if cfg!(msim) { Some(2) } else { None },
            killswitch_tombstone_pruning: false,
            smooth: true,
            enable_compaction_filter: cfg!(test) || cfg!(msim),
            num_epochs_to_retain_for_indexes: None,
        }
    }
}

impl AuthorityStorePruningConfig {
    pub fn set_num_epochs_to_retain(&mut self, num_epochs_to_retain: u64) {
        self.num_epochs_to_retain = num_epochs_to_retain;
    }

    pub fn set_num_epochs_to_retain_for_checkpoints(&mut self, num_epochs_to_retain: Option<u64>) {
        self.num_epochs_to_retain_for_checkpoints = num_epochs_to_retain;
    }

    pub fn num_epochs_to_retain_for_checkpoints(&self) -> Option<u64> {
        self.num_epochs_to_retain_for_checkpoints
            // if n less than 2, coerce to 2 and log
            .map(|n| {
                if n < 2 {
                    info!("num_epochs_to_retain_for_checkpoints must be at least 2, rounding up from {}", n);
                    2
                } else {
                    n
                }
            })
    }

    pub fn set_killswitch_tombstone_pruning(&mut self, killswitch_tombstone_pruning: bool) {
        self.killswitch_tombstone_pruning = killswitch_tombstone_pruning;
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

#[derive(Debug, Clone)]
pub struct ArchiveReaderConfig {
    pub remote_store_config: ObjectStoreConfig,
    pub download_concurrency: NonZeroUsize,
    pub ingestion_url: Option<String>,
    pub remote_store_options: Vec<(String, String)>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateArchiveConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_store_config: Option<ObjectStoreConfig>,
    pub concurrency: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingestion_url: Option<String>,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        default,
        deserialize_with = "deserialize_remote_store_options"
    )]
    pub remote_store_options: Vec<(String, String)>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateSnapshotConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_store_config: Option<ObjectStoreConfig>,
    pub concurrency: usize,
    /// Archive snapshots every N epochs. If set to 0, archival is disabled.
    /// Archived snapshots are copied to `archive/epoch_<N>/` in the same bucket
    /// and are intended to be kept indefinitely.
    #[serde(default)]
    pub archive_interval_epochs: u64,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransactionKeyValueStoreWriteConfig {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_region: String,
    pub table_name: String,
    pub bucket_name: String,
    pub concurrency: usize,
}

/// Configuration for the threshold(s) at which we consider the system
/// to be overloaded. When one of the threshold is passed, the node may
/// stop processing new transactions and/or certificates until the congestion
/// resolves.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthorityOverloadConfig {
    #[serde(default = "default_max_txn_age_in_queue")]
    pub max_txn_age_in_queue: Duration,

    // The interval of checking overload signal.
    #[serde(default = "default_overload_monitor_interval")]
    pub overload_monitor_interval: Duration,

    // The execution queueing latency when entering load shedding mode.
    #[serde(default = "default_execution_queue_latency_soft_limit")]
    pub execution_queue_latency_soft_limit: Duration,

    // The execution queueing latency when entering aggressive load shedding mode.
    #[serde(default = "default_execution_queue_latency_hard_limit")]
    pub execution_queue_latency_hard_limit: Duration,

    // The maximum percentage of transactions to shed in load shedding mode.
    #[serde(default = "default_max_load_shedding_percentage")]
    pub max_load_shedding_percentage: u32,

    // When in aggressive load shedding mode, the minimum percentage of
    // transactions to shed.
    #[serde(default = "default_min_load_shedding_percentage_above_hard_limit")]
    pub min_load_shedding_percentage_above_hard_limit: u32,

    // If transaction ready rate is below this rate, we consider the validator
    // is well under used, and will not enter load shedding mode.
    #[serde(default = "default_safe_transaction_ready_rate")]
    pub safe_transaction_ready_rate: u32,

    // When set to true, transaction signing may be rejected when the validator
    // is overloaded.
    #[serde(default = "default_check_system_overload_at_signing")]
    pub check_system_overload_at_signing: bool,

    // When set to true, transaction execution may be rejected when the validator
    // is overloaded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub check_system_overload_at_execution: bool,

    // Reject a transaction if transaction manager queue length is above this threshold.
    // 100_000 = 10k TPS * 5s resident time in transaction manager (pending + executing) * 2.
    #[serde(default = "default_max_transaction_manager_queue_length")]
    pub max_transaction_manager_queue_length: usize,

    // Reject a transaction if the number of pending transactions depending on the object
    // is above the threshold.
    #[serde(default = "default_max_transaction_manager_per_object_queue_length")]
    pub max_transaction_manager_per_object_queue_length: usize,
}

fn default_max_txn_age_in_queue() -> Duration {
    Duration::from_millis(1000)
}

fn default_overload_monitor_interval() -> Duration {
    Duration::from_secs(10)
}

fn default_execution_queue_latency_soft_limit() -> Duration {
    Duration::from_secs(1)
}

fn default_execution_queue_latency_hard_limit() -> Duration {
    Duration::from_secs(10)
}

fn default_max_load_shedding_percentage() -> u32 {
    95
}

fn default_min_load_shedding_percentage_above_hard_limit() -> u32 {
    50
}

fn default_safe_transaction_ready_rate() -> u32 {
    100
}

fn default_check_system_overload_at_signing() -> bool {
    true
}

fn default_max_transaction_manager_queue_length() -> usize {
    100_000
}

fn default_max_transaction_manager_per_object_queue_length() -> usize {
    2000
}

impl Default for AuthorityOverloadConfig {
    fn default() -> Self {
        Self {
            max_txn_age_in_queue: default_max_txn_age_in_queue(),
            overload_monitor_interval: default_overload_monitor_interval(),
            execution_queue_latency_soft_limit: default_execution_queue_latency_soft_limit(),
            execution_queue_latency_hard_limit: default_execution_queue_latency_hard_limit(),
            max_load_shedding_percentage: default_max_load_shedding_percentage(),
            min_load_shedding_percentage_above_hard_limit:
                default_min_load_shedding_percentage_above_hard_limit(),
            safe_transaction_ready_rate: default_safe_transaction_ready_rate(),
            check_system_overload_at_signing: true,
            check_system_overload_at_execution: false,
            max_transaction_manager_queue_length: default_max_transaction_manager_queue_length(),
            max_transaction_manager_per_object_queue_length:
                default_max_transaction_manager_per_object_queue_length(),
        }
    }
}

fn default_authority_overload_config() -> AuthorityOverloadConfig {
    AuthorityOverloadConfig::default()
}

fn default_traffic_controller_policy_config() -> Option<PolicyConfig> {
    Some(PolicyConfig::default_dos_protection_policy())
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
#[allow(clippy::large_enum_variant)]
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

/// Configurations which determine how we dump state debug info.
/// Debug info is dumped when a node forks.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct StateDebugDumpConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dump_file_directory: Option<PathBuf>,
}

fn read_credential_from_path_or_literal(value: &str) -> Result<String, std::io::Error> {
    let path = Path::new(value);
    if path.exists() && path.is_file() {
        std::fs::read_to_string(path).map(|content| content.trim().to_string())
    } else {
        Ok(value.to_string())
    }
}

// Custom deserializer for remote store options that supports file paths or literal values
fn deserialize_remote_store_options<'de, D>(
    deserializer: D,
) -> Result<Vec<(String, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let raw_options: Vec<(String, String)> = Vec::deserialize(deserializer)?;
    let mut processed_options = Vec::new();

    for (key, value) in raw_options {
        // GCS service_account keys expect a file path, not the file content
        // All other keys (AWS credentials, service_account_key) should read file content
        let is_service_account_path = matches!(
            key.as_str(),
            "google_service_account"
                | "service_account"
                | "google_service_account_path"
                | "service_account_path"
        );

        let processed_value = if is_service_account_path {
            value
        } else {
            match read_credential_from_path_or_literal(&value) {
                Ok(processed) => processed,
                Err(e) => {
                    return Err(D::Error::custom(format!(
                        "Failed to read credential for key '{}': {}",
                        key, e
                    )));
                }
            }
        };

        processed_options.push((key, processed_value));
    }

    Ok(processed_options)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fastcrypto::traits::KeyPair;
    use rand::{SeedableRng, rngs::StdRng};
    use sui_keys::keypair_file::{write_authority_keypair_to_file, write_keypair_to_file};
    use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair, SuiKeyPair, get_key_pair_from_rng};

    use super::{Genesis, StateArchiveConfig};
    use crate::NodeConfig;

    #[test]
    fn serialize_genesis_from_file() {
        let g = Genesis::new_from_file("path/to/file");

        let s = serde_yaml::to_string(&g).unwrap();
        assert_eq!("---\ngenesis-file-location: path/to/file\n", s);
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn fullnode_template() {
        const TEMPLATE: &str = include_str!("../data/fullnode-template.yaml");

        let _template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
    }

    /// Tests that a legacy validator config (captured on 12/06/2024) can be parsed.
    #[test]
    fn legacy_validator_config() {
        const FILE: &str = include_str!("../data/sui-node-legacy.yaml");

        let _template: NodeConfig = serde_yaml::from_str(FILE).unwrap();
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

    #[test]
    fn test_remote_store_options_file_path_support() {
        // Create temporary credential files
        let temp_dir = std::env::temp_dir();
        let access_key_file = temp_dir.join("test_access_key");
        let secret_key_file = temp_dir.join("test_secret_key");

        std::fs::write(&access_key_file, "test_access_key_value").unwrap();
        std::fs::write(&secret_key_file, "test_secret_key_value\n").unwrap();

        let yaml_config = format!(
            r#"
object-store-config: null
concurrency: 5
ingestion-url: "https://example.com"
remote-store-options:
  - ["aws_access_key_id", "{}"]
  - ["aws_secret_access_key", "{}"]
  - ["literal_key", "literal_value"]
"#,
            access_key_file.to_string_lossy(),
            secret_key_file.to_string_lossy()
        );

        let config: StateArchiveConfig = serde_yaml::from_str(&yaml_config).unwrap();

        // Verify that file paths were resolved and literal values preserved
        assert_eq!(config.remote_store_options.len(), 3);

        let access_key_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "aws_access_key_id")
            .unwrap();
        assert_eq!(access_key_option.1, "test_access_key_value");

        let secret_key_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "aws_secret_access_key")
            .unwrap();
        assert_eq!(secret_key_option.1, "test_secret_key_value");

        let literal_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "literal_key")
            .unwrap();
        assert_eq!(literal_option.1, "literal_value");

        // Clean up
        std::fs::remove_file(&access_key_file).ok();
        std::fs::remove_file(&secret_key_file).ok();
    }

    #[test]
    fn test_remote_store_options_literal_values_only() {
        let yaml_config = r#"
object-store-config: null
concurrency: 5
ingestion-url: "https://example.com"
remote-store-options:
  - ["aws_access_key_id", "literal_access_key"]
  - ["aws_secret_access_key", "literal_secret_key"]
"#;

        let config: StateArchiveConfig = serde_yaml::from_str(yaml_config).unwrap();

        assert_eq!(config.remote_store_options.len(), 2);
        assert_eq!(config.remote_store_options[0].1, "literal_access_key");
        assert_eq!(config.remote_store_options[1].1, "literal_secret_key");
    }

    #[test]
    fn test_remote_store_options_gcs_service_account_path_preserved() {
        let temp_dir = std::env::temp_dir();
        let service_account_file = temp_dir.join("test_service_account.json");
        let aws_key_file = temp_dir.join("test_aws_key");

        std::fs::write(&service_account_file, r#"{"type": "service_account"}"#).unwrap();
        std::fs::write(&aws_key_file, "aws_key_value").unwrap();

        let yaml_config = format!(
            r#"
object-store-config: null
concurrency: 5
ingestion-url: "gs://my-bucket"
remote-store-options:
  - ["service_account", "{}"]
  - ["google_service_account_path", "{}"]
  - ["aws_access_key_id", "{}"]
"#,
            service_account_file.to_string_lossy(),
            service_account_file.to_string_lossy(),
            aws_key_file.to_string_lossy()
        );

        let config: StateArchiveConfig = serde_yaml::from_str(&yaml_config).unwrap();

        assert_eq!(config.remote_store_options.len(), 3);

        // service_account should preserve the file path, not read the content
        let service_account_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "service_account")
            .unwrap();
        assert_eq!(
            service_account_option.1,
            service_account_file.to_string_lossy()
        );

        // google_service_account_path should also preserve the file path
        let gcs_path_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "google_service_account_path")
            .unwrap();
        assert_eq!(gcs_path_option.1, service_account_file.to_string_lossy());

        // AWS key should read the file content
        let aws_option = config
            .remote_store_options
            .iter()
            .find(|(key, _)| key == "aws_access_key_id")
            .unwrap();
        assert_eq!(aws_option.1, "aws_key_value");

        // Clean up
        std::fs::remove_file(&service_account_file).ok();
        std::fs::remove_file(&aws_key_file).ok();
    }
}

// RunWithRange is used to specify the ending epoch/checkpoint to process.
// this is intended for use with disaster recovery debugging and verification workflows, never in normal operations
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum RunWithRange {
    Epoch(EpochId),
    Checkpoint(CheckpointSequenceNumber),
}

impl RunWithRange {
    // is epoch_id > RunWithRange::Epoch
    pub fn is_epoch_gt(&self, epoch_id: EpochId) -> bool {
        matches!(self, RunWithRange::Epoch(e) if epoch_id > *e)
    }

    pub fn matches_checkpoint(&self, seq_num: CheckpointSequenceNumber) -> bool {
        matches!(self, RunWithRange::Checkpoint(seq) if *seq == seq_num)
    }

    pub fn into_checkpoint_bound(self) -> Option<CheckpointSequenceNumber> {
        match self {
            RunWithRange::Epoch(_) => None,
            RunWithRange::Checkpoint(seq) => Some(seq),
        }
    }
}
