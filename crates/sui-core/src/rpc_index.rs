// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::par_index_live_object_set::LiveObjectIndexer;
use crate::par_index_live_object_set::ParMakeLiveObjectIndexer;
use itertools::Itertools;
use move_core_types::language_storage::{StructTag, TypeTag};
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use serde::Deserialize;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use sui_config::RpcIndexInitConfig;
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::coin::Coin;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Data;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::TransactionInfo;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use tracing::{debug, info};
use typed_store::rocks::{DBMap, DBMapTableConfigMap, MetricConf};
use typed_store::rocksdb::{MergeOperands, WriteOptions};
use typed_store::traits::Map;
use typed_store::DBMapUtils;
use typed_store::TypedStoreError;

const CURRENT_DB_VERSION: u64 = 3;
// I tried increasing this to 100k and 1M and it didn't speed up indexing at all.
const BALANCE_FLUSH_THRESHOLD: usize = 10_000;

fn bulk_ingestion_write_options() -> WriteOptions {
    let mut opts = WriteOptions::default();
    opts.disable_wal(true);
    opts
}

/// Get available memory, respecting cgroup limits in containerized environments
fn get_available_memory() -> u64 {
    // RefreshKind::nothing().with_memory() avoids collecting other, slower stats
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
    );
    sys.refresh_memory();

    // Check if we have cgroup limits
    if let Some(cgroup_limits) = sys.cgroup_limits() {
        let memory_limit = cgroup_limits.total_memory;
        // cgroup_limits.total_memory is 0 when there's no limit
        if memory_limit > 0 {
            debug!("Using cgroup memory limit: {} bytes", memory_limit);
            return memory_limit;
        }
    }

    // Fall back to system memory if no cgroup limits found
    // sysinfo 0.35 already reports bytes (not KiB like older versions)
    let total_memory_bytes = sys.total_memory();
    debug!("Using system memory: {} bytes", total_memory_bytes);
    total_memory_bytes
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct MetadataInfo {
    /// Version of the Database
    version: u64,
}

/// Checkpoint watermark type
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Watermark {
    Indexed,
    Pruned,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct OwnerIndexKey {
    pub owner: SuiAddress,

    pub object_type: StructTag,

    // If this object is coin-like (eg 0x2::coin::Coin) then this will be the balance of the coin
    // inverted `!coin.balance` in order to force sorting of coins to be from greatest to least
    pub inverted_balance: Option<u64>,

    pub object_id: ObjectID,
}

impl OwnerIndexKey {
    // Creates a key from the provided object.
    // Panics if the provided object is not an Address owned object
    fn from_object(object: &Object) -> Self {
        let owner = match object.owner() {
            Owner::AddressOwner(owner) => owner,
            Owner::ConsensusAddressOwner { owner, .. } => owner,
            _ => panic!("cannot create OwnerIndexKey if object is not address-owned"),
        };
        let object_type = object.struct_tag().expect("packages cannot be owned");

        let inverted_balance = object.as_coin_maybe().map(|coin| !coin.balance.value());

        Self {
            owner: *owner,
            object_type,
            inverted_balance,
            object_id: object.id(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerIndexInfo {
    // object_id and type of this object are a part of the key
    pub version: SequenceNumber,
}

impl OwnerIndexInfo {
    pub fn new(object: &Object) -> Self {
        Self {
            version: object.version(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CoinIndexKey {
    coin_type: StructTag,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BalanceKey {
    pub owner: SuiAddress,
    pub coin_type: StructTag,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct CoinIndexInfo {
    pub coin_metadata_object_id: Option<ObjectID>,
    pub treasury_object_id: Option<ObjectID>,
    pub regulated_coin_metadata_object_id: Option<ObjectID>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct BalanceIndexInfo {
    pub balance_delta: i128,
}

impl From<u64> for BalanceIndexInfo {
    fn from(coin_value: u64) -> Self {
        Self {
            balance_delta: coin_value as i128,
        }
    }
}

impl BalanceIndexInfo {
    fn invert(self) -> Self {
        // Check for potential overflow when negating i128::MIN
        assert!(
            self.balance_delta != i128::MIN,
            "Cannot invert balance_delta: would overflow i128"
        );

        Self {
            balance_delta: -self.balance_delta,
        }
    }

    fn merge_delta(&mut self, other: &Self) {
        self.balance_delta += other.balance_delta;
    }
}

impl From<BalanceIndexInfo> for sui_types::storage::BalanceInfo {
    fn from(index_info: BalanceIndexInfo) -> Self {
        // Note: We represent balance deltas as i128 to simplify merging positive and negative updates.
        // Be aware: Move doesn’t enforce a one-time-witness (OTW) pattern when creating a Supply<T>.
        // Anyone can call `sui::balance::create_supply` and mint unbounded supply, potentially pushing
        // total balances over u64::MAX. To avoid crashing the indexer, we clamp the merged value instead
        // of panicking on overflow. This has the unfortunate consequence of making bugs in the index
        // harder to detect, but is a necessary trade-off to avoid creating a DOS attack vector.
        let balance = index_info.balance_delta.clamp(0, u64::MAX as i128) as u64;
        sui_types::storage::BalanceInfo { balance }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize, PartialOrd, Ord)]
pub struct PackageVersionKey {
    pub original_package_id: ObjectID,
    pub version: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct PackageVersionInfo {
    pub storage_id: ObjectID,
}

fn default_table_options() -> typed_store::rocks::DBOptions {
    typed_store::rocks::default_db_options().disable_write_throttling()
}

fn balance_delta_merge_operator(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let mut result = existing_val
        .map(|v| {
            bcs::from_bytes::<BalanceIndexInfo>(v)
                .expect("Failed to deserialize BalanceIndexInfo from RocksDB - data corruption.")
        })
        .unwrap_or_default();

    for operand in operands.iter() {
        let delta = bcs::from_bytes::<BalanceIndexInfo>(operand)
            .expect("Failed to deserialize BalanceIndexInfo from RocksDB - data corruption.");
        result.merge_delta(&delta);
    }
    Some(
        bcs::to_bytes(&result)
            .expect("Failed to deserialize BalanceIndexInfo from RocksDB - data corruption."),
    )
}

fn balance_table_options() -> typed_store::rocks::DBOptions {
    default_table_options()
        .set_merge_operator_associative("balance_merge", balance_delta_merge_operator)
}

impl CoinIndexInfo {
    fn merge(&mut self, other: Self) {
        self.coin_metadata_object_id = self
            .coin_metadata_object_id
            .or(other.coin_metadata_object_id);
        self.regulated_coin_metadata_object_id = self
            .regulated_coin_metadata_object_id
            .or(other.regulated_coin_metadata_object_id);
        self.treasury_object_id = self.treasury_object_id.or(other.treasury_object_id);
    }
}

/// RocksDB tables for the RpcIndexStore
///
/// Anytime a new table is added, or and existing one has it's schema changed, make sure to also
/// update the value of `CURRENT_DB_VERSION`.
///
/// NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
/// - bounded in size by the live object set
/// - are prune-able and have corresponding logic in the `prune` function
#[derive(DBMapUtils)]
struct IndexStoreTables {
    /// A singleton that store metadata information on the DB.
    ///
    /// A few uses for this singleton:
    /// - determining if the DB has been initialized (as some tables will still be empty post
    ///     initialization)
    /// - version of the DB. Everytime a new table or schema is changed the version number needs to
    ///     be incremented.
    meta: DBMap<(), MetadataInfo>,

    /// Table used to track watermark for the highest indexed checkpoint
    ///
    /// This is useful to help know the highest checkpoint that was indexed in the event that the
    /// node was running with indexes enabled, then run for a period of time with indexes disabled,
    /// and then run with them enabled again so that the tables can be reinitialized.
    #[default_options_override_fn = "default_table_options"]
    watermark: DBMap<Watermark, CheckpointSequenceNumber>,

    /// An index of extra metadata for Epochs.
    ///
    /// Only contains entries for transactions which have yet to be pruned from the main database.
    #[default_options_override_fn = "default_table_options"]
    epochs: DBMap<EpochId, EpochInfo>,

    /// An index of extra metadata for Transactions.
    ///
    /// Only contains entries for transactions which have yet to be pruned from the main database.
    #[default_options_override_fn = "default_table_options"]
    transactions: DBMap<TransactionDigest, TransactionInfo>,

    /// An index of object ownership.
    ///
    /// Allows an efficient iterator to list all objects currently owned by a specific user
    /// account.
    #[default_options_override_fn = "default_table_options"]
    owner: DBMap<OwnerIndexKey, OwnerIndexInfo>,

    /// An index of dynamic fields (children objects).
    ///
    /// Allows an efficient iterator to list all of the dynamic fields owned by a particular
    /// ObjectID.
    #[default_options_override_fn = "default_table_options"]
    dynamic_field: DBMap<DynamicFieldKey, ()>,

    /// An index of Coin Types
    ///
    /// Allows looking up information related to published Coins, like the ObjectID of its
    /// coorisponding CoinMetadata.
    #[default_options_override_fn = "default_table_options"]
    coin: DBMap<CoinIndexKey, CoinIndexInfo>,

    /// An index of Balances.
    ///
    /// Allows looking up balances by owner address and coin type.
    #[default_options_override_fn = "balance_table_options"]
    balance: DBMap<BalanceKey, BalanceIndexInfo>,

    /// An index of Package versions.
    ///
    /// Maps original package ID and version to the storage ID of that version.
    /// Allows efficient listing of all versions of a package.
    #[default_options_override_fn = "default_table_options"]
    package_version: DBMap<PackageVersionKey, PackageVersionInfo>,
    // NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
    // - bounded in size by the live object set
    // - are prune-able and have corresponding logic in the `prune` function
}

impl IndexStoreTables {
    fn track_coin_balance_change(
        object: &Object,
        owner: &SuiAddress,
        is_removal: bool,
        balance_changes: &mut HashMap<BalanceKey, BalanceIndexInfo>,
    ) -> Result<(), StorageError> {
        if let Some((struct_tag, value)) = get_balance_and_type_if_coin(object)? {
            let key = BalanceKey {
                owner: *owner,
                coin_type: struct_tag,
            };

            let mut delta = BalanceIndexInfo::from(value);
            if is_removal {
                delta = delta.invert();
            }

            balance_changes.entry(key).or_default().merge_delta(&delta);
        }
        Ok(())
    }

    fn extract_version_if_package(
        object: &Object,
    ) -> Option<(PackageVersionKey, PackageVersionInfo)> {
        if let Data::Package(package) = &object.data {
            let original_id = package.original_package_id();
            let version = package.version().value();
            let storage_id = object.id();

            let key = PackageVersionKey {
                original_package_id: original_id,
                version,
            };
            let info = PackageVersionInfo { storage_id };
            return Some((key, info));
        }
        None
    }

    fn open<P: Into<PathBuf>>(path: P) -> Self {
        IndexStoreTables::open_tables_read_write(
            path.into(),
            MetricConf::new("rpc-index"),
            None,
            None,
        )
    }

    fn open_with_options<P: Into<PathBuf>>(
        path: P,
        options: typed_store::rocksdb::Options,
        table_options: Option<DBMapTableConfigMap>,
    ) -> Self {
        IndexStoreTables::open_tables_read_write(
            path.into(),
            MetricConf::new("rpc-index"),
            Some(options),
            table_options,
        )
    }

    fn needs_to_do_initialization(&self, checkpoint_store: &CheckpointStore) -> bool {
        (match self.meta.get(&()) {
            Ok(Some(metadata)) => metadata.version != CURRENT_DB_VERSION,
            Ok(None) => true,
            Err(_) => true,
        }) || self.is_indexed_watermark_out_of_date(checkpoint_store)
    }

    // Check if the index watermark is behind the highets_executed watermark.
    fn is_indexed_watermark_out_of_date(&self, checkpoint_store: &CheckpointStore) -> bool {
        let highest_executed_checkpint = checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .ok()
            .flatten();
        let watermark = self.watermark.get(&Watermark::Indexed).ok().flatten();
        watermark < highest_executed_checkpint
    }

    #[tracing::instrument(skip_all)]
    fn init(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        _epoch_store: &AuthorityPerEpochStore,
        _package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
        batch_size_limit: usize,
    ) -> Result<(), StorageError> {
        info!("Initializing RPC indexes");

        let highest_executed_checkpint =
            checkpoint_store.get_highest_executed_checkpoint_seq_number()?;
        let lowest_available_checkpoint = checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()?
            .map(|c| c.saturating_add(1))
            .unwrap_or(0);
        let lowest_available_checkpoint_objects = authority_store
            .perpetual_tables
            .get_highest_pruned_checkpoint()?
            .map(|c| c.saturating_add(1))
            .unwrap_or(0);
        // Doing backfill requires processing objects so we have to restrict our backfill range
        // to the range of checkpoints that we have objects for.
        let lowest_available_checkpoint =
            lowest_available_checkpoint.max(lowest_available_checkpoint_objects);

        let checkpoint_range = highest_executed_checkpint.map(|highest_executed_checkpint| {
            lowest_available_checkpoint..=highest_executed_checkpint
        });

        if let Some(checkpoint_range) = checkpoint_range {
            self.index_existing_transactions(authority_store, checkpoint_store, checkpoint_range)?;
        }

        self.initialize_current_epoch(authority_store, checkpoint_store)?;

        // Only index live objects if genesis checkpoint has been executed.
        // If genesis hasn't been executed yet, the objects will be properly indexed
        // as checkpoints are processed through the normal checkpoint execution path.
        if highest_executed_checkpint.is_some() {
            let coin_index = Mutex::new(HashMap::new());

            let make_live_object_indexer = RpcParLiveObjectSetIndexer {
                tables: self,
                coin_index: &coin_index,
                batch_size_limit,
            };

            crate::par_index_live_object_set::par_index_live_object_set(
                authority_store,
                &make_live_object_indexer,
            )?;

            self.coin.multi_insert(coin_index.into_inner().unwrap())?;
        }

        self.watermark.insert(
            &Watermark::Indexed,
            &highest_executed_checkpint.unwrap_or(0),
        )?;

        self.meta.insert(
            &(),
            &MetadataInfo {
                version: CURRENT_DB_VERSION,
            },
        )?;

        info!("Finished initializing RPC indexes");

        Ok(())
    }

    #[tracing::instrument(skip(self, authority_store, checkpoint_store))]
    fn index_existing_transactions(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        checkpoint_range: std::ops::RangeInclusive<u64>,
    ) -> Result<(), StorageError> {
        info!(
            "Indexing {} checkpoints in range {checkpoint_range:?}",
            checkpoint_range.size_hint().0
        );
        let start_time = Instant::now();

        checkpoint_range.into_par_iter().try_for_each(|seq| {
            let checkpoint_data =
                sparse_checkpoint_data_for_backfill(authority_store, checkpoint_store, seq)?;

            let mut batch = self.transactions.batch();

            self.index_epoch(&checkpoint_data, &mut batch)?;
            self.index_transactions(&checkpoint_data, &mut batch)?;

            batch
                .write_opt(&(bulk_ingestion_write_options()))
                .map_err(StorageError::from)
        })?;

        info!(
            "Indexing checkpoints took {} seconds",
            start_time.elapsed().as_secs()
        );
        Ok(())
    }

    /// Prune data from this Index
    fn prune(
        &self,
        pruned_checkpoint_watermark: u64,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        let mut batch = self.transactions.batch();

        let transactions_to_prune = checkpoint_contents_to_prune
            .iter()
            .flat_map(|contents| contents.iter().map(|digests| digests.transaction));

        batch.delete_batch(&self.transactions, transactions_to_prune)?;
        batch.insert_batch(
            &self.watermark,
            [(Watermark::Pruned, pruned_checkpoint_watermark)],
        )?;

        batch.write()
    }

    /// Index a Checkpoint
    fn index_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        _resolver: &mut dyn LayoutResolver,
    ) -> Result<typed_store::rocks::DBBatch, StorageError> {
        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "indexing checkpoint"
        );

        let mut batch = self.transactions.batch();

        self.index_epoch(checkpoint, &mut batch)?;
        self.index_transactions(checkpoint, &mut batch)?;
        self.index_objects(checkpoint, &mut batch)?;

        batch.insert_batch(
            &self.watermark,
            [(
                Watermark::Indexed,
                checkpoint.checkpoint_summary.sequence_number,
            )],
        )?;

        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "finished indexing checkpoint"
        );

        Ok(batch)
    }

    fn index_epoch(
        &self,
        checkpoint: &CheckpointData,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        let Some(epoch_info) = checkpoint.epoch_info()? else {
            return Ok(());
        };
        if epoch_info.epoch > 0 {
            let prev_epoch = epoch_info.epoch - 1;
            let mut current_epoch = self.epochs.get(&prev_epoch)?.unwrap_or_default();
            current_epoch.epoch = prev_epoch; // set this incase there wasn't an entry
            current_epoch.end_timestamp_ms = epoch_info.start_timestamp_ms;
            current_epoch.end_checkpoint = epoch_info.start_checkpoint.map(|sq| sq - 1);
            batch.insert_batch(&self.epochs, [(prev_epoch, current_epoch)])?;
        }
        batch.insert_batch(&self.epochs, [(epoch_info.epoch, epoch_info)])?;
        Ok(())
    }

    // After attempting to reindex past epochs, ensure that the current epoch is at least partially
    // initalized
    fn initialize_current_epoch(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
    ) -> Result<(), StorageError> {
        let Some(checkpoint) = checkpoint_store.get_highest_executed_checkpoint()? else {
            return Ok(());
        };

        let system_state = sui_types::sui_system_state::get_sui_system_state(authority_store)
            .map_err(|e| StorageError::custom(format!("Failed to find system state: {e}")))?;

        let mut epoch = self.epochs.get(&checkpoint.epoch)?.unwrap_or_default();
        epoch.epoch = checkpoint.epoch;

        if epoch.protocol_version.is_none() {
            epoch.protocol_version = Some(system_state.protocol_version());
        }

        if epoch.start_timestamp_ms.is_none() {
            epoch.start_timestamp_ms = Some(system_state.epoch_start_timestamp_ms());
        }

        if epoch.reference_gas_price.is_none() {
            epoch.reference_gas_price = Some(system_state.reference_gas_price());
        }

        if epoch.system_state.is_none() {
            epoch.system_state = Some(system_state);
        }

        self.epochs.insert(&epoch.epoch, &epoch)?;

        Ok(())
    }

    fn index_transactions(
        &self,
        checkpoint: &CheckpointData,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        for tx in &checkpoint.transactions {
            let info = TransactionInfo::new(
                tx.transaction.transaction_data(),
                &tx.effects,
                &tx.input_objects,
                &tx.output_objects,
                checkpoint.checkpoint_summary.sequence_number,
            );

            let digest = tx.transaction.digest();
            batch.insert_batch(&self.transactions, [(digest, info)])?;
        }

        Ok(())
    }

    fn index_objects(
        &self,
        checkpoint: &CheckpointData,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        let mut coin_index: HashMap<CoinIndexKey, CoinIndexInfo> = HashMap::new();
        let mut balance_changes: HashMap<BalanceKey, BalanceIndexInfo> = HashMap::new();
        let mut package_version_index: Vec<(PackageVersionKey, PackageVersionInfo)> = vec![];

        for tx in &checkpoint.transactions {
            // determine changes from removed objects
            for removed_object in tx.removed_objects_pre_version() {
                match removed_object.owner() {
                    Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                        Self::track_coin_balance_change(
                            removed_object,
                            owner,
                            true,
                            &mut balance_changes,
                        )?;

                        let owner_key = OwnerIndexKey::from_object(removed_object);
                        batch.delete_batch(&self.owner, [owner_key])?;
                    }
                    Owner::ObjectOwner(object_id) => {
                        batch.delete_batch(
                            &self.dynamic_field,
                            [DynamicFieldKey::new(*object_id, removed_object.id())],
                        )?;
                    }
                    Owner::Shared { .. } | Owner::Immutable => {}
                }
            }

            // determine changes from changed objects
            for (object, old_object) in tx.changed_objects() {
                if let Some(old_object) = old_object {
                    match old_object.owner() {
                        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                            Self::track_coin_balance_change(
                                old_object,
                                owner,
                                true,
                                &mut balance_changes,
                            )?;

                            let owner_key = OwnerIndexKey::from_object(old_object);
                            batch.delete_batch(&self.owner, [owner_key])?;
                        }

                        Owner::ObjectOwner(object_id) => {
                            if old_object.owner() != object.owner() {
                                batch.delete_batch(
                                    &self.dynamic_field,
                                    [DynamicFieldKey::new(*object_id, old_object.id())],
                                )?;
                            }
                        }

                        Owner::Shared { .. } | Owner::Immutable => {}
                    }
                }

                match object.owner() {
                    Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                        Self::track_coin_balance_change(
                            object,
                            owner,
                            false,
                            &mut balance_changes,
                        )?;
                        let owner_key = OwnerIndexKey::from_object(object);
                        let owner_info = OwnerIndexInfo::new(object);
                        batch.insert_batch(&self.owner, [(owner_key, owner_info)])?;
                    }
                    Owner::ObjectOwner(parent) => {
                        if should_index_dynamic_field(object) {
                            let field_key = DynamicFieldKey::new(*parent, object.id());
                            batch.insert_batch(&self.dynamic_field, [(field_key, ())])?;
                        }
                    }
                    Owner::Shared { .. } | Owner::Immutable => {}
                }
                if let Some((key, info)) = Self::extract_version_if_package(object) {
                    package_version_index.push((key, info));
                }
            }

            // coin indexing
            //
            // coin indexing relies on the fact that CoinMetadata and TreasuryCap are created in
            // the same transaction so we don't need to worry about overriding any older value
            // that may exist in the database (because there necessarily cannot be).
            for (key, value) in tx.created_objects().flat_map(try_create_coin_index_info) {
                use std::collections::hash_map::Entry;

                match coin_index.entry(key) {
                    Entry::Occupied(mut o) => {
                        o.get_mut().merge(value);
                    }
                    Entry::Vacant(v) => {
                        v.insert(value);
                    }
                }
            }
        }

        batch.insert_batch(&self.coin, coin_index)?;
        batch.partial_merge_batch(&self.balance, balance_changes)?;
        batch.insert_batch(&self.package_version, package_version_index)?;

        Ok(())
    }

    fn get_epoch_info(&self, epoch: EpochId) -> Result<Option<EpochInfo>, TypedStoreError> {
        self.epochs.get(&epoch)
    }

    fn get_transaction_info(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionInfo>, TypedStoreError> {
        self.transactions.get(digest)
    }

    fn owner_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnerIndexKey>,
    ) -> Result<
        impl Iterator<Item = Result<(OwnerIndexKey, OwnerIndexInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        // TODO can we figure out how to pass a raw byte array as a cursor?
        let lower_bound = cursor.unwrap_or_else(|| OwnerIndexKey {
            owner,
            object_type: object_type
                .clone()
                .unwrap_or_else(|| "0x0::a::a".parse::<StructTag>().unwrap()),
            inverted_balance: None,
            object_id: ObjectID::ZERO,
        });

        Ok(self
            .owner
            .safe_iter_with_bounds(Some(lower_bound), None)
            .take_while(move |item| {
                // If there's an error let if flow through
                let Ok((key, _)) = item else {
                    return true;
                };

                // Only take if owner matches
                key.owner == owner
                    // and if an object type was supplied that the type matches
                    && object_type
                        .as_ref()
                        .map(|ty| {
                            ty.address == key.object_type.address
                                && ty.module == key.object_type.module
                                && ty.name == key.object_type.name
                                // If type_params are not provided then we match all params
                                && (ty.type_params.is_empty() ||
                                    // If they are provided the type params must match
                                    ty.type_params == key.object_type.type_params)
                        }).unwrap_or(true)
            }))
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = Result<DynamicFieldKey, TypedStoreError>> + '_, TypedStoreError>
    {
        let lower_bound = DynamicFieldKey::new(parent, cursor.unwrap_or(ObjectID::ZERO));
        let upper_bound = DynamicFieldKey::new(parent, ObjectID::MAX);
        let iter = self
            .dynamic_field
            .safe_iter_with_bounds(Some(lower_bound), Some(upper_bound))
            .map_ok(|(key, ())| key);
        Ok(iter)
    }

    fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> Result<Option<CoinIndexInfo>, TypedStoreError> {
        let key = CoinIndexKey {
            coin_type: coin_type.to_owned(),
        };
        self.coin.get(&key)
    }

    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> Result<Option<BalanceIndexInfo>, TypedStoreError> {
        let key = BalanceKey {
            owner: owner.to_owned(),
            coin_type: coin_type.to_owned(),
        };
        self.balance.get(&key)
    }

    fn balance_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<BalanceKey>,
    ) -> Result<
        impl Iterator<Item = Result<(BalanceKey, BalanceIndexInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        let lower_bound = cursor.unwrap_or_else(|| BalanceKey {
            owner,
            coin_type: "0x0::a::a".parse::<StructTag>().unwrap(),
        });

        Ok(self
            .balance
            .safe_iter_with_bounds(Some(lower_bound), None)
            .scan((), move |_, item| {
                match item {
                    Ok((key, value)) if key.owner == owner => Some(Ok((key, value))),
                    Ok(_) => None,          // Different owner, stop iteration
                    Err(e) => Some(Err(e)), // Propagate error
                }
            }))
    }

    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        cursor: Option<u64>,
    ) -> Result<
        impl Iterator<Item = Result<(PackageVersionKey, PackageVersionInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        let lower_bound = PackageVersionKey {
            original_package_id: original_id,
            version: cursor.unwrap_or(0),
        };
        let upper_bound = PackageVersionKey {
            original_package_id: original_id,
            version: u64::MAX,
        };

        Ok(self
            .package_version
            .safe_iter_with_bounds(Some(lower_bound), Some(upper_bound)))
    }
}

pub struct RpcIndexStore {
    tables: IndexStoreTables,
    pending_updates: Mutex<BTreeMap<u64, typed_store::rocks::DBBatch>>,
}

impl RpcIndexStore {
    /// Given the provided directory, construct the path to the db
    fn db_path(dir: &Path) -> PathBuf {
        dir.join("rpc-index")
    }

    pub async fn new(
        dir: &Path,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
        index_config: Option<&RpcIndexInitConfig>,
    ) -> Self {
        let path = Self::db_path(dir);

        let tables = {
            let tables = IndexStoreTables::open(&path);

            // If the index tables are uninitialized or on an older version then we need to
            // populate them
            if tables.needs_to_do_initialization(checkpoint_store) {
                let batch_size_limit;

                let mut tables = {
                    drop(tables);
                    typed_store::rocks::safe_drop_db(path.clone(), Duration::from_secs(30))
                        .await
                        .expect("unable to destroy old rpc-index db");

                    // Open the empty DB with `unordered_write`s enabled in order to get a ~3x
                    // speedup when indexing
                    let mut options = typed_store::rocksdb::Options::default();
                    options.set_unordered_write(true);

                    // Allow CPU-intensive flushing operations to use all CPUs.
                    let max_background_jobs = if let Some(jobs) =
                        index_config.as_ref().and_then(|c| c.max_background_jobs)
                    {
                        debug!("Using config override for max_background_jobs: {}", jobs);
                        jobs
                    } else {
                        let jobs = num_cpus::get() as i32;
                        debug!(
                            "Calculated max_background_jobs: {} (based on CPU count)",
                            jobs
                        );
                        jobs
                    };
                    options.set_max_background_jobs(max_background_jobs);

                    // We are disabling compaction for all column families below. This means we can
                    // also disable the backpressure that slows down writes when the number of L0
                    // files builds up since we will never compact them anyway.
                    options.set_level_zero_file_num_compaction_trigger(0);
                    options.set_level_zero_slowdown_writes_trigger(-1);
                    options.set_level_zero_stop_writes_trigger(i32::MAX);

                    let total_memory_bytes = get_available_memory();
                    // This is an upper bound on the amount to of ram the memtables can use across
                    // all column families.
                    let db_buffer_size = if let Some(size) =
                        index_config.as_ref().and_then(|c| c.db_write_buffer_size)
                    {
                        debug!(
                            "Using config override for db_write_buffer_size: {} bytes",
                            size
                        );
                        size
                    } else {
                        // Default to 80% of system RAM
                        let size = (total_memory_bytes as f64 * 0.8) as usize;
                        debug!(
                            "Calculated db_write_buffer_size: {} bytes (80% of {} total bytes)",
                            size, total_memory_bytes
                        );
                        size
                    };
                    options.set_db_write_buffer_size(db_buffer_size);

                    // Create column family specific options.
                    let mut table_config_map = BTreeMap::new();

                    // Create options with compactions disabled and large write buffers.
                    // Each CF can use up to 25% of system RAM, but total is still limited by
                    // set_db_write_buffer_size configured above.
                    let mut cf_options = typed_store::rocks::default_db_options();
                    cf_options.options.set_disable_auto_compactions(true);

                    let (buffer_size, buffer_count) = match (
                        index_config.as_ref().and_then(|c| c.cf_write_buffer_size),
                        index_config
                            .as_ref()
                            .and_then(|c| c.cf_max_write_buffer_number),
                    ) {
                        (Some(size), Some(count)) => {
                            debug!(
                                "Using config overrides - buffer_size: {} bytes, buffer_count: {}",
                                size, count
                            );
                            (size, count)
                        }
                        (None, None) => {
                            // Calculate buffer configuration: 25% of RAM split across buffers
                            let cf_memory_budget = (total_memory_bytes as f64 * 0.25) as usize;
                            debug!(
                                "Column family memory budget: {} bytes (25% of {} total bytes)",
                                cf_memory_budget, total_memory_bytes
                            );
                            const MIN_BUFFER_SIZE: usize = 64 * 1024 * 1024; // 64MB minimum

                            // Target number of buffers based on CPU count
                            // More CPUs = more parallel flushing capability
                            let target_buffer_count = num_cpus::get().max(2);

                            // Aim for CPU-based buffer count, but reduce if it would make buffers too small
                            //   For example:
                            // - 128GB RAM, 32 CPUs: 32GB per CF / 32 buffers = 1GB each
                            // - 16GB RAM, 8 CPUs: 4GB per CF / 8 buffers = 512MB each
                            // - 4GB RAM, 8 CPUs: 1GB per CF / 64MB min = ~16 buffers of 64MB each
                            let buffer_size =
                                (cf_memory_budget / target_buffer_count).max(MIN_BUFFER_SIZE);
                            let buffer_count = (cf_memory_budget / buffer_size)
                                .clamp(2, target_buffer_count)
                                as i32;
                            debug!("Calculated buffer_size: {} bytes, buffer_count: {} (based on {} CPUs)", 
                                buffer_size, buffer_count, target_buffer_count);
                            (buffer_size, buffer_count)
                        }
                        _ => {
                            panic!("indexing-cf-write-buffer-size and indexing-cf-max-write-buffer-number must both be specified or both be omitted");
                        }
                    };

                    cf_options.options.set_write_buffer_size(buffer_size);
                    cf_options.options.set_max_write_buffer_number(buffer_count);

                    // Calculate batch size limit: default to half the buffer size or 128MB, whichever is smaller
                    batch_size_limit = if let Some(limit) =
                        index_config.as_ref().and_then(|c| c.batch_size_limit)
                    {
                        debug!(
                            "Using config override for batch_size_limit: {} bytes",
                            limit
                        );
                        limit
                    } else {
                        let half_buffer = buffer_size / 2;
                        let default_limit = 1 << 27; // 128MB
                        let limit = half_buffer.min(default_limit);
                        debug!("Calculated batch_size_limit: {} bytes (min of half_buffer={} and default_limit={})", 
                            limit, half_buffer, default_limit);
                        limit
                    };

                    // Apply cf_options to all tables
                    for (table_name, _) in IndexStoreTables::describe_tables() {
                        table_config_map.insert(table_name, cf_options.clone());
                    }

                    // Override Balance options with the merge operator
                    let mut balance_options = cf_options.clone();
                    balance_options = balance_options.set_merge_operator_associative(
                        "balance_merge",
                        balance_delta_merge_operator,
                    );
                    table_config_map.insert("balance".to_string(), balance_options);

                    IndexStoreTables::open_with_options(
                        &path,
                        options,
                        Some(DBMapTableConfigMap::new(table_config_map)),
                    )
                };

                tables
                    .init(
                        authority_store,
                        checkpoint_store,
                        epoch_store,
                        package_store,
                        batch_size_limit,
                    )
                    .expect("unable to initialize rpc index from live object set");

                // Flush all data to disk before dropping tables.
                // This is critical because WAL is disabled during bulk indexing.
                // Note we only need to call flush on one table because all tables share the same
                // underlying database.
                tables
                    .meta
                    .flush()
                    .expect("Failed to flush RPC index tables to disk");

                let weak_db = Arc::downgrade(&tables.meta.db);
                drop(tables);

                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                loop {
                    if weak_db.strong_count() == 0 {
                        break;
                    }
                    if std::time::Instant::now() > deadline {
                        panic!("unable to reopen DB after indexing");
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }

                // Reopen the DB with default options (eg without `unordered_write`s enabled)
                let reopened_tables = IndexStoreTables::open(&path);

                // Sanity check: verify the database version was persisted correctly
                let stored_version = reopened_tables
                    .meta
                    .get(&())
                    .expect("Failed to read metadata from reopened database")
                    .expect("Metadata not found in reopened database");
                assert_eq!(
                    stored_version.version, CURRENT_DB_VERSION,
                    "Database version mismatch after flush and reopen: expected {}, found {}",
                    CURRENT_DB_VERSION, stored_version.version
                );

                reopened_tables
            } else {
                tables
            }
        };

        Self {
            tables,
            pending_updates: Default::default(),
        }
    }

    pub fn new_without_init(dir: &Path) -> Self {
        let path = Self::db_path(dir);
        let tables = IndexStoreTables::open(path);

        Self {
            tables,
            pending_updates: Default::default(),
        }
    }

    pub fn prune(
        &self,
        pruned_checkpoint_watermark: u64,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        self.tables
            .prune(pruned_checkpoint_watermark, checkpoint_contents_to_prune)
    }

    /// Index a checkpoint and stage the index updated in `pending_updates`.
    ///
    /// Updates will not be committed to the database until `commit_update_for_checkpoint` is
    /// called.
    #[tracing::instrument(
        skip_all,
        fields(checkpoint = checkpoint.checkpoint_summary.sequence_number)
    )]
    pub fn index_checkpoint(&self, checkpoint: &CheckpointData, resolver: &mut dyn LayoutResolver) {
        let sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let batch = self
            .tables
            .index_checkpoint(checkpoint, resolver)
            .expect("db error");

        self.pending_updates
            .lock()
            .unwrap()
            .insert(sequence_number, batch);
    }

    /// Commits the pending updates for the provided checkpoint number.
    ///
    /// Invariants:
    /// - `index_checkpoint` must have been called for the provided checkpoint
    /// - Callers of this function must ensure that it is called for each checkpoint in sequential
    ///   order. This will panic if the provided checkpoint does not match the expected next
    ///   checkpoint to commit.
    #[tracing::instrument(skip(self))]
    pub fn commit_update_for_checkpoint(&self, checkpoint: u64) -> Result<(), StorageError> {
        let next_batch = self.pending_updates.lock().unwrap().pop_first();

        // Its expected that the next batch exists
        let (next_sequence_number, batch) = next_batch.unwrap();
        assert_eq!(
            checkpoint, next_sequence_number,
            "commit_update_for_checkpoint must be called in order"
        );

        Ok(batch.write()?)
    }

    pub fn get_epoch_info(&self, epoch: EpochId) -> Result<Option<EpochInfo>, TypedStoreError> {
        self.tables.get_epoch_info(epoch)
    }

    pub fn get_transaction_info(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionInfo>, TypedStoreError> {
        self.tables.get_transaction_info(digest)
    }

    pub fn owner_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnerIndexKey>,
    ) -> Result<
        impl Iterator<Item = Result<(OwnerIndexKey, OwnerIndexInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        self.tables.owner_iter(owner, object_type, cursor)
    }

    pub fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = Result<DynamicFieldKey, TypedStoreError>> + '_, TypedStoreError>
    {
        self.tables.dynamic_field_iter(parent, cursor)
    }

    pub fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> Result<Option<CoinIndexInfo>, TypedStoreError> {
        self.tables.get_coin_info(coin_type)
    }

    pub fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> Result<Option<BalanceIndexInfo>, TypedStoreError> {
        self.tables.get_balance(owner, coin_type)
    }

    pub fn balance_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<BalanceKey>,
    ) -> Result<
        impl Iterator<Item = Result<(BalanceKey, BalanceIndexInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        self.tables.balance_iter(owner, cursor)
    }

    pub fn package_versions_iter(
        &self,
        original_id: ObjectID,
        cursor: Option<u64>,
    ) -> Result<
        impl Iterator<Item = Result<(PackageVersionKey, PackageVersionInfo), TypedStoreError>> + '_,
        TypedStoreError,
    > {
        self.tables.package_versions_iter(original_id, cursor)
    }
}

fn should_index_dynamic_field(object: &Object) -> bool {
    // Skip any objects that aren't of type `Field<Name, Value>`
    //
    // All dynamic fields are of type:
    //   - Field<Name, Value> for dynamic fields
    //   - Field<Wrapper<Name>, ID>> for dynamic field objects where the ID is the id of the pointed
    //   to object
    //
    object
        .data
        .try_as_move()
        .is_some_and(|move_object| move_object.type_().is_dynamic_field())
}

fn try_create_coin_index_info(object: &Object) -> Option<(CoinIndexKey, CoinIndexInfo)> {
    use sui_types::coin::CoinMetadata;
    use sui_types::coin::RegulatedCoinMetadata;
    use sui_types::coin::TreasuryCap;

    let object_type = object.type_().and_then(MoveObjectType::other)?;

    if let Some(coin_type) = CoinMetadata::is_coin_metadata_with_coin_type(object_type).cloned() {
        return Some((
            CoinIndexKey { coin_type },
            CoinIndexInfo {
                coin_metadata_object_id: Some(object.id()),
                treasury_object_id: None,
                regulated_coin_metadata_object_id: None,
            },
        ));
    }

    if let Some(coin_type) = TreasuryCap::is_treasury_with_coin_type(object_type).cloned() {
        return Some((
            CoinIndexKey { coin_type },
            CoinIndexInfo {
                coin_metadata_object_id: None,
                treasury_object_id: Some(object.id()),
                regulated_coin_metadata_object_id: None,
            },
        ));
    }

    if let Some(coin_type) =
        RegulatedCoinMetadata::is_regulated_coin_metadata_with_coin_type(object_type).cloned()
    {
        return Some((
            CoinIndexKey { coin_type },
            CoinIndexInfo {
                coin_metadata_object_id: None,
                treasury_object_id: None,
                regulated_coin_metadata_object_id: Some(object.id()),
            },
        ));
    }

    None
}

struct RpcParLiveObjectSetIndexer<'a> {
    tables: &'a IndexStoreTables,
    coin_index: &'a Mutex<HashMap<CoinIndexKey, CoinIndexInfo>>,
    batch_size_limit: usize,
}

struct RpcLiveObjectIndexer<'a> {
    tables: &'a IndexStoreTables,
    batch: typed_store::rocks::DBBatch,
    coin_index: &'a Mutex<HashMap<CoinIndexKey, CoinIndexInfo>>,
    balance_changes: HashMap<BalanceKey, BalanceIndexInfo>,
    batch_size_limit: usize,
}

impl<'a> ParMakeLiveObjectIndexer for RpcParLiveObjectSetIndexer<'a> {
    type ObjectIndexer = RpcLiveObjectIndexer<'a>;

    fn make_live_object_indexer(&self) -> Self::ObjectIndexer {
        RpcLiveObjectIndexer {
            tables: self.tables,
            batch: self.tables.owner.batch(),
            coin_index: self.coin_index,
            balance_changes: HashMap::new(),
            batch_size_limit: self.batch_size_limit,
        }
    }
}

impl LiveObjectIndexer for RpcLiveObjectIndexer<'_> {
    fn index_object(&mut self, object: Object) -> Result<(), StorageError> {
        match object.owner {
            // Owner Index
            Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                let owner_key = OwnerIndexKey::from_object(&object);
                let owner_info = OwnerIndexInfo::new(&object);
                self.batch
                    .insert_batch(&self.tables.owner, [(owner_key, owner_info)])?;

                if let Some((coin_type, value)) = get_balance_and_type_if_coin(&object)? {
                    let balance_key = BalanceKey { owner, coin_type };
                    let balance_info = BalanceIndexInfo::from(value);
                    self.balance_changes
                        .entry(balance_key)
                        .or_default()
                        .merge_delta(&balance_info);

                    if self.balance_changes.len() >= BALANCE_FLUSH_THRESHOLD {
                        self.batch.partial_merge_batch(
                            &self.tables.balance,
                            std::mem::take(&mut self.balance_changes),
                        )?;
                    }
                }
            }

            // Dynamic Field Index
            Owner::ObjectOwner(parent) => {
                if should_index_dynamic_field(&object) {
                    let field_key = DynamicFieldKey::new(parent, object.id());
                    self.batch
                        .insert_batch(&self.tables.dynamic_field, [(field_key, ())])?;
                }
            }

            Owner::Shared { .. } | Owner::Immutable => {}
        }

        // Look for CoinMetadata<T> and TreasuryCap<T> objects
        if let Some((key, value)) = try_create_coin_index_info(&object) {
            use std::collections::hash_map::Entry;

            match self.coin_index.lock().unwrap().entry(key) {
                Entry::Occupied(mut o) => {
                    o.get_mut().merge(value);
                }
                Entry::Vacant(v) => {
                    v.insert(value);
                }
            }
        }

        if let Some((key, info)) = IndexStoreTables::extract_version_if_package(&object) {
            self.batch
                .insert_batch(&self.tables.package_version, [(key, info)])?;
        }

        // If the batch size grows to greater than the limit then write out to the DB so that the
        // data we need to hold in memory doesn't grow unbounded.
        if self.batch.size_in_bytes() >= self.batch_size_limit {
            std::mem::replace(&mut self.batch, self.tables.owner.batch())
                .write_opt(&bulk_ingestion_write_options())?;
        }

        Ok(())
    }

    fn finish(mut self) -> Result<(), StorageError> {
        self.batch.partial_merge_batch(
            &self.tables.balance,
            std::mem::take(&mut self.balance_changes),
        )?;
        self.batch.write_opt(&bulk_ingestion_write_options())?;
        Ok(())
    }
}

// TODO figure out a way to dedup this logic. Today we'd need to do quite a bit of refactoring to
// make it possible.
//
// Load a CheckpointData struct without event data
fn sparse_checkpoint_data_for_backfill(
    authority_store: &AuthorityStore,
    checkpoint_store: &CheckpointStore,
    checkpoint: u64,
) -> Result<CheckpointData, StorageError> {
    use sui_types::full_checkpoint_content::CheckpointTransaction;

    let summary = checkpoint_store
        .get_checkpoint_by_sequence_number(checkpoint)?
        .ok_or_else(|| StorageError::missing(format!("missing checkpoint {checkpoint}")))?;
    let contents = checkpoint_store
        .get_checkpoint_contents(&summary.content_digest)?
        .ok_or_else(|| StorageError::missing(format!("missing checkpoint {checkpoint}")))?;

    let transaction_digests = contents
        .iter()
        .map(|execution_digests| execution_digests.transaction)
        .collect::<Vec<_>>();
    let transactions = authority_store
        .multi_get_transaction_blocks(&transaction_digests)?
        .into_iter()
        .map(|maybe_transaction| {
            maybe_transaction.ok_or_else(|| StorageError::custom("missing transaction"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let effects = authority_store
        .multi_get_executed_effects(&transaction_digests)?
        .into_iter()
        .map(|maybe_effects| maybe_effects.ok_or_else(|| StorageError::custom("missing effects")))
        .collect::<Result<Vec<_>, _>>()?;

    let mut full_transactions = Vec::with_capacity(transactions.len());
    for (tx, fx) in transactions.into_iter().zip(effects) {
        let input_objects =
            sui_types::storage::get_transaction_input_objects(authority_store, &fx)?;
        let output_objects =
            sui_types::storage::get_transaction_output_objects(authority_store, &fx)?;

        let full_transaction = CheckpointTransaction {
            transaction: tx.into(),
            effects: fx,
            events: None,
            input_objects,
            output_objects,
        };

        full_transactions.push(full_transaction);
    }

    let checkpoint_data = CheckpointData {
        checkpoint_summary: summary.into(),
        checkpoint_contents: contents,
        transactions: full_transactions,
    };

    Ok(checkpoint_data)
}

fn get_balance_and_type_if_coin(object: &Object) -> Result<Option<(StructTag, u64)>, StorageError> {
    match Coin::extract_balance_if_coin(object) {
        Ok(Some((TypeTag::Struct(struct_tag), value))) => Ok(Some((*struct_tag, value))),
        Ok(Some(_)) => {
            debug!("Coin object {} has non-struct type tag", object.id());
            Ok(None)
        }
        Ok(None) => {
            // Not a coin
            Ok(None)
        }
        Err(e) => {
            // Corrupted coin data
            Err(StorageError::custom(format!(
                "Failed to deserialize coin object {}: {}",
                object.id(),
                e
            )))
        }
    }
}
