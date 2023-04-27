// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! IndexStore supports creation of various ancillary indexes of state in SuiDataStore.
//! The main user of this data is the explorer.

use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::anyhow;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{ModuleId, StructTag, TypeTag};
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::sync::OwnedMutexGuard;

use crate::mutex_table::MutexTable;
use crate::sharded_lru::ShardedLruCache;
use sui_json_rpc_types::{SuiObjectDataFilter, TransactionFilter};
use sui_types::base_types::{
    ObjectDigest, ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::base_types::{ObjectInfo, ObjectRef};
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{self, DynamicFieldInfo};
use sui_types::effects::TransactionEvents;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::object::Owner;
use sui_types::parse_sui_struct_tag;
use sui_types::temporary_store::TxCoins;
use tokio::task::spawn_blocking;
use tracing::{debug, trace};
use typed_store::rocks::{
    default_db_options, read_size_from_env, DBBatch, DBMap, DBOptions, MetricConf, ReadWriteOptions,
};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store_derive::DBMapUtils;

type OwnerIndexKey = (SuiAddress, ObjectID);
type CoinIndexKey = (SuiAddress, String, ObjectID);
type DynamicFieldKey = (ObjectID, ObjectID);
type EventId = (TxSequenceNumber, usize);
type EventIndex = (TransactionEventsDigest, TransactionDigest, u64);
type AllBalance = HashMap<TypeTag, TotalBalance>;

pub const MAX_TX_RANGE_SIZE: u64 = 4096;

pub const MAX_GET_OWNED_OBJECT_SIZE: usize = 256;
const ENV_VAR_COIN_INDEX_BLOCK_CACHE_SIZE_MB: &str = "COIN_INDEX_BLOCK_CACHE_MB";
const ENV_VAR_DISABLE_INDEX_CACHE: &str = "DISABLE_INDEX_CACHE";
const ENV_VAR_INVALIDATE_INSTEAD_OF_UPDATE: &str = "INVALIDATE_INSTEAD_OF_UPDATE";

#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub struct TotalBalance {
    pub balance: i128,
    pub num_coins: i64,
}

#[derive(Debug)]
pub struct ObjectIndexChanges {
    pub deleted_owners: Vec<OwnerIndexKey>,
    pub deleted_dynamic_fields: Vec<DynamicFieldKey>,
    pub new_owners: Vec<(OwnerIndexKey, ObjectInfo)>,
    pub new_dynamic_fields: Vec<(DynamicFieldKey, DynamicFieldInfo)>,
}

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct CoinInfo {
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub balance: u64,
    pub previous_transaction: TransactionDigest,
}

pub struct IndexStoreMetrics {
    balance_lookup_from_db: IntCounter,
    balance_lookup_from_total: IntCounter,
    all_balance_lookup_from_db: IntCounter,
    all_balance_lookup_from_total: IntCounter,
}

impl IndexStoreMetrics {
    pub fn new(registry: &Registry) -> IndexStoreMetrics {
        Self {
            balance_lookup_from_db: register_int_counter_with_registry!(
                "balance_lookup_from_db",
                "Total number of balance requests served from cache",
                registry,
            )
            .unwrap(),
            balance_lookup_from_total: register_int_counter_with_registry!(
                "balance_lookup_from_total",
                "Total number of balance requests served ",
                registry,
            )
            .unwrap(),
            all_balance_lookup_from_db: register_int_counter_with_registry!(
                "all_balance_lookup_from_db",
                "Total number of all balance requests served from cache",
                registry,
            )
            .unwrap(),
            all_balance_lookup_from_total: register_int_counter_with_registry!(
                "all_balance_lookup_from_total",
                "Total number of all balance requests served",
                registry,
            )
            .unwrap(),
        }
    }
}

pub struct IndexStoreCaches {
    per_coin_type_balance: ShardedLruCache<(SuiAddress, TypeTag), SuiResult<TotalBalance>>,
    all_balances: ShardedLruCache<SuiAddress, SuiResult<Arc<HashMap<TypeTag, TotalBalance>>>>,
    locks: MutexTable<SuiAddress>,
}

#[derive(Default)]
pub struct IndexStoreCacheUpdates {
    _locks: Vec<OwnedMutexGuard<()>>,
    per_coin_type_balance_changes: Vec<((SuiAddress, TypeTag), SuiResult<TotalBalance>)>,
    all_balance_changes: Vec<(SuiAddress, SuiResult<Arc<AllBalance>>)>,
}

#[derive(DBMapUtils)]
pub struct IndexStoreTables {
    /// Index from sui address to transactions initiated by that address.
    #[default_options_override_fn = "transactions_from_addr_table_default_config"]
    transactions_from_addr: DBMap<(SuiAddress, TxSequenceNumber), TransactionDigest>,

    /// Index from sui address to transactions that were sent to that address.
    #[default_options_override_fn = "transactions_to_addr_table_default_config"]
    transactions_to_addr: DBMap<(SuiAddress, TxSequenceNumber), TransactionDigest>,

    /// Index from object id to transactions that used that object id as input.
    #[default_options_override_fn = "transactions_by_input_object_id_table_default_config"]
    transactions_by_input_object_id: DBMap<(ObjectID, TxSequenceNumber), TransactionDigest>,

    /// Index from object id to transactions that modified/created that object id.
    #[default_options_override_fn = "transactions_by_mutated_object_id_table_default_config"]
    transactions_by_mutated_object_id: DBMap<(ObjectID, TxSequenceNumber), TransactionDigest>,

    /// Index from package id, module and function identifier to transactions that used that moce function call as input.
    #[default_options_override_fn = "transactions_by_move_function_table_default_config"]
    transactions_by_move_function:
        DBMap<(ObjectID, String, String, TxSequenceNumber), TransactionDigest>,

    /// This is a map between the transaction digest and its timestamp (UTC timestamp in
    /// **milliseconds** since epoch 1/1/1970). A transaction digest is subjectively time stamped
    /// on a node according to the local machine time, so it varies across nodes.
    /// The timestamping happens when the node sees a txn certificate for the first time.
    #[default_options_override_fn = "timestamps_table_default_config"]
    timestamps: DBMap<TransactionDigest, u64>,

    /// Ordering of all indexed transactions.
    #[default_options_override_fn = "transactions_order_table_default_config"]
    transaction_order: DBMap<TxSequenceNumber, TransactionDigest>,

    /// Index from transaction digest to sequence number.
    #[default_options_override_fn = "transactions_seq_table_default_config"]
    transactions_seq: DBMap<TransactionDigest, TxSequenceNumber>,

    /// This is an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    #[default_options_override_fn = "owner_index_table_default_config"]
    owner_index: DBMap<OwnerIndexKey, ObjectInfo>,

    #[default_options_override_fn = "coin_index_table_default_config"]
    coin_index: DBMap<CoinIndexKey, CoinInfo>,

    /// This is an index of object references to currently existing dynamic field object, indexed by the
    /// composite key of the object ID of their parent and the object ID of the dynamic field object.
    /// This composite index allows an efficient iterator to list all objects currently owned
    /// by a specific object, and their object reference.
    #[default_options_override_fn = "dynamic_field_index_table_default_config"]
    dynamic_field_index: DBMap<DynamicFieldKey, DynamicFieldInfo>,

    /// This is an index of all the versions of loaded child objects
    loaded_child_object_versions: DBMap<TransactionDigest, Vec<(ObjectID, SequenceNumber)>>,

    #[default_options_override_fn = "index_table_default_config"]
    event_order: DBMap<EventId, EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_move_module: DBMap<(ModuleId, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_move_event: DBMap<(StructTag, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_sender: DBMap<(SuiAddress, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_time: DBMap<(u64, EventId), EventIndex>,
}

pub struct IndexStore {
    next_sequence_number: AtomicU64,
    tables: IndexStoreTables,
    caches: IndexStoreCaches,
    metrics: Arc<IndexStoreMetrics>,
    max_type_length: u64,
}

// These functions are used to initialize the DB tables
fn transactions_order_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_seq_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_from_addr_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_to_addr_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_input_object_id_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_mutated_object_id_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_move_function_table_default_config() -> DBOptions {
    default_db_options()
}
fn timestamps_table_default_config() -> DBOptions {
    default_db_options().optimize_for_point_lookup(64)
}
fn owner_index_table_default_config() -> DBOptions {
    default_db_options()
}
fn dynamic_field_index_table_default_config() -> DBOptions {
    default_db_options()
}
fn index_table_default_config() -> DBOptions {
    default_db_options()
}
fn coin_index_table_default_config() -> DBOptions {
    DBOptions {
        options: default_db_options()
            .optimize_for_write_throughput()
            .optimize_for_read(
                read_size_from_env(ENV_VAR_COIN_INDEX_BLOCK_CACHE_SIZE_MB).unwrap_or(5 * 1024),
            )
            .options,
        rw_options: ReadWriteOptions {
            ignore_range_deletions: true,
        },
    }
}

impl IndexStore {
    pub fn new(path: PathBuf, registry: &Registry, max_type_length: Option<u64>) -> Self {
        let tables =
            IndexStoreTables::open_tables_read_write(path, MetricConf::default(), None, None);
        let metrics = IndexStoreMetrics::new(registry);
        let caches = IndexStoreCaches {
            per_coin_type_balance: ShardedLruCache::new(1_000_000, 1000),
            all_balances: ShardedLruCache::new(1_000_000, 1000),
            locks: MutexTable::new(128),
        };
        let next_sequence_number = tables
            .transaction_order
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq + 1)
            .unwrap_or(0)
            .into();

        Self {
            tables,
            next_sequence_number,
            caches,
            metrics: Arc::new(metrics),
            max_type_length: max_type_length.unwrap_or(128),
        }
    }

    pub async fn index_coin(
        &self,
        digest: &TransactionDigest,
        batch: &mut DBBatch,
        object_index_changes: &ObjectIndexChanges,
        tx_coins: Option<TxCoins>,
    ) -> SuiResult<IndexStoreCacheUpdates> {
        // In production if this code path is hit, we should expect `tx_coins` to not be None.
        // However, in many tests today we do not distinguish validator and/or fullnode, so
        // we gracefully exist here.
        if tx_coins.is_none() {
            return Ok(IndexStoreCacheUpdates::default());
        }
        // Acquire locks on changed coin owners
        let mut addresses: HashSet<SuiAddress> = HashSet::new();
        addresses.extend(
            object_index_changes
                .deleted_owners
                .iter()
                .map(|(owner, _)| *owner),
        );
        addresses.extend(
            object_index_changes
                .new_owners
                .iter()
                .map(|((owner, _), _)| *owner),
        );
        let _locks = self.caches.locks.acquire_locks(addresses.into_iter()).await;
        let mut balance_changes: HashMap<SuiAddress, HashMap<TypeTag, TotalBalance>> =
            HashMap::new();
        // Index coin info
        let (input_coins, written_coins) = tx_coins.unwrap();
        // 1. Delete old owner if the object is deleted or transferred to a new owner,
        // by looking at `object_index_changes.deleted_owners`.
        // Objects in `deleted_owners` must be owned by `Owner::Address` before the tx,
        // hence must appear in the tx inputs.
        // They also mut be coin type (see `AuthorityState::commit_certificate`).
        let coin_delete_keys = object_index_changes
            .deleted_owners
            .iter()
            .filter_map(|(owner, obj_id)| {
                // If it's not in `input_coins`, then it's not a coin type. Skip.
                let object = input_coins.get(obj_id)?;
                let coin_type_tag = object.coin_type_maybe().unwrap_or_else(|| {
                    panic!(
                        "object_id: {:?} in input_coins is not a coin type, input_coins: {:?}, tx_digest: {:?}",
                        obj_id, input_coins, digest
                    )
                });
                let map = balance_changes.entry(*owner).or_insert(HashMap::new());
                let entry = map.entry(coin_type_tag.clone()).or_insert(TotalBalance {
                    num_coins: 0,
                    balance: 0
                });
                if let Ok(Some(coin_info)) = &self.tables.coin_index.get(&(*owner, coin_type_tag.to_string(), *obj_id)) {
                    entry.num_coins -= 1;
                    entry.balance -= coin_info.balance as i128;
                }
                Some((*owner, coin_type_tag.to_string(), *obj_id))
            }).collect::<Vec<_>>();
        trace!(
            tx_digset=?digest,
            "coin_delete_keys: {:?}",
            coin_delete_keys,
        );
        batch.delete_batch(&self.tables.coin_index, coin_delete_keys.into_iter())?;

        // 2. Upsert new owner, by looking at `object_index_changes.new_owners`.
        // For a object to appear in `new_owners`, it must be owned by `Owner::Address` after the tx.
        // It also must not be deleted, hence appear in written_coins (see `AuthorityState::commit_certificate`)
        // It also must be a coin type (see `AuthorityState::commit_certificate`).
        // Here the coin could be transferred to a new address, to simply have the metadata changed (digest, balance etc)
        // due to a successful or failed transaction.
        let coin_add_keys = object_index_changes
        .new_owners
        .iter()
        .filter_map(|((owner, obj_id), obj_info)| {
            // If it's in written_coins, then it's not a coin. Skip it.
            let (_obj_ref, obj, _write_kind) = written_coins.get(obj_id)?;
            let coin_type_tag = obj.coin_type_maybe().unwrap_or_else(|| {
                panic!(
                    "object_id: {:?} in written_coins is not a coin type, written_coins: {:?}, tx_digest: {:?}",
                    obj_id, written_coins, digest
                )
            });
            let coin = obj.as_coin_maybe().unwrap_or_else(|| {
                panic!(
                    "object_id: {:?} in written_coins cannot be deserialzied as a Coin, written_coins: {:?}, tx_digest: {:?}",
                    obj_id, written_coins, digest
                )
            });
            let map = balance_changes.entry(*owner).or_insert(HashMap::new());
            let entry = map.entry(coin_type_tag.clone()).or_insert(TotalBalance {
                num_coins: 0,
                balance: 0
            });
            let result = self.tables.coin_index.get(&(*owner, coin_type_tag.to_string(), *obj_id));
            if let Ok(Some(coin_info)) = &result {
                entry.balance -= coin_info.balance as i128;
                entry.balance += coin.balance.value() as i128;
            } else if let Ok(None) = &result {
                entry.num_coins += 1;
                entry.balance += coin.balance.value() as i128;
            }
            Some(((*owner, coin_type_tag.to_string(), *obj_id), (CoinInfo {version: obj_info.version, digest: obj_info.digest, balance: coin.balance.value(), previous_transaction: *digest})))
        }).collect::<Vec<_>>();
        trace!(
            tx_digset=?digest,
            "coin_add_keys: {:?}",
            coin_add_keys,
        );

        batch.insert_batch(&self.tables.coin_index, coin_add_keys.into_iter())?;

        let per_coin_type_balance_changes: Vec<_> = balance_changes
            .iter()
            .flat_map(|(address, balance_map)| {
                balance_map.iter().map(|(type_tag, balance)| {
                    (
                        (*address, type_tag.clone()),
                        Ok::<TotalBalance, SuiError>(*balance),
                    )
                })
            })
            .collect();
        let all_balance_changes: Vec<_> = balance_changes
            .into_iter()
            .map(|(address, balance_map)| {
                (
                    address,
                    Ok::<Arc<HashMap<TypeTag, TotalBalance>>, SuiError>(Arc::new(balance_map)),
                )
            })
            .collect();
        let cache_updates = IndexStoreCacheUpdates {
            _locks,
            per_coin_type_balance_changes,
            all_balance_changes,
        };
        Ok(cache_updates)
    }

    pub async fn index_tx(
        &self,
        sender: SuiAddress,
        active_inputs: impl Iterator<Item = ObjectID>,
        mutated_objects: impl Iterator<Item = (ObjectRef, Owner)> + Clone,
        move_functions: impl Iterator<Item = (ObjectID, Identifier, Identifier)> + Clone,
        events: &TransactionEvents,
        object_index_changes: ObjectIndexChanges,
        digest: &TransactionDigest,
        timestamp_ms: u64,
        tx_coins: Option<TxCoins>,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) -> SuiResult<u64> {
        let sequence = self.next_sequence_number.fetch_add(1, Ordering::SeqCst);
        let mut batch = self.tables.transactions_from_addr.batch();

        batch.insert_batch(
            &self.tables.transaction_order,
            std::iter::once((sequence, *digest)),
        )?;

        batch.insert_batch(
            &self.tables.transactions_seq,
            std::iter::once((*digest, sequence)),
        )?;

        batch.insert_batch(
            &self.tables.transactions_from_addr,
            std::iter::once(((sender, sequence), *digest)),
        )?;

        batch.insert_batch(
            &self.tables.transactions_by_input_object_id,
            active_inputs.map(|id| ((id, sequence), *digest)),
        )?;

        batch.insert_batch(
            &self.tables.transactions_by_mutated_object_id,
            mutated_objects
                .clone()
                .map(|(obj_ref, _)| ((obj_ref.0, sequence), *digest)),
        )?;

        batch.insert_batch(
            &self.tables.transactions_by_move_function,
            move_functions.map(|(obj_id, module, function)| {
                (
                    (obj_id, module.to_string(), function.to_string(), sequence),
                    *digest,
                )
            }),
        )?;

        batch.insert_batch(
            &self.tables.transactions_to_addr,
            mutated_objects.filter_map(|(_, owner)| {
                owner
                    .get_owner_address()
                    .ok()
                    .map(|addr| ((addr, sequence), digest))
            }),
        )?;

        batch.insert_batch(
            &self.tables.timestamps,
            std::iter::once((*digest, timestamp_ms)),
        )?;

        // Coin Index
        let cache_updates = self
            .index_coin(digest, &mut batch, &object_index_changes, tx_coins)
            .await?;

        // Owner index
        batch.delete_batch(
            &self.tables.owner_index,
            object_index_changes.deleted_owners.into_iter(),
        )?;
        batch.delete_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.deleted_dynamic_fields.into_iter(),
        )?;

        batch.insert_batch(
            &self.tables.owner_index,
            object_index_changes.new_owners.into_iter(),
        )?;

        batch.insert_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.new_dynamic_fields.into_iter(),
        )?;

        // events
        let event_digest = events.digest();
        batch.insert_batch(
            &self.tables.event_order,
            events
                .data
                .iter()
                .enumerate()
                .map(|(i, _)| ((sequence, i), (event_digest, *digest, timestamp_ms))),
        )?;
        batch.insert_batch(
            &self.tables.event_by_move_module,
            events
                .data
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    (
                        i,
                        ModuleId::new(e.package_id.into(), e.transaction_module.clone()),
                    )
                })
                .map(|(i, m)| ((m, (sequence, i)), (event_digest, *digest, timestamp_ms))),
        )?;
        batch.insert_batch(
            &self.tables.event_by_sender,
            events.data.iter().enumerate().map(|(i, e)| {
                (
                    (e.sender, (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;
        batch.insert_batch(
            &self.tables.event_by_move_event,
            events.data.iter().enumerate().map(|(i, e)| {
                (
                    (e.type_.clone(), (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;

        batch.insert_batch(
            &self.tables.event_by_time,
            events.data.iter().enumerate().map(|(i, _)| {
                (
                    (timestamp_ms, (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;

        // Loaded child objects table
        let loaded_child_objects: Vec<_> = loaded_child_objects.into_iter().collect();
        batch.insert_batch(
            &self.tables.loaded_child_object_versions,
            std::iter::once((*digest, loaded_child_objects)),
        )?;

        let invalidate_caches =
            read_size_from_env(ENV_VAR_INVALIDATE_INSTEAD_OF_UPDATE).unwrap_or(0) > 0;

        if invalidate_caches {
            // Invalidate cache before writing to db so we always serve latest values
            self.invalidate_per_coin_type_cache(
                cache_updates
                    .per_coin_type_balance_changes
                    .iter()
                    .map(|x| x.0.clone()),
            )
            .await?;
            self.invalidate_all_balance_cache(
                cache_updates.all_balance_changes.iter().map(|x| x.0),
            )
            .await?;
        }

        batch.write()?;

        if !invalidate_caches {
            // We cannot update the cache before updating the db or else on failing to write to db
            // we will update the cache (when we retry to index this transaction again we would have
            // updated the cache twice). However, this only means cache is eventually consistent with
            // the db (within a very short delay)
            self.update_per_coin_type_cache(cache_updates.per_coin_type_balance_changes)
                .await?;
            self.update_all_balance_cache(cache_updates.all_balance_changes)
                .await?;
        }
        Ok(sequence)
    }

    pub fn next_sequence_number(&self) -> TxSequenceNumber {
        self.next_sequence_number.load(Ordering::SeqCst) + 1
    }

    pub fn get_transactions(
        &self,
        filter: Option<TransactionFilter>,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        // Lookup TransactionDigest sequence number,
        let cursor = if let Some(cursor) = cursor {
            Some(
                self.get_transaction_seq(&cursor)?
                    .ok_or_else(|| anyhow!("Transaction [{cursor:?}] not found."))?,
            )
        } else {
            None
        };
        match filter {
            Some(TransactionFilter::MoveFunction {
                package,
                module,
                function,
            }) => Ok(self.get_transactions_by_move_function(
                package, module, function, cursor, limit, reverse,
            )?),
            Some(TransactionFilter::InputObject(object_id)) => {
                Ok(self.get_transactions_by_input_object(object_id, cursor, limit, reverse)?)
            }
            Some(TransactionFilter::ChangedObject(object_id)) => {
                Ok(self.get_transactions_by_mutated_object(object_id, cursor, limit, reverse)?)
            }
            Some(TransactionFilter::FromAddress(address)) => {
                Ok(self.get_transactions_from_addr(address, cursor, limit, reverse)?)
            }
            Some(TransactionFilter::ToAddress(address)) => {
                Ok(self.get_transactions_to_addr(address, cursor, limit, reverse)?)
            }
            // NOTE: filter via checkpoint sequence number is implemented in
            // `get_transactions` of authority.rs.
            Some(_) => Err(anyhow!("Unsupported filter: {:?}", filter)),
            None => {
                let iter = self.tables.transaction_order.iter();

                if reverse {
                    let iter = iter
                        .skip_prior_to(&cursor.unwrap_or(TxSequenceNumber::MAX))?
                        .reverse()
                        .skip(usize::from(cursor.is_some()))
                        .map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        Ok(iter.take(limit).collect())
                    } else {
                        Ok(iter.collect())
                    }
                } else {
                    let iter = iter
                        .skip_to(&cursor.unwrap_or(TxSequenceNumber::MIN))?
                        .skip(usize::from(cursor.is_some()))
                        .map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        Ok(iter.take(limit).collect())
                    } else {
                        Ok(iter.collect())
                    }
                }
            }
        }
    }

    /// Return loaded child objects table for a tx
    pub fn loaded_child_object_versions(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<Vec<(ObjectID, SequenceNumber)>>> {
        self.tables
            .loaded_child_object_versions
            .get(transaction_digest)
            .map_err(|err| err.into())
    }

    /// Returns unix timestamp for a transaction if it exists
    pub fn get_timestamp_ms(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<u64>> {
        let ts = self.tables.timestamps.get(transaction_digest)?;
        Ok(ts)
    }

    fn get_transactions_from_index<KeyT: Clone + Serialize + DeserializeOwned + PartialEq>(
        index: &DBMap<(KeyT, TxSequenceNumber), TransactionDigest>,
        key: KeyT,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Ok(if reverse {
            let iter = index
                .iter()
                .skip_prior_to(&(key.clone(), cursor.unwrap_or(TxSequenceNumber::MAX)))?
                .reverse()
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, _), _)| *id == key)
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        } else {
            let iter = index
                .iter()
                .skip_to(&(key.clone(), cursor.unwrap_or(TxSequenceNumber::MIN)))?
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, _), _)| *id == key)
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        })
    }

    pub fn get_transactions_by_input_object(
        &self,
        input_object: ObjectID,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_by_input_object_id,
            input_object,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_by_mutated_object(
        &self,
        mutated_object: ObjectID,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_by_mutated_object_id,
            mutated_object,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_from_addr,
            addr,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        // If we are passed a function with no module return a UserInputError
        if function.is_some() && module.is_none() {
            return Err(SuiError::UserInputError {
                error: UserInputError::MoveFunctionInputError(
                    "Cannot supply function without supplying module".to_string(),
                ),
            });
        }

        // We cannot have a cursor without filling out the other keys.
        if cursor.is_some() && (module.is_none() || function.is_none()) {
            return Err(SuiError::UserInputError {
                error: UserInputError::MoveFunctionInputError(
                    "Cannot supply cursor without supplying module and function".to_string(),
                ),
            });
        }

        let cursor_val = cursor.unwrap_or(if reverse {
            TxSequenceNumber::MAX
        } else {
            TxSequenceNumber::MIN
        });

        let max_string = "Z".repeat(self.max_type_length.try_into().unwrap());
        let module_val = module.clone().unwrap_or(if reverse {
            max_string.clone()
        } else {
            "".to_string()
        });

        let function_val =
            function
                .clone()
                .unwrap_or(if reverse { max_string } else { "".to_string() });

        let key = (package, module_val, function_val, cursor_val);
        let iter = self.tables.transactions_by_move_function.iter();
        Ok(if reverse {
            let iter = iter
                .skip_prior_to(&key)?
                .reverse()
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, m, f, _), _)| {
                    *id == package
                        && module.as_ref().map(|x| x == m).unwrap_or(true)
                        && function.as_ref().map(|x| x == f).unwrap_or(true)
                })
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        } else {
            let iter = iter
                .skip_to(&key)?
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, m, f, _), _)| {
                    *id == package
                        && module.as_ref().map(|x| x == m).unwrap_or(true)
                        && function.as_ref().map(|x| x == f).unwrap_or(true)
                })
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        })
    }

    pub fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_to_addr,
            addr,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transaction_seq(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        Ok(self.tables.transactions_seq.get(digest)?)
    }

    pub fn all_events(
        &self,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            self.tables
                .event_order
                .iter()
                .skip_prior_to(&(tx_seq, event_seq))?
                .reverse()
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_order
                .iter()
                .skip_to(&(tx_seq, event_seq))?
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn events_by_transaction(
        &self,
        digest: &TransactionDigest,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        let seq = self
            .get_transaction_seq(digest)?
            .ok_or(SuiError::TransactionNotFound { digest: *digest })?;
        Ok(if descending {
            self.tables
                .event_order
                .iter()
                .skip_prior_to(&(min(tx_seq, seq), event_seq))?
                .reverse()
                .take_while(|((tx, _), _)| tx == &seq)
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_order
                .iter()
                .skip_to(&(max(tx_seq, seq), event_seq))?
                .take_while(|((tx, _), _)| tx == &seq)
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    fn get_event_from_index<KeyT: Clone + PartialEq + Serialize + DeserializeOwned>(
        index: &DBMap<(KeyT, EventId), (TransactionEventsDigest, TransactionDigest, u64)>,
        key: &KeyT,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            index
                .iter()
                .skip_prior_to(&(key.clone(), (tx_seq, event_seq)))?
                .reverse()
                .take_while(|((m, _), _)| m == key)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            index
                .iter()
                .skip_to(&(key.clone(), (tx_seq, event_seq)))?
                .take_while(|((m, _), _)| m == key)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn events_by_module_id(
        &self,
        module: &ModuleId,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_move_module,
            module,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn events_by_move_event_struct_name(
        &self,
        struct_name: &StructTag,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_move_event,
            struct_name,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn events_by_sender(
        &self,
        sender: &SuiAddress,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_sender,
            sender,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            self.tables
                .event_by_time
                .iter()
                .skip_prior_to(&(end_time, (tx_seq, event_seq)))?
                .reverse()
                .take_while(|((m, _), _)| m >= &start_time)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_by_time
                .iter()
                .skip_to(&(start_time, (tx_seq, event_seq)))?
                .take_while(|((m, _), _)| m <= &end_time)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn get_dynamic_fields_iterator(
        &self,
        object: ObjectID,
        cursor: Option<ObjectID>,
    ) -> SuiResult<impl Iterator<Item = DynamicFieldInfo> + '_> {
        debug!(?object, "get_dynamic_fields");
        let iter_lower_bound = (object, ObjectID::ZERO);
        let iter_upper_bound = (object, ObjectID::MAX);
        Ok(self
            .tables
            .dynamic_field_index
            .iter_with_bounds(Some(iter_lower_bound), Some(iter_upper_bound))
            // The object id 0 is the smallest possible
            .skip_to(&(object, cursor.unwrap_or(ObjectID::ZERO)))?
            // skip an extra b/c the cursor is exclusive
            .skip(usize::from(cursor.is_some()))
            .take_while(move |((object_owner, _), _)| (object_owner == &object))
            .map(|(_, object_info)| object_info))
    }

    pub fn get_dynamic_field_object_id(
        &self,
        object: ObjectID,
        name_type: TypeTag,
        name_bcs_bytes: &[u8],
    ) -> SuiResult<Option<ObjectID>> {
        debug!(?object, "get_dynamic_field_object_id");
        let dynamic_field_id =
            dynamic_field::derive_dynamic_field_id(object, &name_type, name_bcs_bytes).map_err(
                |e| {
                    SuiError::Unknown(format!(
                        "Unable to generate dynamic field id. Got error: {e:?}"
                    ))
                },
            )?;

        if let Some(info) = self
            .tables
            .dynamic_field_index
            .get(&(object, dynamic_field_id))?
        {
            // info.object_id != dynamic_field_id ==> is_wrapper
            debug_assert!(
                info.object_id == dynamic_field_id
                    || matches!(name_type, TypeTag::Struct(tag) if DynamicFieldInfo::is_dynamic_object_field_wrapper(&tag))
            );
            return Ok(Some(info.object_id));
        }

        let dynamic_object_field_struct = DynamicFieldInfo::dynamic_object_field_wrapper(name_type);
        let dynamic_object_field_type = TypeTag::Struct(Box::new(dynamic_object_field_struct));
        let dynamic_object_field_id = dynamic_field::derive_dynamic_field_id(
            object,
            &dynamic_object_field_type,
            name_bcs_bytes,
        )
        .map_err(|e| {
            SuiError::Unknown(format!(
                "Unable to generate dynamic field id. Got error: {e:?}"
            ))
        })?;
        if let Some(info) = self
            .tables
            .dynamic_field_index
            .get(&(object, dynamic_object_field_id))?
        {
            return Ok(Some(info.object_id));
        }

        Ok(None)
    }

    pub fn get_owner_objects(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
        limit: usize,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<Vec<ObjectInfo>> {
        let cursor = match cursor {
            Some(cursor) => cursor,
            None => ObjectID::ZERO,
        };
        Ok(self
            .get_owner_objects_iterator(owner, cursor, filter)?
            .take(limit)
            .collect())
    }

    pub fn get_owned_coins_iterator(
        coin_index: &DBMap<CoinIndexKey, CoinInfo>,
        owner: SuiAddress,
        coin_type_tag: Option<String>,
    ) -> SuiResult<impl Iterator<Item = (String, ObjectID, CoinInfo)> + '_> {
        let all_coins = coin_type_tag.is_none();
        let starting_coin_type =
            coin_type_tag.unwrap_or_else(|| String::from_utf8([0u8].to_vec()).unwrap());
        Ok(coin_index
            .iter()
            .skip_to(&(owner, starting_coin_type.clone(), ObjectID::ZERO))?
            .take_while(move |((addr, coin_type, _), _)| {
                if addr != &owner {
                    return false;
                }
                if !all_coins && &starting_coin_type != coin_type {
                    return false;
                }
                true
            })
            .map(|((_, coin_type, obj_id), coin)| (coin_type, obj_id, coin)))
    }

    pub fn get_owned_coins_iterator_with_cursor(
        &self,
        owner: SuiAddress,
        cursor: (String, ObjectID),
        limit: usize,
        one_coin_type_only: bool,
    ) -> SuiResult<impl Iterator<Item = (String, ObjectID, CoinInfo)> + '_> {
        let (starting_coin_type, starting_object_id) = cursor;
        Ok(self
            .tables
            .coin_index
            .iter()
            .skip_to(&(owner, starting_coin_type.clone(), starting_object_id))?
            .filter(move |((_, _, obj_id), _)| obj_id != &starting_object_id)
            .enumerate()
            .take_while(move |(index, ((addr, coin_type, _), _))| {
                if *index >= limit {
                    return false;
                }
                if addr != &owner {
                    return false;
                }
                if one_coin_type_only && &starting_coin_type != coin_type {
                    return false;
                }
                true
            })
            .map(|(_, ((_, coin_type, obj_id), coin))| (coin_type, obj_id, coin)))
    }

    /// starting_object_id can be used to implement pagination, where a client remembers the last
    /// object id of each page, and use it to query the next page.
    pub fn get_owner_objects_iterator(
        &self,
        owner: SuiAddress,
        starting_object_id: ObjectID,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<impl Iterator<Item = ObjectInfo> + '_> {
        Ok(self
            .tables
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, starting_object_id))?
            .skip(usize::from(starting_object_id != ObjectID::ZERO))
            .take_while(move |((address_owner, _), _)| address_owner == &owner)
            .filter(move |(_, o)| {
                if let Some(filter) = filter.as_ref() {
                    filter.matches(o)
                } else {
                    true
                }
            })
            .map(|(_, object_info)| object_info))
    }

    pub fn insert_genesis_objects(&self, object_index_changes: ObjectIndexChanges) -> SuiResult {
        let mut batch = self.tables.owner_index.batch();
        batch.insert_batch(
            &self.tables.owner_index,
            object_index_changes.new_owners.into_iter(),
        )?;
        batch.insert_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.new_dynamic_fields.into_iter(),
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.tables.owner_index.is_empty()
    }

    pub fn checkpoint_db(&self, path: &Path) -> SuiResult {
        // We are checkpointing the whole db
        self.tables
            .transactions_from_addr
            .checkpoint_db(path)
            .map_err(SuiError::StorageError)
    }

    /// This method first gets the balance from `per_coin_type_balance` cache. On a cache miss, it
    /// gets the balance for passed in `coin_type` from the `all_balance` cache. Only on the second
    /// cache miss, we go to the database (expensive) and update the cache. Notice that db read is
    /// done with `spawn_blocking` as that is expected to block
    pub async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: TypeTag,
    ) -> SuiResult<TotalBalance> {
        let force_disable_cache = read_size_from_env(ENV_VAR_DISABLE_INDEX_CACHE).unwrap_or(0) > 0;
        let cloned_coin_type = coin_type.clone();
        let metrics_cloned = self.metrics.clone();
        let coin_index_cloned = self.tables.coin_index.clone();
        if force_disable_cache {
            return spawn_blocking(move || {
                Self::get_balance_from_db(
                    metrics_cloned,
                    coin_index_cloned,
                    owner,
                    cloned_coin_type,
                )
            })
            .await
            .unwrap()
            .map_err(|e| {
                SuiError::ExecutionError(format!("Failed to read balance frm DB: {:?}", e))
            });
        }

        self.metrics.balance_lookup_from_total.inc();

        let balance = self
            .caches
            .per_coin_type_balance
            .get(&(owner, coin_type.clone()))
            .await;
        if let Some(balance) = balance {
            return balance;
        }
        // cache miss, lookup in all balance cache
        let all_balance = self.caches.all_balances.get(&owner.clone()).await;
        if let Some(Ok(all_balance)) = all_balance {
            if let Some(balance) = all_balance.get(&coin_type) {
                return Ok(*balance);
            }
        }
        let cloned_coin_type = coin_type.clone();
        let metrics_cloned = self.metrics.clone();
        let coin_index_cloned = self.tables.coin_index.clone();
        self.caches
            .per_coin_type_balance
            .get_with((owner, coin_type), async move {
                spawn_blocking(move || {
                    Self::get_balance_from_db(
                        metrics_cloned,
                        coin_index_cloned,
                        owner,
                        cloned_coin_type,
                    )
                })
                .await
                .unwrap()
                .map_err(|e| {
                    SuiError::ExecutionError(format!("Failed to read balance frm DB: {:?}", e))
                })
            })
            .await
    }

    /// This method gets the balance for all coin types from the `all_balance` cache. On a cache miss,
    /// we go to the database (expensive) and update the cache. This cache is dual purpose in the
    /// sense that it not only serves `get_AllBalance()` calls but is also used for serving
    /// `get_Balance()` queries. Notice that db read is performed with `spawn_blocking` as that is
    /// expected to block
    pub async fn get_all_balance(
        &self,
        owner: SuiAddress,
    ) -> SuiResult<Arc<HashMap<TypeTag, TotalBalance>>> {
        let force_disable_cache = read_size_from_env(ENV_VAR_DISABLE_INDEX_CACHE).unwrap_or(0) > 0;
        let metrics_cloned = self.metrics.clone();
        let coin_index_cloned = self.tables.coin_index.clone();
        if force_disable_cache {
            return spawn_blocking(move || {
                Self::get_all_balances_from_db(metrics_cloned, coin_index_cloned, owner)
            })
            .await
            .unwrap()
            .map_err(|e| {
                SuiError::ExecutionError(format!("Failed to read all balance from DB: {:?}", e))
            });
        }

        self.metrics.all_balance_lookup_from_total.inc();
        let metrics_cloned = self.metrics.clone();
        let coin_index_cloned = self.tables.coin_index.clone();
        self.caches
            .all_balances
            .get_with(owner, async move {
                spawn_blocking(move || {
                    Self::get_all_balances_from_db(metrics_cloned, coin_index_cloned, owner)
                })
                .await
                .unwrap()
                .map_err(|e| {
                    SuiError::ExecutionError(format!("Failed to read all balance from DB: {:?}", e))
                })
            })
            .await
    }

    /// Read balance for a `SuiAddress` and `CoinType` from the backend database
    pub fn get_balance_from_db(
        metrics: Arc<IndexStoreMetrics>,
        coin_index: DBMap<CoinIndexKey, CoinInfo>,
        owner: SuiAddress,
        coin_type: TypeTag,
    ) -> SuiResult<TotalBalance> {
        metrics.balance_lookup_from_db.inc();
        let coin_type_str = coin_type.to_string();
        let coins =
            Self::get_owned_coins_iterator(&coin_index, owner, Some(coin_type_str.clone()))?
                .map(|(_coin_type, obj_id, coin)| (coin_type_str.clone(), obj_id, coin));

        let mut balance = 0i128;
        let mut num_coins = 0;
        for (_coin_type, _obj_id, coin_info) in coins {
            balance += coin_info.balance as i128;
            num_coins += 1;
        }
        Ok(TotalBalance { balance, num_coins })
    }

    /// Read all balances for a `SuiAddress` from the backend database
    pub fn get_all_balances_from_db(
        metrics: Arc<IndexStoreMetrics>,
        coin_index: DBMap<CoinIndexKey, CoinInfo>,
        owner: SuiAddress,
    ) -> SuiResult<Arc<HashMap<TypeTag, TotalBalance>>> {
        metrics.all_balance_lookup_from_db.inc();
        let mut balances: HashMap<TypeTag, TotalBalance> = HashMap::new();
        let coins = Self::get_owned_coins_iterator(&coin_index, owner, None)?
            .map(|(coin_type, obj_id, coin)| (coin_type, obj_id, coin))
            .group_by(|(coin_type, _obj_id, _coin)| coin_type.clone());
        for (coin_type, coins) in &coins {
            let mut total_balance = 0i128;
            let mut coin_object_count = 0;
            for (_coin_type, _obj_id, coin_info) in coins {
                total_balance += coin_info.balance as i128;
                coin_object_count += 1;
            }
            let coin_type =
                TypeTag::Struct(Box::new(parse_sui_struct_tag(&coin_type).map_err(|e| {
                    SuiError::ExecutionError(format!(
                        "Failed to parse event sender address: {:?}",
                        e
                    ))
                })?));
            balances.insert(
                coin_type,
                TotalBalance {
                    num_coins: coin_object_count,
                    balance: total_balance,
                },
            );
        }
        Ok(Arc::new(balances))
    }

    async fn invalidate_per_coin_type_cache(
        &self,
        keys: impl IntoIterator<Item = (SuiAddress, TypeTag)>,
    ) -> SuiResult {
        self.caches
            .per_coin_type_balance
            .batch_invalidate(keys)
            .await;
        Ok(())
    }

    async fn invalidate_all_balance_cache(
        &self,
        addresses: impl IntoIterator<Item = SuiAddress>,
    ) -> SuiResult {
        self.caches.all_balances.batch_invalidate(addresses).await;
        Ok(())
    }

    async fn update_per_coin_type_cache(
        &self,
        keys: impl IntoIterator<Item = ((SuiAddress, TypeTag), SuiResult<TotalBalance>)>,
    ) -> SuiResult {
        self.caches
            .per_coin_type_balance
            .batch_merge(keys, Self::merge_balance)
            .await;
        Ok(())
    }

    fn merge_balance(
        old_balance: &SuiResult<TotalBalance>,
        balance_delta: &SuiResult<TotalBalance>,
    ) -> SuiResult<TotalBalance> {
        if let Ok(old_balance) = old_balance {
            if let Ok(balance_delta) = balance_delta {
                Ok(TotalBalance {
                    balance: old_balance.balance + balance_delta.balance,
                    num_coins: old_balance.num_coins + balance_delta.num_coins,
                })
            } else {
                balance_delta.clone()
            }
        } else {
            old_balance.clone()
        }
    }

    async fn update_all_balance_cache(
        &self,
        keys: impl IntoIterator<Item = (SuiAddress, SuiResult<Arc<HashMap<TypeTag, TotalBalance>>>)>,
    ) -> SuiResult {
        self.caches
            .all_balances
            .batch_merge(keys, Self::merge_all_balance)
            .await;
        Ok(())
    }

    fn merge_all_balance(
        old_balance: &SuiResult<Arc<HashMap<TypeTag, TotalBalance>>>,
        balance_delta: &SuiResult<Arc<HashMap<TypeTag, TotalBalance>>>,
    ) -> SuiResult<Arc<HashMap<TypeTag, TotalBalance>>> {
        if let Ok(old_balance) = old_balance {
            if let Ok(balance_delta) = balance_delta {
                let mut new_balance = HashMap::new();
                for (key, value) in old_balance.iter() {
                    new_balance.insert(key.clone(), *value);
                }
                for (key, delta) in balance_delta.iter() {
                    let old = new_balance.entry(key.clone()).or_insert(TotalBalance {
                        balance: 0,
                        num_coins: 0,
                    });
                    let new_total = TotalBalance {
                        balance: old.balance + delta.balance,
                        num_coins: old.num_coins + delta.num_coins,
                    };
                    new_balance.insert(key.clone(), new_total);
                }
                Ok(Arc::new(new_balance))
            } else {
                balance_delta.clone()
            }
        } else {
            old_balance.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::indexes::ObjectIndexChanges;
    use crate::IndexStore;
    use move_core_types::account_address::AccountAddress;
    use prometheus::Registry;
    use std::collections::BTreeMap;
    use std::env::temp_dir;
    use sui_types::base_types::{ObjectInfo, ObjectType, SuiAddress};
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::TransactionEvents;
    use sui_types::gas_coin::GAS;
    use sui_types::object;
    use sui_types::object::Owner;
    use sui_types::storage::WriteKind;

    #[tokio::test]
    async fn test_index_cache() -> anyhow::Result<()> {
        // This test is going to invoke `index_tx()`where 10 coins each with balance 100
        // are going to be added to an address. The balance is then going to be read from the db
        // and the cache. It should be 1000. Then, we are going to delete 3 of those coins from
        // the address and invoke `index_tx()` again and read balance. The balance should be 700
        // and verified from both db and cache.
        // This tests make sure we are invalidating entries in the cache and always reading latest
        // balance.
        let index_store = IndexStore::new(temp_dir(), &Registry::default(), Some(128));
        let address: SuiAddress = AccountAddress::random().into();
        let mut written_objects = BTreeMap::new();
        let mut object_map = BTreeMap::new();

        let mut new_objects = vec![];
        for _i in 0..10 {
            let object = object::Object::new_gas_with_balance_and_owner_for_testing(100, address);
            new_objects.push((
                (address, object.id()),
                ObjectInfo {
                    object_id: object.id(),
                    version: object.version(),
                    digest: object.digest(),
                    type_: ObjectType::Struct(object.type_().unwrap().clone()),
                    owner: Owner::AddressOwner(address),
                    previous_transaction: object.previous_transaction,
                },
            ));
            object_map.insert(object.id(), object.clone());
            written_objects.insert(
                object.data.id(),
                (object.compute_object_reference(), object, WriteKind::Mutate),
            );
        }
        let object_index_changes = ObjectIndexChanges {
            deleted_owners: vec![],
            deleted_dynamic_fields: vec![],
            new_owners: new_objects,
            new_dynamic_fields: vec![],
        };

        let tx_coins = (object_map.clone(), written_objects.clone());
        index_store
            .index_tx(
                address,
                vec![].into_iter(),
                vec![].into_iter(),
                vec![].into_iter(),
                &TransactionEvents { data: vec![] },
                object_index_changes,
                &TransactionDigest::random(),
                1234,
                Some(tx_coins),
                BTreeMap::new(),
            )
            .await?;

        let balance_from_db = IndexStore::get_balance_from_db(
            index_store.metrics.clone(),
            index_store.tables.coin_index.clone(),
            address,
            GAS::type_tag(),
        )?;
        let balance = index_store.get_balance(address, GAS::type_tag()).await?;
        assert_eq!(balance, balance_from_db);
        assert_eq!(balance.balance, 1000);
        assert_eq!(balance.num_coins, 10);

        let all_balance = index_store.get_all_balance(address).await?;
        let balance = all_balance.get(&GAS::type_tag()).unwrap();
        assert_eq!(*balance, balance_from_db);
        assert_eq!(balance.balance, 1000);
        assert_eq!(balance.num_coins, 10);

        written_objects.clear();
        let mut deleted_objects = vec![];
        for (id, object) in object_map.iter().take(3) {
            deleted_objects.push((address, *id));
            written_objects.insert(
                object.data.id(),
                (
                    object.compute_object_reference(),
                    object.clone(),
                    WriteKind::Create,
                ),
            );
        }
        let object_index_changes = ObjectIndexChanges {
            deleted_owners: deleted_objects,
            deleted_dynamic_fields: vec![],
            new_owners: vec![],
            new_dynamic_fields: vec![],
        };
        let tx_coins = (object_map, written_objects);
        index_store
            .index_tx(
                address,
                vec![].into_iter(),
                vec![].into_iter(),
                vec![].into_iter(),
                &TransactionEvents { data: vec![] },
                object_index_changes,
                &TransactionDigest::random(),
                1234,
                Some(tx_coins),
                BTreeMap::new(),
            )
            .await?;
        let balance_from_db = IndexStore::get_balance_from_db(
            index_store.metrics.clone(),
            index_store.tables.coin_index.clone(),
            address,
            GAS::type_tag(),
        )?;
        let balance = index_store.get_balance(address, GAS::type_tag()).await?;
        assert_eq!(balance, balance_from_db);
        assert_eq!(balance.balance, 700);
        assert_eq!(balance.num_coins, 7);
        // Invalidate per coin type balance cache and read from all balance cache to ensure
        // the balance matches
        index_store
            .caches
            .per_coin_type_balance
            .invalidate(&(address, GAS::type_tag()))
            .await;
        let all_balance = index_store.get_all_balance(address).await;
        let all_balance = all_balance?;
        assert_eq!(all_balance.get(&GAS::type_tag()).unwrap().balance, 700);
        assert_eq!(all_balance.get(&GAS::type_tag()).unwrap().num_coins, 7);
        let balance = index_store.get_balance(address, GAS::type_tag()).await?;
        assert_eq!(balance, balance_from_db);
        assert_eq!(balance.balance, 700);
        assert_eq!(balance.num_coins, 7);

        Ok(())
    }
}
