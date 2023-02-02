// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![allow(clippy::mutable_key_type)]

use arc_swap::ArcSwap;
use crypto::{NetworkPublicKey, PublicKey};
use fastcrypto::traits::EncodeDecodeBase64;
use multiaddr::Multiaddr;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufWriter, Write as _},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tracing::info;
use utils::get_available_port;

mod duration_format;
pub mod utils;

/// The epoch number.
pub type Epoch = u64;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Node {0} is not in the committee")]
    NotInCommittee(String),

    #[error("Node {0} is not in the worker cache")]
    NotInWorkerCache(String),

    #[error("Unknown worker id {0}")]
    UnknownWorker(WorkerId),

    #[error("Failed to read config file '{file}': {message}")]
    ImportError { file: String, message: String },

    #[error("Failed to write config file '{file}': {message}")]
    ExportError { file: String, message: String },
}

#[derive(Error, Debug)]
pub enum CommitteeUpdateError {
    #[error("Node {0} is not in the committee")]
    NotInCommittee(String),

    #[error("Node {0} was not in the update")]
    MissingFromUpdate(String),

    #[error("Node {0} has a different stake than expected")]
    DifferentStake(String),
}

pub trait Import: DeserializeOwned {
    fn import(path: &str) -> Result<Self, ConfigError> {
        let reader = || -> Result<Self, std::io::Error> {
            let data = fs::read(path)?;
            Ok(serde_json::from_slice(data.as_slice())?)
        };
        reader().map_err(|e| ConfigError::ImportError {
            file: path.to_string(),
            message: e.to_string(),
        })
    }
}

impl<D: DeserializeOwned> Import for D {}

pub trait Export: Serialize {
    fn export(&self, path: &str) -> Result<(), ConfigError> {
        let writer = || -> Result<(), std::io::Error> {
            let file = OpenOptions::new().create(true).write(true).open(path)?;
            let mut writer = BufWriter::new(file);
            let data = serde_json::to_string_pretty(self).unwrap();
            writer.write_all(data.as_ref())?;
            writer.write_all(b"\n")?;
            Ok(())
        };
        writer().map_err(|e| ConfigError::ExportError {
            file: path.to_string(),
            message: e.to_string(),
        })
    }
}

impl<S: Serialize> Export for S {}

// TODO: the stake and voting power of a validator can be different so
// in some places when we are actually referring to the voting power, we
// should use a different type alias, field name, etc.
// Also, consider unify this with `StakeUnit` on Sui side.
pub type Stake = u64;
pub type WorkerId = u32;

/// Holds all the node properties. An example is provided to
/// showcase the usage and deserialization from a json file.
/// To define a Duration on the property file can use either
/// milliseconds or seconds (e.x 5s, 10ms , 2000ms).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parameters {
    /// When the primary has `header_num_of_batches_threshold` num of batch digests available,
    /// then it can propose a new header.
    #[serde(default = "Parameters::default_header_num_of_batches_threshold")]
    pub header_num_of_batches_threshold: usize,

    /// The maximum number of batch digests included in a header.
    #[serde(default = "Parameters::default_max_header_num_of_batches")]
    pub max_header_num_of_batches: usize,

    /// The maximum delay that the primary should wait between generating two headers, even if
    /// other conditions are not satisfied besides having enough parent stakes.
    #[serde(
        with = "duration_format",
        default = "Parameters::default_max_header_delay"
    )]
    pub max_header_delay: Duration,
    /// When the delay from last header reaches `min_header_delay`, a new header can be proposed
    /// even if batches have not reached `header_num_of_batches_threshold`.
    #[serde(
        with = "duration_format",
        default = "Parameters::default_min_header_delay"
    )]
    pub min_header_delay: Duration,

    /// The depth of the garbage collection (Denominated in number of rounds).
    pub gc_depth: u64,
    /// The delay after which the synchronizer retries to send sync requests. Denominated in ms.
    #[serde(with = "duration_format")]
    pub sync_retry_delay: Duration,
    /// Determine with how many nodes to sync when re-trying to send sync-request. These nodes
    /// are picked at random from the committee.
    pub sync_retry_nodes: usize,
    /// The preferred batch size. The workers seal a batch of transactions when it reaches this size.
    /// Denominated in bytes.
    pub batch_size: usize,
    /// The delay after which the workers seal a batch of transactions, even if `max_batch_size`
    /// is not reached.
    #[serde(with = "duration_format")]
    pub max_batch_delay: Duration,
    /// The parameters for the block synchronizer
    pub block_synchronizer: BlockSynchronizerParameters,
    /// The parameters for the Consensus API gRPC server
    pub consensus_api_grpc: ConsensusAPIGrpcParameters,
    /// The maximum number of concurrent requests for messages accepted from an un-trusted entity
    pub max_concurrent_requests: usize,
    /// Properties for the prometheus metrics
    pub prometheus_metrics: PrometheusMetricsParameters,
    /// Network admin server ports for primary & worker.
    pub network_admin_server: NetworkAdminServerParameters,
    /// Anemo network settings.
    #[serde(default = "AnemoParameters::default")]
    pub anemo: AnemoParameters,
}

impl Parameters {
    fn default_header_num_of_batches_threshold() -> usize {
        32
    }

    fn default_max_header_num_of_batches() -> usize {
        1_000
    }

    fn default_max_header_delay() -> Duration {
        Duration::from_secs(2)
    }

    fn default_min_header_delay() -> Duration {
        Duration::from_secs_f64(1.8)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkAdminServerParameters {
    /// Primary network admin server port number
    pub primary_network_admin_server_port: u16,
    /// Worker network admin server base port number
    pub worker_network_admin_server_base_port: u16,
}

impl Default for NetworkAdminServerParameters {
    fn default() -> Self {
        let host = "127.0.0.1";
        Self {
            primary_network_admin_server_port: get_available_port(host),
            worker_network_admin_server_base_port: get_available_port(host),
        }
    }
}

impl NetworkAdminServerParameters {
    fn with_available_port(&self) -> Self {
        let mut params = self.clone();
        let default = Self::default();
        params.primary_network_admin_server_port = default.primary_network_admin_server_port;
        params.worker_network_admin_server_base_port =
            default.worker_network_admin_server_base_port;
        params
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AnemoParameters {
    /// Per-peer rate-limits (in requests/sec) for the PrimaryToPrimary service.
    pub send_message_rate_limit: Option<NonZeroU32>,
    pub get_payload_availability_rate_limit: Option<NonZeroU32>,
    pub get_certificates_rate_limit: Option<NonZeroU32>,

    /// Per-peer rate-limits (in requests/sec) for the WorkerToWorker service.
    pub report_batch_rate_limit: Option<NonZeroU32>,
    pub request_batch_rate_limit: Option<NonZeroU32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PrometheusMetricsParameters {
    /// Socket address the server should be listening to.
    pub socket_addr: Multiaddr,
}

impl Default for PrometheusMetricsParameters {
    fn default() -> Self {
        let host = "127.0.0.1";
        Self {
            socket_addr: format!("/ip4/{}/tcp/{}/http", host, get_available_port(host))
                .parse()
                .unwrap(),
        }
    }
}

impl PrometheusMetricsParameters {
    fn with_available_port(&self) -> Self {
        let mut params = self.clone();
        let default = Self::default();
        params.socket_addr = default.socket_addr;
        params
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConsensusAPIGrpcParameters {
    /// Socket address the server should be listening to.
    pub socket_addr: Multiaddr,
    /// The timeout configuration when requesting batches from workers.
    #[serde(with = "duration_format")]
    pub get_collections_timeout: Duration,
    /// The timeout configuration when removing batches from workers.
    #[serde(with = "duration_format")]
    pub remove_collections_timeout: Duration,
}

impl Default for ConsensusAPIGrpcParameters {
    fn default() -> Self {
        let host = "127.0.0.1";
        Self {
            socket_addr: format!("/ip4/{}/tcp/{}/http", host, get_available_port(host))
                .parse()
                .unwrap(),
            get_collections_timeout: Duration::from_millis(5_000),
            remove_collections_timeout: Duration::from_millis(5_000),
        }
    }
}

impl ConsensusAPIGrpcParameters {
    fn with_available_port(&self) -> Self {
        let mut params = self.clone();
        let default = Self::default();
        params.socket_addr = default.socket_addr;
        params
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct BlockSynchronizerParameters {
    /// The timeout configuration for synchronizing certificate digests from a starting round.
    #[serde(
        with = "duration_format",
        default = "BlockSynchronizerParameters::default_range_synchronize_timeout"
    )]
    pub range_synchronize_timeout: Duration,
    /// The timeout configuration when requesting certificates from peers.
    #[serde(
        with = "duration_format",
        default = "BlockSynchronizerParameters::default_certificates_synchronize_timeout"
    )]
    pub certificates_synchronize_timeout: Duration,
    /// Timeout when has requested the payload for a certificate and is
    /// waiting to receive them.
    #[serde(
        with = "duration_format",
        default = "BlockSynchronizerParameters::default_payload_synchronize_timeout"
    )]
    pub payload_synchronize_timeout: Duration,
    /// The timeout configuration when for when we ask the other peers to
    /// discover who has the payload available for the dictated certificates.
    #[serde(
        with = "duration_format",
        default = "BlockSynchronizerParameters::default_payload_availability_timeout"
    )]
    pub payload_availability_timeout: Duration,
    /// When a certificate is fetched on the fly from peers, it is submitted
    /// from the block synchronizer handler for further processing to core
    /// to validate and ensure parents are available and history is causal
    /// complete. This property is the timeout while we wait for core to
    /// perform this processes and the certificate to become available to
    /// the handler to consume.
    #[serde(
        with = "duration_format",
        default = "BlockSynchronizerParameters::default_handler_certificate_deliver_timeout"
    )]
    pub handler_certificate_deliver_timeout: Duration,
}

impl BlockSynchronizerParameters {
    fn default_range_synchronize_timeout() -> Duration {
        Duration::from_secs(30)
    }
    fn default_certificates_synchronize_timeout() -> Duration {
        Duration::from_secs(30)
    }
    fn default_payload_synchronize_timeout() -> Duration {
        Duration::from_secs(30)
    }
    fn default_payload_availability_timeout() -> Duration {
        Duration::from_secs(30)
    }
    fn default_handler_certificate_deliver_timeout() -> Duration {
        Duration::from_secs(30)
    }
}

impl Default for BlockSynchronizerParameters {
    fn default() -> Self {
        Self {
            range_synchronize_timeout:
                BlockSynchronizerParameters::default_range_synchronize_timeout(),
            certificates_synchronize_timeout:
                BlockSynchronizerParameters::default_certificates_synchronize_timeout(),
            payload_synchronize_timeout:
                BlockSynchronizerParameters::default_payload_synchronize_timeout(),
            payload_availability_timeout:
                BlockSynchronizerParameters::default_payload_availability_timeout(),
            handler_certificate_deliver_timeout:
                BlockSynchronizerParameters::default_handler_certificate_deliver_timeout(),
        }
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            header_num_of_batches_threshold: 32,
            max_header_num_of_batches: 1000,
            max_header_delay: Duration::from_millis(100),
            min_header_delay: Duration::from_millis(100),
            gc_depth: 50,
            sync_retry_delay: Duration::from_millis(5_000),
            sync_retry_nodes: 3,
            batch_size: 500_000,
            max_batch_delay: Duration::from_millis(100),
            block_synchronizer: BlockSynchronizerParameters::default(),
            consensus_api_grpc: ConsensusAPIGrpcParameters::default(),
            max_concurrent_requests: 500_000,
            prometheus_metrics: PrometheusMetricsParameters::default(),
            network_admin_server: NetworkAdminServerParameters::default(),
            anemo: AnemoParameters::default(),
        }
    }
}

impl Parameters {
    pub fn with_available_ports(&self) -> Self {
        let mut params = self.clone();
        params.consensus_api_grpc = params.consensus_api_grpc.with_available_port();
        params.prometheus_metrics = params.prometheus_metrics.with_available_port();
        params.network_admin_server = params.network_admin_server.with_available_port();
        params
    }

    pub fn tracing(&self) {
        info!(
            "Header number of batches threshold set to {}",
            self.header_num_of_batches_threshold
        );
        info!(
            "Header max number of batches set to {}",
            self.max_header_num_of_batches
        );
        info!(
            "Max header delay set to {} ms",
            self.max_header_delay.as_millis()
        );
        info!(
            "Min header delay set to {} ms",
            self.min_header_delay.as_millis()
        );
        info!("Garbage collection depth set to {} rounds", self.gc_depth);
        info!(
            "Sync retry delay set to {} ms",
            self.sync_retry_delay.as_millis()
        );
        info!("Sync retry nodes set to {} nodes", self.sync_retry_nodes);
        info!("Batch size set to {} B", self.batch_size);
        info!(
            "Max batch delay set to {} ms",
            self.max_batch_delay.as_millis()
        );
        info!(
            "Synchronize range timeout set to {} s",
            self.block_synchronizer.range_synchronize_timeout.as_secs()
        );
        info!(
            "Synchronize certificates timeout set to {} s",
            self.block_synchronizer
                .certificates_synchronize_timeout
                .as_secs()
        );
        info!(
            "Payload (batches) availability timeout set to {} s",
            self.block_synchronizer
                .payload_availability_timeout
                .as_secs()
        );
        info!(
            "Synchronize payload (batches) timeout set to {} s",
            self.block_synchronizer
                .payload_synchronize_timeout
                .as_secs()
        );
        info!(
            "Consensus API gRPC Server set to listen on on {}",
            self.consensus_api_grpc.socket_addr
        );
        info!(
            "Get collections timeout set to {} ms",
            self.consensus_api_grpc.get_collections_timeout.as_millis()
        );
        info!(
            "Remove collections timeout set to {} ms",
            self.consensus_api_grpc
                .remove_collections_timeout
                .as_millis()
        );
        info!(
            "Handler certificate deliver timeout set to {} s",
            self.block_synchronizer
                .handler_certificate_deliver_timeout
                .as_secs()
        );
        info!(
            "Max concurrent requests set to {}",
            self.max_concurrent_requests
        );
        info!(
            "Prometheus metrics server will run on {}",
            self.prometheus_metrics.socket_addr
        );
        info!(
            "Primary network admin server will run on 127.0.0.1:{}",
            self.network_admin_server.primary_network_admin_server_port
        );
        info!(
            "Worker network admin server will run starting on base port 127.0.0.1:{}",
            self.network_admin_server
                .worker_network_admin_server_base_port
        );
    }
}

#[derive(Clone, Serialize, Deserialize, Eq, Hash, PartialEq, Debug)]
pub struct WorkerInfo {
    /// The public key of this worker.
    pub name: NetworkPublicKey,
    /// Address to receive client transactions (WAN).
    pub transactions: Multiaddr,
    /// Address to receive messages from other workers (WAN) and our primary.
    pub worker_address: Multiaddr,
}

pub type SharedWorkerCache = Arc<ArcSwap<WorkerCache>>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkerIndex(pub BTreeMap<WorkerId, WorkerInfo>);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkerCache {
    /// The authority to worker index.
    pub workers: BTreeMap<PublicKey, WorkerIndex>,
    /// The epoch number for workers
    pub epoch: Epoch,
}

impl From<WorkerCache> for SharedWorkerCache {
    fn from(worker_cache: WorkerCache) -> Self {
        Arc::new(ArcSwap::from_pointee(worker_cache))
    }
}

impl std::fmt::Display for WorkerIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WorkerIndex {:?}",
            self.0
                .iter()
                .map(|(key, value)| { format!("{}:{:?}", key, value) })
                .collect::<Vec<_>>()
        )
    }
}

impl std::fmt::Display for WorkerCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WorkerCache E{}: {:?}",
            self.epoch(),
            self.workers
                .iter()
                .map(|(k, v)| {
                    if let Some(x) = k.encode_base64().get(0..16) {
                        format!("{}: {}", x, v)
                    } else {
                        format!("Invalid key: {}", k)
                    }
                })
                .collect::<Vec<_>>()
        )
    }
}

impl WorkerCache {
    /// Returns the current epoch.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Returns the addresses of a specific worker (`id`) of a specific authority (`to`).
    pub fn worker(&self, to: &PublicKey, id: &WorkerId) -> Result<WorkerInfo, ConfigError> {
        self.workers
            .iter()
            .find_map(|v| match_opt::match_opt!(v, (name, authority) if name == to => authority))
            .ok_or_else(|| {
                ConfigError::NotInWorkerCache(ToString::to_string(&(*to).encode_base64()))
            })?
            .0
            .iter()
            .find(|(worker_id, _)| worker_id == &id)
            .map(|(_, worker)| worker.clone())
            .ok_or_else(|| ConfigError::NotInWorkerCache((*to).encode_base64()))
    }

    /// Returns the addresses of all our workers.
    pub fn our_workers(&self, myself: &PublicKey) -> Result<Vec<WorkerInfo>, ConfigError> {
        let res = self
            .workers
            .iter()
            .find_map(
                |v| match_opt::match_opt!(v, (name, authority) if name == myself => authority),
            )
            .ok_or_else(|| ConfigError::NotInWorkerCache((*myself).encode_base64()))?
            .0
            .values()
            .cloned()
            .collect();
        Ok(res)
    }

    /// Returns the addresses of all known workers.
    pub fn all_workers(&self) -> Vec<(NetworkPublicKey, Multiaddr)> {
        self.workers
            .iter()
            .flat_map(|(_, w)| {
                w.0.values()
                    .map(|w| (w.name.clone(), w.worker_address.clone()))
            })
            .collect()
    }

    /// Returns the addresses of all workers with a specific id except the ones of the authority
    /// specified by `myself`.
    pub fn others_workers_by_id(
        &self,
        myself: &PublicKey,
        id: &WorkerId,
    ) -> Vec<(PublicKey, WorkerInfo)> {
        self.workers
            .iter()
            .filter(|(name, _)| *name != myself )
            .flat_map(
                |(name, authority)|  authority.0.iter().flat_map(
                    |v| match_opt::match_opt!(v,(worker_id, addresses) if worker_id == id => (name.clone(), addresses.clone()))))
            .collect()
    }

    /// Returns the addresses of all workers that are not of our node.
    pub fn others_workers(&self, myself: &PublicKey) -> Vec<(PublicKey, WorkerInfo)> {
        self.workers
            .iter()
            .filter(|(name, _)| *name != myself)
            .flat_map(|(name, authority)| authority.0.iter().map(|v| (name.clone(), v.1.clone())))
            .collect()
    }

    /// Return the network addresses that are present in the current worker cache
    /// that are from a primary key that are no longer in the committee. Current
    /// committee keys provided as an argument.
    pub fn network_diff(&self, keys: Vec<&PublicKey>) -> HashSet<&Multiaddr> {
        self.workers
            .iter()
            .filter(|(name, _)| !keys.contains(name))
            .flat_map(|(_, authority)| {
                authority
                    .0
                    .values()
                    .map(|address| &address.transactions)
                    .chain(authority.0.values().map(|address| &address.worker_address))
            })
            .collect()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct Authority {
    /// The voting power of this authority.
    pub stake: Stake,
    /// The network address of the primary.
    pub primary_address: Multiaddr,
    /// Network key of the primary.
    pub network_key: NetworkPublicKey,
}

pub type SharedCommittee = Arc<ArcSwap<Committee>>;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct Committee {
    /// The authorities of epoch.
    pub authorities: BTreeMap<PublicKey, Authority>,
    /// The epoch number of this committee
    pub epoch: Epoch,
}

impl From<Committee> for SharedCommittee {
    fn from(committee: Committee) -> Self {
        Arc::new(ArcSwap::from_pointee(committee))
    }
}

impl std::fmt::Display for Committee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Committee E{}: {:?}",
            self.epoch(),
            self.authorities
                .keys()
                .map(|x| {
                    if let Some(k) = x.encode_base64().get(0..16) {
                        k.to_owned()
                    } else {
                        format!("Invalid key: {}", x)
                    }
                })
                .collect::<Vec<_>>()
        )
    }
}

impl Committee {
    /// Returns the current epoch.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Returns the keys in the committee
    pub fn keys(&self) -> Vec<&PublicKey> {
        self.authorities.keys().clone().collect::<Vec<&PublicKey>>()
    }

    pub fn authorities(&self) -> impl Iterator<Item = (&PublicKey, &Authority)> {
        self.authorities.iter()
    }

    pub fn authority_by_network_key(
        &self,
        network_key: &NetworkPublicKey,
    ) -> Option<(&PublicKey, &Authority)> {
        self.authorities
            .iter()
            .find(|(_, authority)| authority.network_key == *network_key)
    }

    /// Returns the number of authorities.
    pub fn size(&self) -> usize {
        self.authorities.len()
    }

    /// Return the stake of a specific authority.
    pub fn stake(&self, name: &PublicKey) -> Stake {
        self.authorities
            .get(&name.clone())
            .map_or_else(|| 0, |x| x.stake)
    }

    /// Returns the stake required to reach a quorum (2f+1).
    pub fn quorum_threshold(&self) -> Stake {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (2 N + 3) / 3 = 2f + 1 + (2k + 2)/3 = 2f + 1 + k = N - f
        let total_votes: Stake = self.authorities.values().map(|x| x.stake).sum();
        2 * total_votes / 3 + 1
    }

    /// Returns the stake required to reach availability (f+1).
    pub fn validity_threshold(&self) -> Stake {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (N + 2) / 3 = f + 1 + k/3 = f + 1
        let total_votes: Stake = self.authorities.values().map(|x| x.stake).sum();
        (total_votes + 2) / 3
    }

    /// Returns a leader node as a weighted choice seeded by the provided integer
    pub fn leader(&self, seed: u64) -> PublicKey {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[32 - 8..].copy_from_slice(&seed.to_le_bytes());
        let mut rng = StdRng::from_seed(seed_bytes);
        let choices = self
            .authorities
            .iter()
            .map(|(name, authority)| (name, authority.stake as f32))
            .collect::<Vec<_>>();
        choices
            .choose_weighted(&mut rng, |item| item.1)
            .expect("Weighted choice error: stake values incorrect!")
            .0
            .clone()
    }

    /// Returns the primary address of the target primary.
    pub fn primary(&self, to: &PublicKey) -> Result<Multiaddr, ConfigError> {
        self.authorities
            .get(&to.clone())
            .map(|x| x.primary_address.clone())
            .ok_or_else(|| ConfigError::NotInCommittee((*to).encode_base64()))
    }

    pub fn network_key(&self, pk: &PublicKey) -> Result<NetworkPublicKey, ConfigError> {
        self.authorities
            .get(&pk.clone())
            .map(|x| x.network_key.clone())
            .ok_or_else(|| ConfigError::NotInCommittee((*pk).encode_base64()))
    }

    /// Return all the network addresses in the committee.
    pub fn others_primaries(
        &self,
        myself: &PublicKey,
    ) -> Vec<(PublicKey, Multiaddr, NetworkPublicKey)> {
        self.authorities
            .iter()
            .filter(|(name, _)| *name != myself)
            .map(|(name, authority)| {
                (
                    name.clone(),
                    authority.primary_address.clone(),
                    authority.network_key.clone(),
                )
            })
            .collect()
    }

    fn get_all_network_addresses(&self) -> HashSet<&Multiaddr> {
        self.authorities
            .values()
            .map(|authority| &authority.primary_address)
            .collect()
    }

    /// Return the network addresses that are present in the current committee but that are absent
    /// from the new committee (provided as argument).
    pub fn network_diff<'a>(&'a self, other: &'a Self) -> HashSet<&Multiaddr> {
        self.get_all_network_addresses()
            .difference(&other.get_all_network_addresses())
            .cloned()
            .collect()
    }

    /// Update the networking information of some of the primaries. The arguments are a full vector of
    /// authorities which Public key and Stake must match the one stored in the current Committee. Any discrepancy
    /// will generate no update and return a vector of errors.
    pub fn update_primary_network_info(
        &mut self,
        mut new_info: BTreeMap<PublicKey, (Stake, Multiaddr)>,
    ) -> Result<(), Vec<CommitteeUpdateError>> {
        let mut errors = None;

        let table = &self.authorities;
        let push_error_and_return = |acc, error| {
            let mut error_table = if let Err(errors) = acc {
                errors
            } else {
                Vec::new()
            };
            error_table.push(error);
            Err(error_table)
        };

        let res = table
            .iter()
            .fold(Ok(BTreeMap::new()), |acc, (pk, authority)| {
                if let Some((stake, address)) = new_info.remove(pk) {
                    if stake == authority.stake {
                        match acc {
                            // No error met yet, update the accumulator
                            Ok(mut bmap) => {
                                let mut res = authority.clone();
                                res.primary_address = address;
                                bmap.insert(pk.clone(), res);
                                Ok(bmap)
                            }
                            // in error mode, continue
                            _ => acc,
                        }
                    } else {
                        // Stake does not match: create or append error
                        push_error_and_return(
                            acc,
                            CommitteeUpdateError::DifferentStake(pk.to_string()),
                        )
                    }
                } else {
                    // This key is absent from new information
                    push_error_and_return(
                        acc,
                        CommitteeUpdateError::MissingFromUpdate(pk.to_string()),
                    )
                }
            });

        // If there are elements left in new_info, they are not in the original table
        // If new_info is empty, this is a no-op.
        let res = new_info.iter().fold(res, |acc, (pk, _)| {
            push_error_and_return(acc, CommitteeUpdateError::NotInCommittee(pk.to_string()))
        });

        match res {
            Ok(new_table) => self.authorities = new_table,
            Err(errs) => {
                errors = Some(errs);
            }
        };

        errors.map(Err).unwrap_or(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use crate::Parameters;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn tracing_should_print_parameters() {
        // GIVEN
        let parameters = Parameters::default();

        // WHEN
        parameters.tracing();

        // THEN
        assert!(logs_contain("Header number of batches threshold set to 32"));
        assert!(logs_contain("Header max number of batches set to 1000"));
        assert!(logs_contain("Max header delay set to 100 ms"));
        assert!(logs_contain("Garbage collection depth set to 50 rounds"));
        assert!(logs_contain("Sync retry delay set to 5000 ms"));
        assert!(logs_contain("Sync retry nodes set to 3 nodes"));
        assert!(logs_contain("Batch size set to 500000 B"));
        assert!(logs_contain("Max batch delay set to 100 ms"));
        assert!(logs_contain("Synchronize certificates timeout set to 30 s"));
        assert!(logs_contain(
            "Payload (batches) availability timeout set to 30 s"
        ));
        assert!(logs_contain(
            "Synchronize payload (batches) timeout set to 30 s"
        ));
        assert!(logs_contain(
            "Handler certificate deliver timeout set to 30 s"
        ));
        assert!(logs_contain(
            "Consensus API gRPC Server set to listen on on /ip4/127.0.0.1/tcp"
        ));
        assert!(logs_contain("Get collections timeout set to 5000 ms"));
        assert!(logs_contain("Remove collections timeout set to 5000 ms"));
        assert!(logs_contain("Max concurrent requests set to 500000"));
        assert!(logs_contain(
            "Prometheus metrics server will run on /ip4/127.0.0.1/tcp"
        ));
        assert!(logs_contain(
            "Primary network admin server will run on 127.0.0.1:"
        ));
        assert!(logs_contain(
            "Worker network admin server will run starting on base port 127.0.0.1:"
        ));
    }
}
