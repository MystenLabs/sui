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

use crypto::{NetworkPublicKey, PublicKey};
use fastcrypto::traits::EncodeDecodeBase64;
use mysten_network::Multiaddr;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufWriter, Write as _},
    num::NonZeroU32,
    time::Duration,
};
use thiserror::Error;
use tracing::info;
use utils::get_available_port;

pub mod committee;
pub use committee::*;
mod duration_format;
pub mod utils;

/// The epoch number.
pub type Epoch = u64;

// Opaque bytes uniquely identifying the current chain. Analogue of the
// type in `sui-types` crate.
#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ChainIdentifier([u8; 32]);

impl ChainIdentifier {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn unknown() -> Self {
        Self([0; 32])
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

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

// TODO: This actually represents voting power (out of 10,000) and not amount staked.
// Consider renaming to `VotingPower`.
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
    #[serde(default = "Parameters::default_gc_depth")]
    pub gc_depth: u64,
    /// The delay after which the synchronizer retries to send sync requests. Denominated in ms.
    #[serde(
        with = "duration_format",
        default = "Parameters::default_sync_retry_delay"
    )]
    pub sync_retry_delay: Duration,
    /// Determine with how many nodes to sync when re-trying to send sync-request. These nodes
    /// are picked at random from the committee.
    #[serde(default = "Parameters::default_sync_retry_nodes")]
    pub sync_retry_nodes: usize,
    /// The preferred batch size. The workers seal a batch of transactions when it reaches this size.
    /// Denominated in bytes.
    #[serde(default = "Parameters::default_batch_size")]
    pub batch_size: usize,
    /// The delay after which the workers seal a batch of transactions, even if `max_batch_size`
    /// is not reached.
    #[serde(
        with = "duration_format",
        default = "Parameters::default_max_batch_delay"
    )]
    pub max_batch_delay: Duration,
    /// The maximum number of concurrent requests for messages accepted from an un-trusted entity
    #[serde(default = "Parameters::default_max_concurrent_requests")]
    pub max_concurrent_requests: usize,
    /// Properties for the prometheus metrics
    #[serde(default = "PrometheusMetricsParameters::default")]
    pub prometheus_metrics: PrometheusMetricsParameters,
    /// Network admin server ports for primary & worker.
    #[serde(default = "NetworkAdminServerParameters::default")]
    pub network_admin_server: NetworkAdminServerParameters,
    /// Anemo network settings.
    #[serde(default = "AnemoParameters::default")]
    pub anemo: AnemoParameters,
}

impl Parameters {
    pub const DEFAULT_FILENAME: &'static str = "parameters.json";

    fn default_header_num_of_batches_threshold() -> usize {
        32
    }

    fn default_max_header_num_of_batches() -> usize {
        1_000
    }

    fn default_max_header_delay() -> Duration {
        Duration::from_secs(1)
    }

    fn default_min_header_delay() -> Duration {
        Duration::from_secs_f64(0.5)
    }

    fn default_gc_depth() -> u64 {
        50
    }

    fn default_sync_retry_delay() -> Duration {
        Duration::from_millis(5_000)
    }

    fn default_sync_retry_nodes() -> usize {
        3
    }

    fn default_batch_size() -> usize {
        5_000_000
    }

    fn default_max_batch_delay() -> Duration {
        Duration::from_millis(100)
    }

    fn default_max_concurrent_requests() -> usize {
        500_000
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
    pub send_certificate_rate_limit: Option<NonZeroU32>,

    /// Per-peer rate-limits (in requests/sec) for the WorkerToWorker service.
    pub report_batch_rate_limit: Option<NonZeroU32>,
    pub request_batches_rate_limit: Option<NonZeroU32>,

    /// Size in bytes above which network messages are considered excessively large. Excessively
    /// large messages will still be handled, but logged and reported in metrics for debugging.
    ///
    /// If unspecified, this will default to 8 MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excessive_message_size: Option<usize>,
}

impl AnemoParameters {
    // By default, at most 10 certificates can be sent concurrently to a peer.
    pub fn send_certificate_rate_limit(&self) -> u32 {
        self.send_certificate_rate_limit
            .unwrap_or(NonZeroU32::new(20).unwrap())
            .get()
    }

    // By default, at most 100 batches can be broadcasted concurrently.
    pub fn report_batch_rate_limit(&self) -> u32 {
        self.report_batch_rate_limit
            .unwrap_or(NonZeroU32::new(200).unwrap())
            .get()
    }

    // As of 11/02/2023, when one worker is actively fetching, each peer receives
    // 20~30 requests per second.
    pub fn request_batches_rate_limit(&self) -> u32 {
        self.request_batches_rate_limit
            .unwrap_or(NonZeroU32::new(100).unwrap())
            .get()
    }

    pub fn excessive_message_size(&self) -> usize {
        const EXCESSIVE_MESSAGE_SIZE: usize = 8 << 20;

        self.excessive_message_size
            .unwrap_or(EXCESSIVE_MESSAGE_SIZE)
    }
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
    pub const DEFAULT_PORT: usize = 9184;

    fn with_available_port(&self) -> Self {
        let mut params = self.clone();
        let default = Self::default();
        params.socket_addr = default.socket_addr;
        params
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            header_num_of_batches_threshold: Parameters::default_header_num_of_batches_threshold(),
            max_header_num_of_batches: Parameters::default_max_header_num_of_batches(),
            max_header_delay: Parameters::default_max_header_delay(),
            min_header_delay: Parameters::default_min_header_delay(),
            gc_depth: Parameters::default_gc_depth(),
            sync_retry_delay: Parameters::default_sync_retry_delay(),
            sync_retry_nodes: Parameters::default_sync_retry_nodes(),
            batch_size: Parameters::default_batch_size(),
            max_batch_delay: Parameters::default_max_batch_delay(),
            max_concurrent_requests: Parameters::default_max_concurrent_requests(),
            prometheus_metrics: PrometheusMetricsParameters::default(),
            network_admin_server: NetworkAdminServerParameters::default(),
            anemo: AnemoParameters::default(),
        }
    }
}

impl Parameters {
    pub fn with_available_ports(&self) -> Self {
        let mut params = self.clone();
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkerIndex(pub BTreeMap<WorkerId, WorkerInfo>);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkerCache {
    /// The authority to worker index.
    pub workers: BTreeMap<PublicKey, WorkerIndex>,
    /// The epoch number for workers
    pub epoch: Epoch,
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
    pub const DEFAULT_FILENAME: &'static str = "workers.json";

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
