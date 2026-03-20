// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GraphQL-backed store with live object reads only.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use anyhow::{Context, Error, Result};
use cynic::{GraphQlResponse, Operation};
use reqwest::header::USER_AGENT;
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};
use tracing::{debug, debug_span};

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, SetupStore, StoreSummary, TransactionInfo,
    TransactionStore, TransactionStoreWriter, VersionQuery, gql_queries, node::Node,
};

/// GraphQL-backed implementation of the data store traits.
pub struct DataStore {
    client: reqwest::Client,
    rpc: reqwest::Url,
    node: Node,
    version: String,
    metrics: DataStoreMetrics,
}

#[derive(Default)]
struct DataStoreMetrics {
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

impl DataStore {
    pub fn new(node: Node, version: &str) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        let rpc = reqwest::Url::parse(node.gql_url())
            .context("failed to parse GraphQL RPC endpoint URL")?;

        Ok(Self {
            client,
            rpc,
            node,
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
    ) -> Result<GraphQlResponse<T>, Error>
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
    ) -> Result<GraphQlResponse<T>, Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        client
            .post(rpc.clone())
            .header(USER_AGENT, format!("sui-data-store-v{version}"))
            .json(operation)
            .send()
            .await
            .context("failed to send GraphQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("failed to decode GraphQL response")
    }

    async fn objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let _span = debug_span!("gql_objects_query", num_keys = keys.len()).entered();
        let start = Instant::now();
        let data = gql_queries::object_query::query(keys, self).await?;
        debug!(
            elapsed_ms = start.elapsed().as_millis(),
            num_keys = keys.len(),
            "GraphQL object query completed"
        );
        Ok(data)
    }
}

impl TransactionStore for DataStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("GraphQL transaction reads are not implemented in the PR2 slice")
    }
}

impl TransactionStoreWriter for DataStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("GraphQL transaction writes are not implemented in the PR2 slice")
    }
}

impl EpochStore for DataStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("GraphQL epoch reads are not implemented in the PR2 slice")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("GraphQL protocol-config reads are not implemented in the PR2 slice")
    }
}

impl EpochStoreWriter for DataStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("GraphQL epoch writes are not implemented in the PR2 slice")
    }
}

impl ObjectStore for DataStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        match block_on!(self.objects(keys)) {
            Ok(results) => {
                assert_eq!(results.len(), keys.len());
                for (key, result) in keys.iter().zip(results.iter()) {
                    match key.version_query {
                        VersionQuery::Version(_) => {
                            if result.is_some() {
                                self.metrics.obj_version_hit.fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.metrics
                                    .obj_version_miss
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        VersionQuery::RootVersion(_) => {
                            if result.is_some() {
                                self.metrics.obj_root_hit.fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.metrics.obj_root_miss.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        VersionQuery::AtCheckpoint(_) => {
                            if result.is_some() {
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
            Err(err) => {
                for key in keys {
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
                Err(err)
            }
        }
    }
}

impl CheckpointStore for DataStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        todo!("GraphQL checkpoint reads are not implemented in the PR2 slice")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
        todo!("GraphQL latest-checkpoint lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("GraphQL checkpoint-digest lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("GraphQL contents-digest lookups are not implemented in the PR2 slice")
    }
}

impl CheckpointStoreWriter for DataStore {
    fn write_checkpoint(&self, _checkpoint: &FullCheckpointData) -> Result<(), Error> {
        todo!("GraphQL checkpoint writes are not implemented in the PR2 slice")
    }
}

impl SetupStore for DataStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(Some(block_on!(gql_queries::chain_id_query::query(self))?))
    }
}

impl StoreSummary for DataStore {
    fn summary<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        let version_hit = self.metrics.obj_version_hit.load(Ordering::Relaxed);
        let version_miss = self.metrics.obj_version_miss.load(Ordering::Relaxed);
        let version_error = self.metrics.obj_version_error.load(Ordering::Relaxed);
        let root_hit = self.metrics.obj_root_hit.load(Ordering::Relaxed);
        let root_miss = self.metrics.obj_root_miss.load(Ordering::Relaxed);
        let root_error = self.metrics.obj_root_error.load(Ordering::Relaxed);
        let checkpoint_hit = self.metrics.obj_checkpoint_hit.load(Ordering::Relaxed);
        let checkpoint_miss = self.metrics.obj_checkpoint_miss.load(Ordering::Relaxed);
        let checkpoint_error = self.metrics.obj_checkpoint_error.load(Ordering::Relaxed);

        writeln!(writer, "DataStore (remote GraphQL object source)")?;
        writeln!(writer, "  Node: {:?}", self.node())?;
        writeln!(
            writer,
            "  Objects(version): hit={version_hit} miss={version_miss} error={version_error}"
        )?;
        writeln!(
            writer,
            "  Objects(root): hit={root_hit} miss={root_miss} error={root_error}"
        )?;
        writeln!(
            writer,
            "  Objects(checkpoint): hit={checkpoint_hit} miss={checkpoint_miss} error={checkpoint_error}"
        )?;
        Ok(())
    }
}
