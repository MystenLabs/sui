// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An implementation of the replay interfaces: `TransactionStore`, `EpochStore` and `ObjectStore`
//! backed by the RPC GQL endpoint. Schema in `crates/sui-indexer-alt-graphql/schema.graphql`.
//! The RPC calls are implemented in `gql_queries.rs`.

use crate::summary_metrics::*;
use crate::{
    data_stores::gql_queries,
    replay_interface::{
        EpochData, EpochStore, ObjectKey, ObjectStore, SetupStore, StoreSummary, TransactionInfo,
        TransactionStore, VersionQuery,
    },
    Node,
};
use anyhow::Context;
use cynic::{GraphQlResponse, Operation};
use reqwest::header::USER_AGENT;
use std::time::Instant;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
};
use sui_types::{
    committee::ProtocolVersion,
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};
use tracing::{debug, debug_span};

type EpochId = u64;

/// Provide an implementation of the replay_interface traits backed by GQL RPC endpoint.
pub struct DataStore {
    client: reqwest::Client,
    rpc: reqwest::Url,
    node: Node,
    // Keep the epoch data considering its small size and footprint
    epoch_map: RwLock<BTreeMap<EpochId, EpochData>>,
    /// The binary's version passed to the User-Agent header in GQL query requests
    version: String,
    /// Metrics for hit/miss accounting
    metrics: DataStoreMetrics,
}

#[derive(Default)]
struct DataStoreMetrics {
    // transactions
    txn_hit: AtomicU64,
    txn_miss: AtomicU64,
    txn_error: AtomicU64,
    // epochs
    epoch_hit: AtomicU64,
    epoch_miss: AtomicU64,
    epoch_error: AtomicU64,
    // protocol config
    proto_hit: AtomicU64,
    proto_miss: AtomicU64,
    proto_error: AtomicU64,
    // objects by query kind
    obj_version_hit: AtomicU64,
    obj_version_miss: AtomicU64,
    obj_version_error: AtomicU64,
    obj_root_hit: AtomicU64,
    obj_root_miss: AtomicU64,
    obj_root_error: AtomicU64,
    obj_checkpoint_hit: AtomicU64,
    obj_checkpoint_miss: AtomicU64,
    obj_checkpoint_error: AtomicU64,
}

macro_rules! block_on {
    ($expr:expr) => {{
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            if tokio::runtime::Handle::try_current().is_ok() {
                // When already inside a Tokio runtime, spawn a scoped thread to
                // run a separate current-thread runtime without requiring
                // tokio::task::block_in_place (which may be unavailable).
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .expect("failed to build Tokio runtime");
                            rt.block_on($expr)
                        })
                        .join()
                        .expect("failed to join scoped thread running nested runtime")
                })
            } else {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build Tokio runtime");
                rt.block_on($expr)
            }
        }
    }};
}

impl TransactionStore for DataStore {
    fn transaction_data_and_effects(
        &self,
        digest: &str,
    ) -> Result<Option<TransactionInfo>, anyhow::Error> {
        match block_on!(self.transaction(digest)) {
            Ok(Some((data, effects, checkpoint))) => {
                self.metrics.txn_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(TransactionInfo {
                    data,
                    effects,
                    checkpoint,
                }))
            }
            Ok(None) => {
                self.metrics.txn_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.metrics.txn_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

impl EpochStore for DataStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, anyhow::Error> {
        if let Some(epoch_data) = self.epoch_map.read().unwrap().get(&epoch) {
            self.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(epoch_data.clone()));
        }
        match block_on!(self.epoch(epoch)) {
            Ok(Some(epoch_data)) => {
                self.epoch_map
                    .write()
                    .unwrap()
                    .insert(epoch, epoch_data.clone());
                self.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(epoch_data))
            }
            Ok(None) => {
                self.metrics.epoch_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.metrics.epoch_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    // Get the protocol config for an epoch directly from the binary
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, anyhow::Error> {
        match self.epoch_info(epoch) {
            Ok(Some(epoch)) => {
                self.metrics.proto_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(ProtocolConfig::get_for_version(
                    ProtocolVersion::new(epoch.protocol_version),
                    self.chain(),
                )))
            }
            Ok(None) => {
                self.metrics.proto_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.metrics.proto_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

impl ObjectStore for DataStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        match block_on!(self.objects(keys)) {
            Ok(results) => {
                assert_eq!(results.len(), keys.len());
                for (key, res) in keys.iter().zip(results.iter()) {
                    match key.version_query {
                        VersionQuery::Version(_) => {
                            if res.is_some() {
                                self.metrics.obj_version_hit.fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.metrics
                                    .obj_version_miss
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        VersionQuery::RootVersion(_) => {
                            if res.is_some() {
                                self.metrics.obj_root_hit.fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.metrics.obj_root_miss.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        VersionQuery::AtCheckpoint(_) => {
                            if res.is_some() {
                                self.metrics
                                    .obj_checkpoint_hit
                                    .fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.metrics
                                    .obj_checkpoint_miss
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
                Ok(results)
            }
            Err(e) => {
                // Attribute the error to each requested key type
                for key in keys.iter() {
                    match key.version_query {
                        VersionQuery::Version(_) => {
                            self.metrics
                                .obj_version_error
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        VersionQuery::RootVersion(_) => {
                            self.metrics.obj_root_error.fetch_add(1, Ordering::Relaxed);
                        }
                        VersionQuery::AtCheckpoint(_) => {
                            self.metrics
                                .obj_checkpoint_error
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Err(e)
            }
        }
    }
}

impl SetupStore for DataStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, anyhow::Error> {
        // Return the chain identifier
        Ok(Some(block_on!(gql_queries::chain_id_query::query(self))?))
    }
}

impl DataStore {
    pub fn new(node: Node, version: &str) -> Result<Self, anyhow::Error> {
        debug!("Start stores creation");
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        let url = node.rpc_url();
        let rpc =
            reqwest::Url::parse(url).context(format!("Failed to parse GQL RPC URL {}", url))?;
        let epoch_map = RwLock::new(BTreeMap::new());
        debug!("End stores creation");

        Ok(Self {
            client,
            node,
            epoch_map,
            rpc,
            version: version.to_string(),
            metrics: DataStoreMetrics::default(),
        })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }
    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    pub(crate) async fn run_query<T, V>(
        &self,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        Self::run_query_internal(&self.client, &self.rpc, &self.version, operation).await
    }

    async fn run_query_internal<T, V>(
        client: &reqwest::Client,
        rpc: &reqwest::Url,
        version: &str,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        client
            .post(rpc.clone())
            .header(USER_AGENT, format!("sui-replay-v{}", version))
            .json(&operation)
            .send()
            .await
            .context("Failed to send GQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("Failed to read response in GQL query")
    }

    async fn transaction(
        &self,
        digest: &str,
    ) -> Result<Option<(TransactionData, TransactionEffects, u64)>, anyhow::Error> {
        let _span = debug_span!("gql_txn_query", digest = %digest).entered();
        debug!(op = "txn_query", phase = "start", "transaction query");
        tx_counts_add_txn();
        let t0 = Instant::now();
        let data = gql_queries::txn_query::query(digest.to_string(), self).await;
        let elapsed = t0.elapsed().as_millis();
        tx_metrics_add_txn(elapsed);
        debug!(
            op = "txn_query",
            phase = "end",
            elapsed_ms = elapsed,
            "transaction query"
        );
        data
    }

    async fn epoch(&self, epoch_id: u64) -> Result<Option<EpochData>, anyhow::Error> {
        let _span = debug_span!("gql_epoch_query", epoch = epoch_id).entered();
        debug!(op = "epoch_query", phase = "start", "epoch query");
        tx_counts_add_epoch();
        let t0 = Instant::now();
        let data = gql_queries::epoch_query::query(epoch_id, self).await;
        let elapsed = t0.elapsed().as_millis();
        tx_metrics_add_epoch(elapsed);
        debug!(
            op = "epoch_query",
            phase = "end",
            elapsed_ms = elapsed,
            "epoch query"
        );
        data
    }

    async fn objects(
        &self,
        keys: &[ObjectKey],
    ) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        let _span = debug_span!("gql_objects_query", num_keys = keys.len()).entered();
        debug!(op = "objects_query", phase = "start", "objects query");
        // Track how many objects were requested in this batch
        tx_objs_add(keys.len());
        tx_counts_add_objs();
        let t0 = Instant::now();
        let data = gql_queries::object_query::query(keys, self).await?;
        let elapsed = t0.elapsed().as_millis();
        tx_metrics_add_objs(elapsed);
        debug!(
            op = "objects_query",
            phase = "end",
            elapsed_ms = elapsed,
            "objects query"
        );
        Ok(data)
    }
}

impl StoreSummary for DataStore {
    fn summary<W: std::io::Write>(&self, w: &mut W) -> anyhow::Result<()> {
        let m = &self.metrics;
        let txn_hit = m.txn_hit.load(Ordering::Relaxed);
        let txn_miss = m.txn_miss.load(Ordering::Relaxed);
        let txn_err = m.txn_error.load(Ordering::Relaxed);
        let epoch_hit = m.epoch_hit.load(Ordering::Relaxed);
        let epoch_miss = m.epoch_miss.load(Ordering::Relaxed);
        let epoch_err = m.epoch_error.load(Ordering::Relaxed);
        let proto_hit = m.proto_hit.load(Ordering::Relaxed);
        let proto_miss = m.proto_miss.load(Ordering::Relaxed);
        let proto_err = m.proto_error.load(Ordering::Relaxed);
        let obj_v_hit = m.obj_version_hit.load(Ordering::Relaxed);
        let obj_v_miss = m.obj_version_miss.load(Ordering::Relaxed);
        let obj_v_err = m.obj_version_error.load(Ordering::Relaxed);
        let obj_r_hit = m.obj_root_hit.load(Ordering::Relaxed);
        let obj_r_miss = m.obj_root_miss.load(Ordering::Relaxed);
        let obj_r_err = m.obj_root_error.load(Ordering::Relaxed);
        let obj_c_hit = m.obj_checkpoint_hit.load(Ordering::Relaxed);
        let obj_c_miss = m.obj_checkpoint_miss.load(Ordering::Relaxed);
        let obj_c_err = m.obj_checkpoint_error.load(Ordering::Relaxed);
        let obj_total_hit = obj_v_hit + obj_r_hit + obj_c_hit;
        let obj_total_miss = obj_v_miss + obj_r_miss + obj_c_miss;
        let obj_total_err = obj_v_err + obj_r_err + obj_c_err;
        let total_hit = txn_hit + epoch_hit + proto_hit + obj_total_hit;
        let total_miss = txn_miss + epoch_miss + proto_miss + obj_total_miss;
        let total_err = txn_err + epoch_err + proto_err + obj_total_err;

        writeln!(w, "DataStore (remote GQL) summary")?;
        writeln!(w, "  Node: {:?}", self.node())?;
        writeln!(w, "  Cache sizes:")?;
        writeln!(
            w,
            "    Epochs: {} entries",
            self.epoch_map.read().unwrap().len()
        )?;
        writeln!(
            w,
            "  Overall:    hit={} miss={} error={}",
            total_hit, total_miss, total_err
        )?;
        writeln!(w, "  Hits/Misses:")?;
        writeln!(
            w,
            "    Transaction: hit={} miss={} error={}",
            txn_hit, txn_miss, txn_err
        )?;
        writeln!(
            w,
            "    Epoch:       hit={} miss={} error={}",
            epoch_hit, epoch_miss, epoch_err
        )?;
        writeln!(
            w,
            "    Protocol:    hit={} miss={} error={}",
            proto_hit, proto_miss, proto_err
        )?;
        writeln!(
            w,
            "    Objects(all): hit={} miss={} error={}",
            obj_total_hit, obj_total_miss, obj_total_err
        )?;
        writeln!(
            w,
            "      Version:     hit={} miss={}",
            obj_v_hit, obj_v_miss
        )?;
        writeln!(
            w,
            "      RootVersion: hit={} miss={}",
            obj_r_hit, obj_r_miss
        )?;
        writeln!(
            w,
            "      Checkpoint:  hit={} miss={}",
            obj_c_hit, obj_c_miss
        )?;
        Ok(())
    }
}
