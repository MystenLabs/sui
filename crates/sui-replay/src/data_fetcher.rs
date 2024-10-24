// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::ReplayEngineError;
use crate::types::EPOCH_CHANGE_STRUCT_TAG;
use async_trait::async_trait;
use futures::future::join_all;
use lru::LruCache;
use move_core_types::language_storage::StructTag;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use sui_core::authority::NodeStateDump;
use sui_json_rpc_api::QUERY_MAX_RESULT_LIMIT;
use sui_json_rpc_types::EventFilter;
use sui_json_rpc_types::SuiEvent;
use sui_json_rpc_types::SuiGetPastObjectRequest;
use sui_json_rpc_types::SuiObjectData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_json_rpc_types::SuiObjectResponse;
use sui_json_rpc_types::SuiPastObjectResponse;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use sui_types::digests::TransactionDigest;
use sui_types::object::Object;
use sui_types::transaction::SenderSignedData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::{EndOfEpochTransactionKind, TransactionKind};

/// This trait defines the interfaces for fetching data from some local or remote store
#[async_trait]
pub(crate) trait DataFetcher {
    #![allow(implied_bounds_entailment)]
    /// Fetch the specified versions of objects
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, ReplayEngineError>;

    /// Fetch the latest versions of objects
    async fn multi_get_latest(
        &self,
        objects: &[ObjectID],
    ) -> Result<Vec<Object>, ReplayEngineError>;

    /// Fetch the TXs for this checkpoint
    async fn get_checkpoint_txs(
        &self,
        id: u64,
    ) -> Result<Vec<TransactionDigest>, ReplayEngineError>;

    /// Fetch the transaction info for a given transaction digest
    async fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, ReplayEngineError>;

    async fn get_loaded_child_objects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, ReplayEngineError>;

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, ReplayEngineError>;

    async fn fetch_random_transaction(
        &self,
        // TODO: add more params
        checkpoint_id_start: Option<u64>,
        checkpoint_id_end: Option<u64>,
    ) -> Result<TransactionDigest, ReplayEngineError>;

    async fn get_epoch_start_timestamp_and_rgp(
        &self,
        epoch_id: u64,
    ) -> Result<(u64, u64), ReplayEngineError>;

    async fn get_epoch_change_events(
        &self,
        reverse: bool,
    ) -> Result<Vec<SuiEvent>, ReplayEngineError>;

    async fn get_chain_id(&self) -> Result<String, ReplayEngineError>;

    async fn get_child_object(
        &self,
        object_id: &ObjectID,
        version_upper_bound: VersionNumber,
    ) -> Result<Object, ReplayEngineError>;
}

#[derive(Clone)]
pub enum Fetchers {
    Remote(RemoteFetcher),
    NodeStateDump(NodeStateDumpFetcher),
}

impl Fetchers {
    pub fn as_remote(&self) -> &RemoteFetcher {
        match self {
            Fetchers::Remote(q) => q,
            Fetchers::NodeStateDump(_) => panic!("not a remote fetcher"),
        }
    }

    pub fn into_remote(self) -> RemoteFetcher {
        match self {
            Fetchers::Remote(q) => {
                // Since `into_remote` is called when we use this fetcher to create a new fetcher,
                // we should clear the cache to avoid using stale data.
                q.clear_cache_for_new_task();
                q
            }
            Fetchers::NodeStateDump(_) => panic!("not a remote fetcher"),
        }
    }

    pub fn as_node_state_dump(&self) -> &NodeStateDumpFetcher {
        match self {
            Fetchers::Remote(_) => panic!("not a node state dump fetcher"),
            Fetchers::NodeStateDump(q) => q,
        }
    }
}

#[async_trait]
impl DataFetcher for Fetchers {
    #![allow(implied_bounds_entailment)]
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.multi_get_versioned(objects).await,
            Fetchers::NodeStateDump(q) => q.multi_get_versioned(objects).await,
        }
    }

    async fn multi_get_latest(
        &self,
        objects: &[ObjectID],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.multi_get_latest(objects).await,
            Fetchers::NodeStateDump(q) => q.multi_get_latest(objects).await,
        }
    }

    async fn get_checkpoint_txs(
        &self,
        id: u64,
    ) -> Result<Vec<TransactionDigest>, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_checkpoint_txs(id).await,
            Fetchers::NodeStateDump(q) => q.get_checkpoint_txs(id).await,
        }
    }

    async fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_transaction(tx_digest).await,
            Fetchers::NodeStateDump(q) => q.get_transaction(tx_digest).await,
        }
    }

    async fn get_loaded_child_objects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_loaded_child_objects(tx_digest).await,
            Fetchers::NodeStateDump(q) => q.get_loaded_child_objects(tx_digest).await,
        }
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_latest_checkpoint_sequence_number().await,
            Fetchers::NodeStateDump(q) => q.get_latest_checkpoint_sequence_number().await,
        }
    }

    async fn fetch_random_transaction(
        &self,
        checkpoint_id_start: Option<u64>,
        checkpoint_id_end: Option<u64>,
    ) -> Result<TransactionDigest, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => {
                q.fetch_random_transaction(checkpoint_id_start, checkpoint_id_end)
                    .await
            }
            Fetchers::NodeStateDump(q) => {
                q.fetch_random_transaction(checkpoint_id_start, checkpoint_id_end)
                    .await
            }
        }
    }

    async fn get_epoch_start_timestamp_and_rgp(
        &self,
        epoch_id: u64,
    ) -> Result<(u64, u64), ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_epoch_start_timestamp_and_rgp(epoch_id).await,
            Fetchers::NodeStateDump(q) => q.get_epoch_start_timestamp_and_rgp(epoch_id).await,
        }
    }

    async fn get_epoch_change_events(
        &self,
        reverse: bool,
    ) -> Result<Vec<SuiEvent>, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_epoch_change_events(reverse).await,
            Fetchers::NodeStateDump(q) => q.get_epoch_change_events(reverse).await,
        }
    }
    async fn get_chain_id(&self) -> Result<String, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_chain_id().await,
            Fetchers::NodeStateDump(q) => q.get_chain_id().await,
        }
    }
    async fn get_child_object(
        &self,
        object_id: &ObjectID,
        version_upper_bound: VersionNumber,
    ) -> Result<Object, ReplayEngineError> {
        match self {
            Fetchers::Remote(q) => q.get_child_object(object_id, version_upper_bound).await,
            Fetchers::NodeStateDump(q) => q.get_child_object(object_id, version_upper_bound).await,
        }
    }
}

const VERSIONED_OBJECT_CACHE_CAPACITY: Option<NonZeroUsize> = NonZeroUsize::new(1_000);
const LATEST_OBJECT_CACHE_CAPACITY: Option<NonZeroUsize> = NonZeroUsize::new(1_000);
const EPOCH_INFO_CACHE_CAPACITY: Option<NonZeroUsize> = NonZeroUsize::new(10_000);

pub struct RemoteFetcher {
    /// This is used to download items not in store
    pub rpc_client: SuiClient,
    /// Cache versioned objects
    pub versioned_object_cache: RwLock<LruCache<(ObjectID, VersionNumber), Object>>,
    /// Cache non-versioned objects
    pub latest_object_cache: RwLock<LruCache<ObjectID, Object>>,
    /// Cache epoch info
    pub epoch_info_cache: RwLock<LruCache<u64, (u64, u64)>>,
}

impl Clone for RemoteFetcher {
    fn clone(&self) -> Self {
        let mut latest =
            LruCache::new(LATEST_OBJECT_CACHE_CAPACITY.expect("Cache size must be non zero"));
        self.latest_object_cache.read().iter().for_each(|(k, v)| {
            latest.put(*k, v.clone());
        });

        let mut versioned =
            LruCache::new(VERSIONED_OBJECT_CACHE_CAPACITY.expect("Cache size must be non zero"));
        self.versioned_object_cache
            .read()
            .iter()
            .for_each(|(k, v)| {
                versioned.put(*k, v.clone());
            });

        let mut ep = LruCache::new(EPOCH_INFO_CACHE_CAPACITY.expect("Cache size must be non zero"));
        self.epoch_info_cache.read().iter().for_each(|(k, v)| {
            ep.put(*k, *v);
        });

        Self {
            rpc_client: self.rpc_client.clone(),
            versioned_object_cache: RwLock::new(versioned),
            latest_object_cache: RwLock::new(latest),
            epoch_info_cache: RwLock::new(ep),
        }
    }
}

impl RemoteFetcher {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self {
            rpc_client,
            versioned_object_cache: RwLock::new(LruCache::new(
                VERSIONED_OBJECT_CACHE_CAPACITY.expect("Cache size must be non zero"),
            )),
            latest_object_cache: RwLock::new(LruCache::new(
                LATEST_OBJECT_CACHE_CAPACITY.expect("Cache size must be non zero"),
            )),
            epoch_info_cache: RwLock::new(LruCache::new(
                EPOCH_INFO_CACHE_CAPACITY.expect("Cache size must be non zero"),
            )),
        }
    }

    pub fn check_versioned_cache(
        &self,
        objects: &[(ObjectID, VersionNumber)],
    ) -> (Vec<Object>, Vec<(ObjectID, VersionNumber)>) {
        let mut to_fetch = Vec::new();
        let mut cached = Vec::new();
        for (object_id, version) in objects {
            if let Some(obj) = self
                .versioned_object_cache
                .read()
                .peek(&(*object_id, *version))
            {
                cached.push(obj.clone());
            } else {
                to_fetch.push((*object_id, *version));
            }
        }

        (cached, to_fetch)
    }

    pub fn check_latest_cache(&self, objects: &[ObjectID]) -> (Vec<Object>, Vec<ObjectID>) {
        let mut to_fetch = Vec::new();
        let mut cached = Vec::new();
        for object_id in objects {
            if let Some(obj) = self.latest_object_cache.read().peek(object_id) {
                cached.push(obj.clone());
            } else {
                to_fetch.push(*object_id);
            }
        }

        (cached, to_fetch)
    }

    pub fn clear_cache_for_new_task(&self) {
        // Only the latest object cache cannot be reused across tasks.
        // All other caches should be valid as long as the network doesn't change.
        self.latest_object_cache.write().clear();
    }
}

#[async_trait]
impl DataFetcher for RemoteFetcher {
    #![allow(implied_bounds_entailment)]
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, VersionNumber)],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        // First check which we have in cache
        let (cached, to_fetch) = self.check_versioned_cache(objects);

        let options = SuiObjectDataOptions::bcs_lossless();

        let objs: Vec<_> = to_fetch
            .iter()
            .map(|(object_id, version)| SuiGetPastObjectRequest {
                object_id: *object_id,
                version: *version,
            })
            .collect();

        let objectsx = objs.chunks(*QUERY_MAX_RESULT_LIMIT).map(|q| {
            self.rpc_client
                .read_api()
                .try_multi_get_parsed_past_object(q.to_vec(), options.clone())
        });

        join_all(objectsx)
            .await
            .into_iter()
            .collect::<Result<Vec<Vec<_>>, _>>()
            .map_err(ReplayEngineError::from)?
            .iter()
            .flatten()
            .map(|q| convert_past_obj_response(q.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map(|mut x| {
                // Add the cached objects to the result
                x.extend(cached);
                // Backfill the cache
                for obj in &x {
                    let r = obj.compute_object_reference();
                    self.versioned_object_cache
                        .write()
                        .put((r.0, r.1), obj.clone());
                }
                x
            })
    }

    async fn get_child_object(
        &self,
        object_id: &ObjectID,
        version_upper_bound: VersionNumber,
    ) -> Result<Object, ReplayEngineError> {
        let response = self
            .rpc_client
            .read_api()
            .try_get_object_before_version(*object_id, version_upper_bound)
            .await
            .map_err(|q| ReplayEngineError::SuiRpcError { err: q.to_string() })?;
        convert_past_obj_response(response)
    }

    async fn multi_get_latest(
        &self,
        objects: &[ObjectID],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        // First check which we have in cache
        let (cached, to_fetch) = self.check_latest_cache(objects);

        let options = SuiObjectDataOptions::bcs_lossless();

        let objectsx = to_fetch.chunks(*QUERY_MAX_RESULT_LIMIT).map(|q| {
            self.rpc_client
                .read_api()
                .multi_get_object_with_options(q.to_vec(), options.clone())
        });

        join_all(objectsx)
            .await
            .into_iter()
            .collect::<Result<Vec<Vec<_>>, _>>()
            .map_err(ReplayEngineError::from)?
            .iter()
            .flatten()
            .map(obj_from_sui_obj_response)
            .collect::<Result<Vec<_>, _>>()
            .map(|mut x| {
                // Add the cached objects to the result
                x.extend(cached);
                // Backfill the cache
                for obj in &x {
                    self.latest_object_cache.write().put(obj.id(), obj.clone());
                }
                x
            })
    }

    async fn get_checkpoint_txs(
        &self,
        id: u64,
    ) -> Result<Vec<TransactionDigest>, ReplayEngineError> {
        Ok(self
            .rpc_client
            .read_api()
            .get_checkpoint(id.into())
            .await
            .map_err(|q| ReplayEngineError::SuiRpcError { err: q.to_string() })?
            .transactions)
    }

    async fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, ReplayEngineError> {
        let tx_fetch_opts = SuiTransactionBlockResponseOptions::full_content();

        self.rpc_client
            .read_api()
            .get_transaction_with_options(*tx_digest, tx_fetch_opts)
            .await
            .map_err(ReplayEngineError::from)
    }

    async fn get_loaded_child_objects(
        &self,
        _: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, ReplayEngineError> {
        Ok(vec![])
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, ReplayEngineError> {
        self.rpc_client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await
            .map_err(ReplayEngineError::from)
    }

    async fn fetch_random_transaction(
        &self,
        // TODO: add more params
        checkpoint_id_start_inclusive: Option<u64>,
        checkpoint_id_end_inclusive: Option<u64>,
    ) -> Result<TransactionDigest, ReplayEngineError> {
        let checkpoint_id_end = checkpoint_id_end_inclusive
            .unwrap_or(self.get_latest_checkpoint_sequence_number().await?);
        let checkpoint_id_start = checkpoint_id_start_inclusive.unwrap_or(1);
        let checkpoint_id = rand::thread_rng().gen_range(checkpoint_id_start..=checkpoint_id_end);

        let txs = self.get_checkpoint_txs(checkpoint_id).await?;
        let tx_idx = rand::thread_rng().gen_range(0..txs.len());

        Ok(txs[tx_idx])
    }

    async fn get_epoch_start_timestamp_and_rgp(
        &self,
        epoch_id: u64,
    ) -> Result<(u64, u64), ReplayEngineError> {
        // Check epoch info cache
        if let Some((ts, rgp)) = self.epoch_info_cache.read().peek(&epoch_id) {
            return Ok((*ts, *rgp));
        }

        let event = self
            .get_epoch_change_events(true)
            .await?
            .into_iter()
            .find(|ev| match extract_epoch_and_version(ev.clone()) {
                Ok((epoch, _)) => epoch == epoch_id,
                Err(_) => false,
            })
            .ok_or(ReplayEngineError::EventNotFound { epoch: epoch_id })?;

        let reference_gas_price = if let serde_json::Value::Object(w) = event.parsed_json {
            u64::from_str(&w["reference_gas_price"].to_string().replace('\"', "")).unwrap()
        } else {
            return Err(ReplayEngineError::UnexpectedEventFormat {
                event: event.clone(),
            });
        };

        let epoch_change_tx = event.id.tx_digest;

        // Fetch full transaction content
        let tx_info = self.get_transaction(&epoch_change_tx).await?;

        let orig_tx: SenderSignedData = bcs::from_bytes(&tx_info.raw_transaction).unwrap();
        let tx_kind_orig = orig_tx.transaction_data().kind();

        match tx_kind_orig {
            TransactionKind::ChangeEpoch(change) => {
                // Backfill cache
                self.epoch_info_cache.write().put(
                    epoch_id,
                    (change.epoch_start_timestamp_ms, reference_gas_price),
                );

                return Ok((change.epoch_start_timestamp_ms, reference_gas_price));
            }
            TransactionKind::EndOfEpochTransaction(kinds) => {
                for kind in kinds {
                    if let EndOfEpochTransactionKind::ChangeEpoch(change) = kind {
                        // Backfill cache
                        self.epoch_info_cache.write().put(
                            epoch_id,
                            (change.epoch_start_timestamp_ms, reference_gas_price),
                        );

                        return Ok((change.epoch_start_timestamp_ms, reference_gas_price));
                    }
                }
            }
            _ => {}
        }
        Err(ReplayEngineError::InvalidEpochChangeTx { epoch: epoch_id })
    }

    async fn get_epoch_change_events(
        &self,
        reverse: bool,
    ) -> Result<Vec<SuiEvent>, ReplayEngineError> {
        let struct_tag_str = EPOCH_CHANGE_STRUCT_TAG.to_string();
        let struct_tag = StructTag::from_str(&struct_tag_str)?;

        let mut epoch_change_events: Vec<SuiEvent> = vec![];
        let mut has_next_page = true;
        let mut cursor = None;

        while has_next_page {
            let page_data = self
                .rpc_client
                .event_api()
                .query_events(
                    EventFilter::MoveEventType(struct_tag.clone()),
                    cursor,
                    None,
                    reverse,
                )
                .await
                .map_err(|e| ReplayEngineError::UnableToQuerySystemEvents {
                    rpc_err: e.to_string(),
                })?;
            epoch_change_events.extend(page_data.data);
            has_next_page = page_data.has_next_page;
            cursor = page_data.next_cursor;
        }

        Ok(epoch_change_events)
    }

    async fn get_chain_id(&self) -> Result<String, ReplayEngineError> {
        let chain_id = self
            .rpc_client
            .read_api()
            .get_chain_identifier()
            .await
            .map_err(|e| ReplayEngineError::UnableToGetChainId { err: e.to_string() })?;
        Ok(chain_id)
    }
}

fn convert_past_obj_response(resp: SuiPastObjectResponse) -> Result<Object, ReplayEngineError> {
    match resp {
        SuiPastObjectResponse::VersionFound(o) => obj_from_sui_obj_data(&o),
        SuiPastObjectResponse::ObjectDeleted(r) => Err(ReplayEngineError::ObjectDeleted {
            id: r.object_id,
            version: r.version,
            digest: r.digest,
        }),
        SuiPastObjectResponse::ObjectNotExists(id) => Err(ReplayEngineError::ObjectNotExist { id }),
        SuiPastObjectResponse::VersionNotFound(id, version) => {
            Err(ReplayEngineError::ObjectVersionNotFound { id, version })
        }
        SuiPastObjectResponse::VersionTooHigh {
            object_id,
            asked_version,
            latest_version,
        } => Err(ReplayEngineError::ObjectVersionTooHigh {
            id: object_id,
            asked_version,
            latest_version,
        }),
    }
}

fn obj_from_sui_obj_response(o: &SuiObjectResponse) -> Result<Object, ReplayEngineError> {
    let o = o.object().map_err(ReplayEngineError::from)?.clone();
    obj_from_sui_obj_data(&o)
}

fn obj_from_sui_obj_data(o: &SuiObjectData) -> Result<Object, ReplayEngineError> {
    match TryInto::<Object>::try_into(o.clone()) {
        Ok(obj) => Ok(obj),
        Err(e) => Err(e.into()),
    }
}

pub fn extract_epoch_and_version(ev: SuiEvent) -> Result<(u64, u64), ReplayEngineError> {
    if let serde_json::Value::Object(w) = ev.parsed_json {
        let epoch = u64::from_str(&w["epoch"].to_string().replace('\"', "")).unwrap();
        let version = u64::from_str(&w["protocol_version"].to_string().replace('\"', "")).unwrap();
        return Ok((epoch, version));
    }

    Err(ReplayEngineError::UnexpectedEventFormat { event: ev })
}

#[derive(Clone)]
pub struct NodeStateDumpFetcher {
    pub node_state_dump: NodeStateDump,
    pub object_ref_pool: BTreeMap<(ObjectID, SequenceNumber), Object>,
    pub latest_object_version_pool: BTreeMap<ObjectID, Object>,

    // Used when we need to fetch data from remote such as
    pub backup_remote_fetcher: Option<RemoteFetcher>,
}

impl From<NodeStateDump> for NodeStateDumpFetcher {
    fn from(node_state_dump: NodeStateDump) -> Self {
        let mut object_ref_pool = BTreeMap::new();
        let mut latest_object_version_pool: BTreeMap<ObjectID, Object> = BTreeMap::new();

        node_state_dump
            .all_objects()
            .iter()
            .for_each(|current_obj| {
                // Dense storage
                object_ref_pool.insert(
                    (current_obj.id, current_obj.version),
                    current_obj.object.clone(),
                );

                // Only most recent
                if let Some(last_seen_obj) = latest_object_version_pool.get(&current_obj.id) {
                    if current_obj.version <= last_seen_obj.version() {
                        return;
                    }
                };
                latest_object_version_pool.insert(current_obj.id, current_obj.object.clone());
            });
        Self {
            node_state_dump,
            object_ref_pool,
            latest_object_version_pool,
            backup_remote_fetcher: None,
        }
    }
}

impl NodeStateDumpFetcher {
    pub fn new(
        node_state_dump: NodeStateDump,
        backup_remote_fetcher: Option<RemoteFetcher>,
    ) -> Self {
        let mut s = Self::from(node_state_dump);
        s.backup_remote_fetcher = backup_remote_fetcher;
        s
    }
}

#[async_trait]
impl DataFetcher for NodeStateDumpFetcher {
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        let mut resp = vec![];
        match objects.iter().try_for_each(|(id, version)| {
            if let Some(obj) = self.object_ref_pool.get(&(*id, *version)) {
                resp.push(obj.clone());
                return Ok(());
            }
            Err(ReplayEngineError::ObjectVersionNotFound {
                id: *id,
                version: *version,
            })
        }) {
            Ok(_) => return Ok(resp),
            Err(e) => {
                if let Some(backup_remote_fetcher) = &self.backup_remote_fetcher {
                    return backup_remote_fetcher.multi_get_versioned(objects).await;
                }
                return Err(e);
            }
        };
    }

    async fn multi_get_latest(
        &self,
        objects: &[ObjectID],
    ) -> Result<Vec<Object>, ReplayEngineError> {
        let mut resp = vec![];
        match objects.iter().try_for_each(|id| {
            if let Some(obj) = self.latest_object_version_pool.get(id) {
                resp.push(obj.clone());
                return Ok(());
            }
            Err(ReplayEngineError::ObjectNotExist { id: *id })
        }) {
            Ok(_) => return Ok(resp),
            Err(e) => {
                if let Some(backup_remote_fetcher) = &self.backup_remote_fetcher {
                    return backup_remote_fetcher.multi_get_latest(objects).await;
                }
                return Err(e);
            }
        };
    }

    async fn get_checkpoint_txs(
        &self,
        _id: u64,
    ) -> Result<Vec<TransactionDigest>, ReplayEngineError> {
        unimplemented!("get_checkpoint_txs for state dump is not implemented")
    }

    async fn get_transaction(
        &self,
        _tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, ReplayEngineError> {
        unimplemented!("get_transaction for state dump is not implemented")
    }

    async fn get_loaded_child_objects(
        &self,
        _tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, ReplayEngineError> {
        Ok(self
            .node_state_dump
            .loaded_child_objects
            .iter()
            .map(|q| (q.id, q.version, q.digest))
            .map(|w| (w.0, w.1))
            .collect())
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, ReplayEngineError> {
        unimplemented!("get_latest_checkpoint_sequence_number for state dump is not implemented")
    }

    async fn fetch_random_transaction(
        &self,
        // TODO: add more params
        _checkpoint_id_start: Option<u64>,
        _checkpoint_id_end: Option<u64>,
    ) -> Result<TransactionDigest, ReplayEngineError> {
        unimplemented!("fetch_random_tx for state dump is not implemented")
    }

    async fn get_epoch_start_timestamp_and_rgp(
        &self,
        _epoch_id: u64,
    ) -> Result<(u64, u64), ReplayEngineError> {
        Ok((
            self.node_state_dump.epoch_start_timestamp_ms,
            self.node_state_dump.reference_gas_price,
        ))
    }

    async fn get_epoch_change_events(
        &self,
        _reverse: bool,
    ) -> Result<Vec<SuiEvent>, ReplayEngineError> {
        unimplemented!("get_epoch_change_events for state dump is not implemented")
    }

    async fn get_chain_id(&self) -> Result<String, ReplayEngineError> {
        unimplemented!("get_chain_id for state dump is not implemented")
    }

    async fn get_child_object(
        &self,
        _object_id: &ObjectID,
        _version_upper_bound: VersionNumber,
    ) -> Result<Object, ReplayEngineError> {
        unimplemented!("get child object is not implemented for state dump");
    }
}
