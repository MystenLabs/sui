// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityStore;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::checkpoints::CheckpointStore;
use crate::par_index_live_object_set::LiveObjectIndexer;
use crate::par_index_live_object_set::ParMakeLiveObjectIndexer;
use itertools::Itertools;
use move_core_types::language_storage::{StructTag, TypeTag};
use mysten_common::ZipDebugEqIteratorExt;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use roaring::RoaringBitmap;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::for_each_event_dimension;
use sui_inverted_index::for_each_transaction_dimension;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::coin::Coin;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{AccumulatorValue, TransactionEffectsAPI};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Data;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::LedgerBitmapBucket;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::TransactionInfo;
use sui_types::storage::error::Error as StorageError;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::transaction::{TransactionDataAPI, TransactionKind};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use tracing::{debug, info, warn};
use typed_store::DBMapUtils;
use typed_store::TypedStoreError;
use typed_store::rocks::{DBMap, DBMapTableConfigMap, MetricConf};
use typed_store::rocksdb::{MergeOperands, WriteOptions, compaction_filter::Decision};
use typed_store::traits::Map;

const CURRENT_DB_VERSION: u64 = 4;

// I tried increasing this to 100k and 1M and it didn't speed up indexing at all.
const BALANCE_FLUSH_THRESHOLD: usize = 10_000;

// Bitmap inverted index constants
// A change to these constants requires bumping CURRENT_DB_VERSION
const TX_BUCKET_SIZE: u64 = 65_536;
// 2^28: 4,096 transactions per bucket. Must match sui-kvstore's
// `event_bitmap_index::BUCKET_SIZE` and the reader's `EVENT_BITMAP_BUCKET_SIZE`.
const EVENT_BUCKET_SIZE: u64 = 268_435_456;
const EVENT_BITS: u32 = 16;
const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;
const MAX_TX_SEQ: u64 = u64::MAX >> EVENT_BITS;

const _: () = assert!(TX_BUCKET_SIZE <= u32::MAX as u64);
const _: () = assert!(EVENT_BUCKET_SIZE <= u32::MAX as u64);
const _: () = assert!(EVENT_BITS < u32::BITS);
const _: () = assert!(EVENT_BITS < u64::BITS);
const _: () = assert!(MAX_EVENTS_PER_TX as u64 == 1u64 << EVENT_BITS);
const _: () = assert!(EVENT_BUCKET_SIZE.is_multiple_of(MAX_EVENTS_PER_TX as u64));

fn checked_encode_event_seq(tx_seq: u64, event_idx: u32) -> Result<u64, StorageError> {
    if event_idx >= MAX_EVENTS_PER_TX {
        return Err(StorageError::custom(format!(
            "event_idx {event_idx} exceeds packed event-seq limit {}",
            MAX_EVENTS_PER_TX - 1
        )));
    }
    if tx_seq > MAX_TX_SEQ {
        return Err(StorageError::custom(format!(
            "tx_seq {tx_seq} exceeds packed event-seq limit {MAX_TX_SEQ}"
        )));
    }
    Ok((tx_seq << EVENT_BITS) | (event_idx as u64))
}

/// Lowest packed event_seq for a given `tx_seq` (idx 0), as an `Option` so
/// the compaction filter (and other untrusted-input callers) get a clean
/// `None` instead of an overflowing shift when `tx_seq > MAX_TX_SEQ`.
fn checked_event_seq_lo(tx_seq: u64) -> Option<u64> {
    if tx_seq <= MAX_TX_SEQ {
        Some(tx_seq << EVENT_BITS)
    } else {
        None
    }
}

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

/// Per-DB rpc-index settings, persisted in their own column family so the
/// schema `version` stays a clean monotonic number rather than carrying packed
/// feature bits.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
struct IndexSettings {
    /// Whether this DB was built with ledger-history indexing enabled.
    ledger_history_indexing: bool,
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
    pub coin_balance_delta: i128,
    pub address_balance_delta: i128,
}

impl BalanceIndexInfo {
    fn merge_delta(&mut self, other: &Self) {
        self.coin_balance_delta = self
            .coin_balance_delta
            .saturating_add(other.coin_balance_delta);
        self.address_balance_delta = self
            .address_balance_delta
            .saturating_add(other.address_balance_delta);
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
        let coin_balance = index_info.coin_balance_delta.clamp(0, u64::MAX as i128) as u64;
        let address_balance = index_info.address_balance_delta.clamp(0, u64::MAX as i128) as u64;
        sui_types::storage::BalanceInfo {
            coin_balance,
            address_balance,
        }
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

/// Row of the `tx_seq_digest` table — direct mapping from `tx_sequence_number`
/// to `(digest, event_count, checkpoint_number)`. `event_count` lets event
/// listings enumerate a transaction's event_seqs without rereading the tx row.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct TxSeqDigestInfo {
    pub digest: TransactionDigest,
    pub event_count: u32,
    pub checkpoint_number: CheckpointSequenceNumber,
}

/// Row key for both bitmap CFs.
///
/// `dimension_key` is `[tag_byte][value_bytes]` per `sui-inverted-index`.
/// `bucket_id` is the integer division `seq / BUCKET_SIZE` for whichever
/// sequence space the CF is keyed by (tx_seq for `transaction_bitmap`, packed
/// event_seq for `event_bitmap`).
///
/// typed-store encodes keys with bincode `with_big_endian().with_fixint_encoding()`,
/// so the on-disk layout is:
///   [8 B BE length(dimension_key)] [dimension_key bytes] [8 B BE bucket_id]
/// Within a fixed `dimension_key`, range scans over `bucket_id` are
/// numerically ordered. The compaction filter recovers `bucket_id` from the
/// trailing 8 bytes.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BitmapIndexKey {
    pub dimension_key: Vec<u8>,
    pub bucket_id: u64,
}

/// Value stored in the bitmap CFs: the raw bytes of `RoaringBitmap::serialize_into`.
///
/// typed-store BCS-wraps this on disk (ULEB128 length prefix + raw bitmap
/// bytes). The merge operator decodes operands, ORs them, and re-encodes —
/// see `bitmap_union_merge_operator`.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct BitmapBlob(pub Vec<u8>);

impl From<RoaringBitmap> for BitmapBlob {
    fn from(bm: RoaringBitmap) -> Self {
        let mut buf = Vec::with_capacity(bm.serialized_size());
        bm.serialize_into(&mut buf)
            .expect("RoaringBitmap::serialize_into on Vec cannot fail");
        Self(buf)
    }
}

fn ledger_tx_seq_digest(tx_sequence_number: u64, info: TxSeqDigestInfo) -> LedgerTxSeqDigest {
    LedgerTxSeqDigest {
        tx_sequence_number,
        digest: info.digest,
        event_count: info.event_count,
        checkpoint_number: info.checkpoint_number,
    }
}

fn decode_ledger_bitmap_bucket(
    key: BitmapIndexKey,
    blob: BitmapBlob,
) -> Result<LedgerBitmapBucket, TypedStoreError> {
    let bitmap = RoaringBitmap::deserialize_from(&blob.0[..]).map_err(|e| {
        TypedStoreError::SerializationError(format!("decode ledger bitmap bucket: {e}"))
    })?;
    Ok(LedgerBitmapBucket {
        bucket_id: key.bucket_id,
        bitmap,
    })
}

/// Which sequence space a bitmap CF is keyed by. Owns the whole-bucket
/// removability math the `BitmapCompactionFilter` applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapKind {
    Transaction,
    Event,
}

impl BitmapKind {
    /// Returns true when every seq in `bucket_id` is strictly below
    /// `pruned_tx_seq_exclusive` — i.e. the whole bucket row is safe to drop.
    ///
    /// Both kinds bucket by integer division, but in different sequence
    /// spaces, while the prune watermark is always in tx-seq space:
    /// `Transaction` buckets are tx-seq ranges, so the watermark compares
    /// directly; `Event` buckets are packed-event-seq ranges, so the watermark
    /// is first converted to its lowest event-seq (`checked_event_seq_lo`).
    ///
    /// All arithmetic is `checked_*`: `bucket_id` is untrusted input decoded
    /// from a rocksdb key, and an overflow must not panic the compaction
    /// thread — it conservatively returns `false` (keep) instead.
    fn bucket_fully_pruned(self, bucket_id: u64, pruned_tx_seq_exclusive: u64) -> bool {
        match self {
            BitmapKind::Transaction => bucket_id
                .checked_add(1)
                .and_then(|b| b.checked_mul(TX_BUCKET_SIZE))
                .map(|hi| hi <= pruned_tx_seq_exclusive)
                .unwrap_or(false),
            BitmapKind::Event => {
                let bucket_hi = bucket_id
                    .checked_add(1)
                    .and_then(|b| b.checked_mul(EVENT_BUCKET_SIZE));
                let threshold = checked_event_seq_lo(pruned_tx_seq_exclusive);
                match (bucket_hi, threshold) {
                    (Some(hi), Some(th)) => hi <= th,
                    _ => false,
                }
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct IndexStoreOptions {
    pub events_compaction_filter: Option<EventsCompactionFilter>,
    /// Shared exclusive tx-seq prune floor for compaction filters.
    /// A zero floor keeps every bucket.
    pub pruning_tx_seq_exclusive: Arc<AtomicU64>,
}

fn default_table_options() -> typed_store::rocks::DBOptions {
    typed_store::rocks::default_db_options().disable_write_throttling()
}

/// Like `default_table_options`, but honors range-delete tombstones immediately
/// instead of ignoring them until compaction (the rocksdb default). `tx_seq_digest`
/// is pruned with `schedule_delete_range` and `first_tx_seq_digest_key` reads the
/// resulting pruning floor straight back, so tombstones must be visible to reads at
/// once or the floor would not advance until a compaction happened to run.
fn tx_seq_digest_table_options() -> typed_store::rocks::DBOptions {
    let mut options = default_table_options();
    options.rw_options = options.rw_options.clone().set_ignore_range_deletions(false);
    options
}

fn events_table_options(
    compaction_filter: Option<EventsCompactionFilter>,
) -> typed_store::rocks::DBOptions {
    let mut options = default_table_options();
    if let Some(filter) = compaction_filter {
        options.options.set_compaction_filter(
            "events_by_stream",
            move |_, key, value| match filter.filter(key, value) {
                Ok(decision) => decision,
                Err(e) => {
                    warn!(
                        "Failed to parse event key during compaction: {}, key: {:?}",
                        e, key
                    );
                    Decision::Remove
                }
            },
        );
    }
    options
}

fn balance_delta_merge_operator(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let mut result = if let Some(existing_val) = existing_val {
        bcs::from_bytes::<BalanceIndexInfo>(existing_val)
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to deserialize BalanceIndexInfo from RocksDB - data corruption: {e}"
                )
            })
            .ok()?
    } else {
        BalanceIndexInfo::default()
    };

    for operand in operands.iter() {
        let delta = bcs::from_bytes::<BalanceIndexInfo>(operand)
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to deserialize BalanceIndexInfo from RocksDB - data corruption: {e}"
                )
            })
            .ok()?;
        result.merge_delta(&delta);
    }

    Some(
        bcs::to_bytes(&result)
            .expect("Failed to deserialize BalanceIndexInfo from RocksDB - data corruption."),
    )
}

fn balance_compaction_filter(_level: u32, _key: &[u8], value: &[u8]) -> Decision {
    let balance_info = match bcs::from_bytes::<BalanceIndexInfo>(value) {
        Ok(info) => info,
        Err(_) => return Decision::Keep,
    };

    if balance_info.coin_balance_delta == 0 && balance_info.address_balance_delta == 0 {
        Decision::Remove
    } else {
        Decision::Keep
    }
}

fn balance_table_options() -> typed_store::rocks::DBOptions {
    default_table_options()
        .set_merge_operator_associative("balance_merge", balance_delta_merge_operator)
        .set_compaction_filter("balance_zero_filter", balance_compaction_filter)
}

// Bitmap inverted index: merge operator, compaction filter, options.

fn decode_bitmap_blob(bcs_bytes: &[u8]) -> Result<RoaringBitmap, anyhow::Error> {
    let blob: BitmapBlob = bcs::from_bytes(bcs_bytes)?;
    Ok(RoaringBitmap::deserialize_from(&blob.0[..])?)
}

fn encode_bitmap_blob(bm: &RoaringBitmap) -> Vec<u8> {
    let mut buf = Vec::with_capacity(bm.serialized_size());
    bm.serialize_into(&mut buf)
        .expect("RoaringBitmap::serialize_into on Vec cannot fail");
    bcs::to_bytes(&BitmapBlob(buf)).expect("BCS encode of BitmapBlob cannot fail")
}

/// RocksDB merge operator for both bitmap CFs. ORs all operands (and any
/// existing on-disk value) into a single bitmap.
fn bitmap_union_merge_operator(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let mut acc = match existing_val {
        Some(v) => match decode_bitmap_blob(v) {
            Ok(bm) => bm,
            Err(e) => {
                tracing::error!(
                    "Failed to deserialize existing BitmapBlob during merge - data corruption: {e}"
                );
                return None;
            }
        },
        None => RoaringBitmap::new(),
    };

    for operand in operands.iter() {
        match decode_bitmap_blob(operand) {
            Ok(bm) => acc |= bm,
            Err(e) => {
                tracing::error!(
                    "Failed to deserialize BitmapBlob operand during merge - data corruption: {e}"
                );
                return None;
            }
        }
    }

    // Convert dense containers to run containers before serializing the
    // accumulated bitmap. This is the on-disk representation (the merge
    // operator's output is what RocksDB stores), and a bucket that matches
    // many consecutive tx_seqs compresses substantially as runs. Mirrors the
    // BigTable bitmap committer, which optimizes each row before writing.
    // Operands are not optimized — they carry a bit or a handful, so there is
    // nothing for run-encoding to collapse.
    acc.optimize();
    Some(encode_bitmap_blob(&acc))
}

/// Whole-bucket compaction filter for bitmap CFs. Reads the trailing 8 bytes
/// of a typed-store key as `bucket_id` (bincode big-endian fixed-int), then
/// removes the row iff the bucket is entirely below the current
/// `tx_seq_pruning_watermark` exclusive value.
///
/// Never `Remove` on a parse failure: silent data loss is worse than a stuck
/// row. The bucket math is `checked_*` because `bucket_id` is untrusted input
/// from rocksdb — a corrupted key shouldn't be able to panic the compaction
/// thread.
#[derive(Clone)]
pub struct BitmapCompactionFilter {
    pruning_tx_seq_exclusive: Arc<AtomicU64>,
    kind: BitmapKind,
}

impl BitmapCompactionFilter {
    pub fn new(pruning_tx_seq_exclusive: Arc<AtomicU64>, kind: BitmapKind) -> Self {
        Self {
            pruning_tx_seq_exclusive,
            kind,
        }
    }

    pub fn filter(&self, key: &[u8], _value: &[u8]) -> Decision {
        if key.len() < 8 {
            warn!(
                kind = ?self.kind,
                "bitmap compaction filter saw key shorter than 8 bytes ({}); keeping",
                key.len(),
            );
            return Decision::Keep;
        }
        let bucket_id =
            u64::from_be_bytes(key[key.len() - 8..].try_into().expect("len checked above"));
        let pruned_exclusive = self.pruning_tx_seq_exclusive.load(Ordering::Relaxed);

        if self.kind.bucket_fully_pruned(bucket_id, pruned_exclusive) {
            Decision::Remove
        } else {
            Decision::Keep
        }
    }
}

/// Default bitmap CF options. The merge operator must be present on every open;
/// `bitmap_cf_options` adds the runtime compaction filter.
fn bitmap_cf_default_options() -> typed_store::rocks::DBOptions {
    default_table_options()
        .set_merge_operator_associative("bitmap_union_merge", bitmap_union_merge_operator)
}

/// Bitmap CF options with the per-CF compaction filter attached.
fn bitmap_cf_options(
    filter_name: &str,
    filter: BitmapCompactionFilter,
) -> typed_store::rocks::DBOptions {
    let mut options = bitmap_cf_default_options();
    options
        .options
        .set_compaction_filter(filter_name, move |_level, key, value| {
            filter.filter(key, value)
        });
    options
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
    ///   initialization)
    /// - version of the DB. Everytime a new table or schema is changed the version number needs to
    ///   be incremented.
    meta: DBMap<(), MetadataInfo>,

    /// A singleton recording which optional features this DB was built with
    /// (currently just ledger-history indexing). Kept separate from `meta` so
    /// the schema `version` need not encode feature flags. Auto-created on
    /// older DBs; a missing entry reads as `IndexSettings::default()` (all
    /// features off).
    settings: DBMap<(), IndexSettings>,

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
    #[allow(unused)]
    #[deprecated]
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

    /// Authenticated events index by (stream_id, checkpoint_seq, transaction_idx, event_index)
    events_by_stream: DBMap<EventIndexKey, ()>,

    /// `tx_sequence_number` → (digest, event_count, checkpoint_number).
    #[default_options_override_fn = "tx_seq_digest_table_options"]
    tx_seq_digest: DBMap<u64, TxSeqDigestInfo>,

    /// Transaction bitmap index keyed by `(dimension_key, tx_seq bucket)`.
    #[default_options_override_fn = "bitmap_cf_default_options"]
    transaction_bitmap: DBMap<BitmapIndexKey, BitmapBlob>,

    /// Event bitmap index keyed by `(dimension_key, packed event_seq bucket)`.
    #[default_options_override_fn = "bitmap_cf_default_options"]
    event_bitmap: DBMap<BitmapIndexKey, BitmapBlob>,
    // NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
    // - bounded in size by the live object set
    // - are prune-able and have corresponding logic in the `prune` function
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventIndexKey {
    pub stream_id: SuiAddress,
    pub checkpoint_seq: u64,
    /// The accumulator version that this event is settled into
    pub accumulator_version: u64,
    pub transaction_idx: u32,
    pub event_index: u32,
}

/// Compaction filter for automatic pruning of old authenticated events during RocksDB compaction.
#[derive(Clone)]
pub struct EventsCompactionFilter {
    pruning_watermark: Arc<std::sync::atomic::AtomicU64>,
}

impl EventsCompactionFilter {
    pub fn new(pruning_watermark: Arc<std::sync::atomic::AtomicU64>) -> Self {
        Self { pruning_watermark }
    }

    pub fn filter(&self, key: &[u8], _value: &[u8]) -> anyhow::Result<Decision> {
        let event_key: EventIndexKey = bcs::from_bytes(key)?;
        let watermark = self
            .pruning_watermark
            .load(std::sync::atomic::Ordering::Relaxed);

        if event_key.checkpoint_seq <= watermark {
            Ok(Decision::Remove)
        } else {
            Ok(Decision::Keep)
        }
    }
}

/// Completely empty a column family and physically reclaim its disk. A single
/// range tombstone over `[first, last)` (end-exclusive, so paired with a point
/// delete of `last`) drops every row in O(1). The range is then compacted so
/// the data is physically removed: this reclaims the space immediately rather
/// than waiting for a background compaction, and makes the rows invisible to
/// reads regardless of the CF's `ignore_range_deletions` setting.
fn clear_table<K, V>(map: &DBMap<K, V>) -> Result<(), TypedStoreError>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    let first = map.safe_iter().next().transpose()?.map(|(k, _)| k);
    let last = map
        .reversed_safe_iter_with_bounds(None, None)?
        .next()
        .transpose()?
        .map(|(k, _)| k);
    let (Some(first), Some(last)) = (first, last) else {
        return Ok(());
    };
    let mut batch = map.batch();
    batch.schedule_delete_range(map, &first, &last)?;
    batch.delete_batch(map, std::iter::once(&last))?;
    batch.write()?;
    map.compact_range(&first, &last)
}

impl IndexStoreTables {
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

    fn open_with_index_options<P: Into<PathBuf>>(
        path: P,
        index_options: IndexStoreOptions,
    ) -> Self {
        // The typed-store derive macro only honors the per-field
        // `default_options_override_fn` when `tables_db_options_override` is
        // `None`. As soon as we pass `Some(map)`, any CF missing from the map
        // silently falls back to bare `default_db_options()` — losing the
        // `disable_write_throttling` applied by `default_table_options`. To
        // avoid that, populate the map with `default_table_options()` for every
        // table before overriding the few that need bespoke configuration.
        let mut table_options = std::collections::BTreeMap::new();
        for (table_name, _) in IndexStoreTables::describe_tables() {
            table_options.insert(table_name, default_table_options());
        }
        table_options.insert("balance".to_string(), balance_table_options());
        // Range-delete pruning needs tombstones honored by reads immediately.
        table_options.insert("tx_seq_digest".to_string(), tx_seq_digest_table_options());
        table_options.insert(
            "events_by_stream".to_string(),
            events_table_options(index_options.events_compaction_filter),
        );

        let bitmap_filter_tx = BitmapCompactionFilter::new(
            index_options.pruning_tx_seq_exclusive.clone(),
            BitmapKind::Transaction,
        );
        let bitmap_filter_event = BitmapCompactionFilter::new(
            index_options.pruning_tx_seq_exclusive.clone(),
            BitmapKind::Event,
        );
        table_options.insert(
            "transaction_bitmap".to_string(),
            bitmap_cf_options("transaction_bitmap_filter", bitmap_filter_tx),
        );
        table_options.insert(
            "event_bitmap".to_string(),
            bitmap_cf_options("event_bitmap_filter", bitmap_filter_event),
        );

        IndexStoreTables::open_tables_read_write_with_deprecation_option(
            path.into(),
            MetricConf::new("rpc-index"),
            None,
            Some(DBMapTableConfigMap::new(table_options)),
            true, // remove deprecated tables
        )
    }

    fn open_with_options<P: Into<PathBuf>>(
        path: P,
        options: typed_store::rocksdb::Options,
        table_options: Option<DBMapTableConfigMap>,
    ) -> Self {
        IndexStoreTables::open_tables_read_write_with_deprecation_option(
            path.into(),
            MetricConf::new("rpc-index"),
            Some(options),
            table_options,
            true, // remove deprecated tables
        )
    }

    /// Whether the ledger-history feature was enabled when this DB was built,
    /// as persisted in the `settings` CF. A missing entry (fresh or pre-feature
    /// DB) reads as `false`.
    fn persisted_ledger_history_indexing(&self) -> bool {
        self.settings
            .get(&())
            .ok()
            .flatten()
            .map(|s| s.ledger_history_indexing)
            .unwrap_or(false)
    }

    fn needs_to_do_initialization(
        &self,
        checkpoint_store: &CheckpointStore,
        ledger_history_indexing: bool,
    ) -> bool {
        let schema_stale = match self.meta.get(&()) {
            Ok(Some(metadata)) => metadata.version != CURRENT_DB_VERSION,
            Ok(None) | Err(_) => true,
        };
        // *Enabling* ledger history requires a full rebuild to backfill the
        // historical rows. *Disabling* it does not — the base indexes stay
        // valid, so the now-unused history CFs are dropped in place by
        // `disable_ledger_history_indexing` instead.
        let enabling = ledger_history_indexing && !self.persisted_ledger_history_indexing();
        schema_stale || enabling || self.is_indexed_watermark_out_of_date(checkpoint_store)
    }

    /// Drop the ledger-history column families and clear the persisted feature
    /// flag without rebuilding the rest of the index. Used when a node that had
    /// ledger history enabled restarts with it disabled: the base indexes are
    /// untouched, so only the now-unused history rows need to go. Idempotent —
    /// it runs at open before the store goes live, so a crash mid-way just
    /// re-runs it on the next start.
    fn disable_ledger_history_indexing(&self) -> Result<(), TypedStoreError> {
        clear_table(&self.tx_seq_digest)?;
        clear_table(&self.transaction_bitmap)?;
        clear_table(&self.event_bitmap)?;
        self.settings.insert(
            &(),
            &IndexSettings {
                ledger_history_indexing: false,
            },
        )?;
        Ok(())
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
        rpc_config: &sui_config::RpcConfig,
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

        if let Some(checkpoint_range) = checkpoint_range.clone() {
            self.index_existing_checkpoints(
                authority_store,
                checkpoint_store,
                checkpoint_range,
                rpc_config,
            )?;
        }

        if rpc_config.ledger_history_indexing()
            && let Some(checkpoint_range) = checkpoint_range
        {
            self.backfill_ledger_history_indexes(
                authority_store,
                checkpoint_store,
                checkpoint_range,
            )?;
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

        // Write the schema version and the feature settings in one batch so the
        // two can never be persisted independently.
        let mut batch = self.meta.batch();
        batch.insert_batch(
            &self.meta,
            [(
                (),
                MetadataInfo {
                    version: CURRENT_DB_VERSION,
                },
            )],
        )?;
        batch.insert_batch(
            &self.settings,
            [(
                (),
                IndexSettings {
                    ledger_history_indexing: rpc_config.ledger_history_indexing(),
                },
            )],
        )?;
        batch.write()?;

        info!("Finished initializing RPC indexes");

        Ok(())
    }

    #[tracing::instrument(skip(self, authority_store, checkpoint_store, rpc_config))]
    fn index_existing_checkpoints(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        checkpoint_range: std::ops::RangeInclusive<u64>,
        rpc_config: &sui_config::RpcConfig,
    ) -> Result<(), StorageError> {
        info!(
            "Indexing {} checkpoints in range {checkpoint_range:?}",
            checkpoint_range.size_hint().0
        );
        let start_time = Instant::now();

        checkpoint_range.into_par_iter().try_for_each(|seq| {
            let load_events = rpc_config.authenticated_events_indexing();
            let Some(checkpoint_data) = sparse_checkpoint_data_for_epoch_backfill(
                authority_store,
                checkpoint_store,
                seq,
                load_events,
            )?
            else {
                return Ok(());
            };

            let mut batch = self.epochs.batch();

            self.index_epoch(&checkpoint_data, &mut batch)?;

            batch
                .write_opt(bulk_ingestion_write_options())
                .map_err(StorageError::from)
        })?;

        info!(
            "Indexing checkpoints took {} seconds",
            start_time.elapsed().as_secs()
        );
        Ok(())
    }

    /// Backfill ledger history rows over a freshly recreated rpc-index DB.
    ///
    /// Bulk writes disable WAL, so this flushes before `init()` writes the
    /// `meta.version` / `settings` markers; otherwise a crash could persist
    /// those markers without the rows they claim to cover.
    fn backfill_ledger_history_indexes(
        &self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        checkpoint_range: std::ops::RangeInclusive<u64>,
    ) -> Result<(), StorageError> {
        info!("ledger history backfill: cps {checkpoint_range:?}");
        let start_time = Instant::now();

        checkpoint_range.clone().into_par_iter().try_for_each(
            |seq| -> Result<(), StorageError> {
                let cp_data = full_checkpoint_data_for_backfill(
                    authority_store,
                    checkpoint_store,
                    seq,
                )?
                .ok_or_else(|| {
                    // Missing retained data would leave a permanent hole.
                    StorageError::missing(format!(
                        "ledger history backfill: checkpoint {seq} is missing from local storage \
                         but falls inside the retained backfill range {checkpoint_range:?}"
                    ))
                })?;
                let mut batch = self.meta.batch();
                self.write_ledger_history_rows_for_checkpoint(&cp_data, &mut batch)?;
                batch
                    .write_opt(bulk_ingestion_write_options())
                    .map_err(StorageError::from)
            },
        )?;

        // Flushing one CF flushes the whole shared RocksDB instance.
        self.tx_seq_digest.flush().map_err(|e| {
            StorageError::custom(format!("flush after ledger history backfill: {e}"))
        })?;

        info!(
            "ledger history backfill took {} seconds",
            start_time.elapsed().as_secs()
        );
        Ok(())
    }

    /// The lowest live key of `tx_seq_digest`: the ledger history pruning
    /// floor in tx-seq space. `prune()` maintains the invariant that this
    /// equals the highest fully-pruned tx_seq (exclusive): pruning
    /// point-deletes `tx_seq_digest` rows below the floor, and forward
    /// indexing only adds rows above it. Returns `None` when the CF is empty
    /// (nothing indexed yet), which callers treat as floor 0.
    fn first_tx_seq_digest_key(&self) -> Result<Option<u64>, TypedStoreError> {
        match self.tx_seq_digest.safe_iter().next() {
            Some(Ok((k, _))) => Ok(Some(k)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Prune data from this Index. `pruned_tx_seq_exclusive` is the
    /// absolute tx-seq floor after this prune — the caller derives it from
    /// the last-pruned checkpoint's `network_total_transactions`. Returns
    /// that floor iff ledger history maintenance ran and advanced it, so the caller
    /// can update the compaction-filter atomic after the batch commits and
    /// the atomic never leads disk.
    fn prune(
        &self,
        pruned_checkpoint_watermark: u64,
        pruned_tx_seq_exclusive: u64,
        ledger_history_enabled: bool,
    ) -> Result<Option<u64>, TypedStoreError> {
        let mut batch = self.watermark.batch();

        batch.insert_batch(
            &self.watermark,
            [(Watermark::Pruned, pruned_checkpoint_watermark)],
        )?;

        // When enabled, `tx_seq_digest` and bitmap CFs share this tx-seq floor.
        if ledger_history_enabled {
            // The previous floor is the first live `tx_seq_digest` key,
            // not a stored watermark: pruning and forward indexing keep
            // the first key equal to the highest fully-pruned tx_seq
            // (exclusive). An empty CF means floor 0.
            let prev_exclusive = self.first_tx_seq_digest_key()?.unwrap_or(0);

            // The range delete and `Watermark::Pruned` ride the same batch, so the
            // prune commits atomically (all-or-nothing). Recovery self-heals:
            // the next prune re-derives `prev_exclusive` from the first live key,
            // whether or not this batch committed. `pruned_tx_seq_exclusive ==
            // prev_exclusive` when a crashed prune is replayed — a no-op, not an
            // error. The CF is opened with `ignore_range_deletions = false`, so the
            // tombstone is visible to `first_tx_seq_digest_key` immediately.
            if pruned_tx_seq_exclusive > prev_exclusive {
                batch.schedule_delete_range(
                    &self.tx_seq_digest,
                    &prev_exclusive,
                    &pruned_tx_seq_exclusive,
                )?;
                batch.write()?;
                return Ok(Some(pruned_tx_seq_exclusive));
            }
            batch.write()?;
            return Ok(None);
        }

        batch.write()?;
        Ok(None)
    }

    /// Index a Checkpoint
    fn index_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        _resolver: &mut dyn LayoutResolver,
        rpc_config: &sui_config::RpcConfig,
        ledger_history_enabled: bool,
    ) -> Result<typed_store::rocks::DBBatch, StorageError> {
        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "indexing checkpoint"
        );

        let mut batch = self.owner.batch();

        self.index_epoch(checkpoint, &mut batch)?;
        self.index_transactions(
            checkpoint,
            &mut batch,
            rpc_config.authenticated_events_indexing(),
        )?;
        self.index_objects(checkpoint, &mut batch)?;

        // Ledger history rows ride the same batch as `Watermark::Indexed`.
        if ledger_history_enabled {
            self.write_ledger_history_rows_for_checkpoint(checkpoint, &mut batch)?;
        }

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

    fn extract_accumulator_version(&self, tx: &CheckpointTransaction) -> Option<u64> {
        let TransactionKind::ProgrammableSystemTransaction(pt) =
            tx.transaction.transaction_data().kind()
        else {
            return None;
        };

        if pt.shared_input_objects().any(|obj| {
            obj.id == SUI_ACCUMULATOR_ROOT_OBJECT_ID
                && obj.mutability == sui_types::transaction::SharedObjectMutability::Mutable
        }) {
            return tx.output_objects.iter().find_map(|obj| {
                if obj.id() == SUI_ACCUMULATOR_ROOT_OBJECT_ID {
                    Some(obj.version().value())
                } else {
                    None
                }
            });
        }

        None
    }

    fn index_transaction_events(
        &self,
        tx: &CheckpointTransaction,
        checkpoint_seq: u64,
        tx_idx: u32,
        accumulator_version: Option<u64>,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        let acc_events = tx.effects.accumulator_events();
        if acc_events.is_empty() {
            return Ok(());
        }

        let mut entries: Vec<(EventIndexKey, ())> = Vec::new();
        for acc in acc_events {
            if let AccumulatorValue::EventDigest(event_digests) = &acc.write.value {
                let Some(accumulator_version) = accumulator_version else {
                    mysten_common::debug_fatal!(
                        "Found events at checkpoint {} tx {} before any accumulator settlement",
                        checkpoint_seq,
                        tx_idx
                    );
                    continue;
                };

                if let Some(stream_id) =
                    sui_types::accumulator_root::stream_id_from_accumulator_event(&acc)
                {
                    for (idx, _d) in event_digests {
                        let key = EventIndexKey {
                            stream_id,
                            checkpoint_seq,
                            accumulator_version,
                            transaction_idx: tx_idx,
                            event_index: *idx as u32,
                        };
                        entries.push((key, ()));
                    }
                }
            }
        }

        if !entries.is_empty() {
            batch.insert_batch(&self.events_by_stream, entries)?;
        }
        Ok(())
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
        index_events: bool,
    ) -> Result<(), StorageError> {
        let cp = checkpoint.checkpoint_summary.sequence_number;
        let mut current_accumulator_version: Option<u64> = None;

        // iterate in reverse order, process accumulator settlements first
        for (tx_idx, tx) in checkpoint.transactions.iter().enumerate().rev() {
            let balance_changes = sui_types::balance_change::derive_detailed_balance_changes(
                &tx.effects,
                &tx.input_objects,
                &tx.output_objects,
            )
            .into_iter()
            .filter_map(|change| {
                if let TypeTag::Struct(coin_type) = change.coin_type {
                    Some((
                        BalanceKey {
                            owner: change.address,
                            coin_type: *coin_type,
                        },
                        BalanceIndexInfo {
                            coin_balance_delta: change.coin_amount,
                            address_balance_delta: change.address_amount,
                        },
                    ))
                } else {
                    None
                }
            });
            batch.partial_merge_batch(&self.balance, balance_changes)?;

            if index_events {
                if let Some(version) = self.extract_accumulator_version(tx) {
                    current_accumulator_version = Some(version);
                }

                self.index_transaction_events(
                    tx,
                    cp,
                    tx_idx as u32,
                    current_accumulator_version,
                    batch,
                )?;
            }
        }

        Ok(())
    }

    /// Emit `tx_seq_digest` rows and bitmap merge operands for every tx in
    /// `checkpoint`. Shared by forward indexing (`index_checkpoint`) and the
    /// rebuild-time ledger history backfill. There is no separate watermark:
    /// `Watermark::Indexed` is the source of truth for coverage.
    fn write_ledger_history_rows_for_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        let cp_seq = checkpoint.checkpoint_summary.sequence_number;
        let net_total = checkpoint
            .checkpoint_summary
            .data()
            .network_total_transactions;
        let tx_count = checkpoint.transactions.len() as u64;
        // `network_total_transactions` is cumulative *including* this cp.
        // checked_sub: if the cp's network_total_transactions is somehow
        // less than its own tx count, surface an error rather than wrap.
        let tx_lo = net_total.checked_sub(tx_count).ok_or_else(|| {
            StorageError::custom(format!(
                "checkpoint {cp_seq}: network_total_transactions ({net_total}) \
                 < tx_count ({tx_count})"
            ))
        })?;

        // Build one ObjectSet covering all txs in the cp so the dimension
        // extractor's object_set.get(ObjectKey) lookups work without per-tx
        // allocation. Costs O(input_objects + output_objects) clones.
        let mut object_set = ObjectSet::default();
        for tx in &checkpoint.transactions {
            for obj in tx.input_objects.iter().chain(tx.output_objects.iter()) {
                object_set.insert(obj.clone());
            }
        }

        // Group tx-space bitmap bits across the whole checkpoint so repeated
        // dimensions in the same tx bucket produce one Rocks merge operand.
        let mut tx_groups: FxHashMap<(Vec<u8>, u64), RoaringBitmap> = FxHashMap::default();

        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_lo + i as u64;

            let tx_data = tx.transaction.transaction_data();
            let digest = *tx.transaction.digest();
            let event_count = tx.events.as_ref().map(|e| e.data.len() as u32).unwrap_or(0);

            // tx_seq_digest: one direct row per tx, no merge needed.
            batch.insert_batch(
                &self.tx_seq_digest,
                [(
                    tx_seq,
                    TxSeqDigestInfo {
                        digest,
                        event_count,
                        checkpoint_number: cp_seq,
                    },
                )],
            )?;

            // Tx-space bitmap: dedup dimension_keys within this tx, then add
            // this tx's bit to the checkpoint-scoped bitmap group.
            let tx_bucket = tx_seq / TX_BUCKET_SIZE;
            let tx_bit = (tx_seq % TX_BUCKET_SIZE) as u32;
            let mut tx_dim_keys: FxHashSet<Vec<u8>> = FxHashSet::default();
            for_each_transaction_dimension(
                tx_data,
                &tx.effects,
                tx.events.as_ref(),
                &object_set,
                |dim, value| {
                    tx_dim_keys.insert(encode_dimension_key(dim, value));
                },
            );
            for dim_key in tx_dim_keys {
                tx_groups
                    .entry((dim_key, tx_bucket))
                    .or_default()
                    .insert(tx_bit);
            }

            // Event-space bitmap: bits from multiple events of the same tx
            // can share a (dim_key, bucket); group into a RoaringBitmap so
            // we emit at most one operand per group.
            let mut event_groups: FxHashMap<(Vec<u8>, u64), RoaringBitmap> = FxHashMap::default();
            let mut event_seq_error = None;
            for_each_event_dimension(
                tx_data.sender(),
                &tx.effects,
                tx.events.as_ref(),
                |event_idx, dim, value| {
                    let event_seq = match checked_encode_event_seq(tx_seq, event_idx) {
                        Ok(event_seq) => event_seq,
                        Err(e) => {
                            event_seq_error.get_or_insert(e);
                            return;
                        }
                    };
                    let bucket = event_seq / EVENT_BUCKET_SIZE;
                    let bit = (event_seq % EVENT_BUCKET_SIZE) as u32;
                    event_groups
                        .entry((encode_dimension_key(dim, value), bucket))
                        .or_default()
                        .insert(bit);
                },
            );
            if let Some(e) = event_seq_error {
                return Err(e);
            }
            let event_ops = event_groups.into_iter().map(|((dim_key, bucket), bm)| {
                (
                    BitmapIndexKey {
                        dimension_key: dim_key,
                        bucket_id: bucket,
                    },
                    BitmapBlob::from(bm),
                )
            });
            batch.partial_merge_batch(&self.event_bitmap, event_ops)?;
        }

        let tx_ops = tx_groups.into_iter().map(|((dim_key, bucket), bm)| {
            (
                BitmapIndexKey {
                    dimension_key: dim_key,
                    bucket_id: bucket,
                },
                BitmapBlob::from(bm),
            )
        });
        batch.partial_merge_batch(&self.transaction_bitmap, tx_ops)?;

        Ok(())
    }

    fn index_objects(
        &self,
        checkpoint: &CheckpointData,
        batch: &mut typed_store::rocks::DBBatch,
    ) -> Result<(), StorageError> {
        let mut coin_index: HashMap<CoinIndexKey, CoinIndexInfo> = HashMap::new();
        let mut package_version_index: Vec<(PackageVersionKey, PackageVersionInfo)> = vec![];

        for tx in &checkpoint.transactions {
            // determine changes from removed objects
            for removed_object in tx.removed_objects_pre_version() {
                match removed_object.owner() {
                    Owner::AddressOwner(_) | Owner::ConsensusAddressOwner { .. } => {
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
                        Owner::AddressOwner(_) | Owner::ConsensusAddressOwner { .. } => {
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
                    Owner::AddressOwner(_) | Owner::ConsensusAddressOwner { .. } => {
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
        batch.insert_batch(&self.package_version, package_version_index)?;

        Ok(())
    }

    fn get_epoch_info(&self, epoch: EpochId) -> Result<Option<EpochInfo>, TypedStoreError> {
        self.epochs.get(&epoch)
    }

    fn event_iter(
        &self,
        stream_id: SuiAddress,
        start_checkpoint: u64,
        start_accumulator_version: u64,
        start_transaction_idx: u32,
        start_event_idx: u32,
        end_checkpoint: u64,
        limit: u32,
    ) -> Result<impl Iterator<Item = Result<EventIndexKey, TypedStoreError>> + '_, TypedStoreError>
    {
        let lower = EventIndexKey {
            stream_id,
            checkpoint_seq: start_checkpoint,
            accumulator_version: start_accumulator_version,
            transaction_idx: start_transaction_idx,
            event_index: start_event_idx,
        };
        let upper = EventIndexKey {
            stream_id,
            checkpoint_seq: end_checkpoint,
            accumulator_version: u64::MAX,
            transaction_idx: u32::MAX,
            event_index: u32::MAX,
        };

        Ok(self
            .events_by_stream
            .safe_iter_with_bounds(Some(lower), Some(upper))
            .map(|res| res.map(|(k, _)| k))
            .take(limit as usize))
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
    rpc_config: sui_config::RpcConfig,
    /// Shared with the bitmap compaction filters. Advanced by `prune()` after
    /// the corresponding watermark batch commits, so compactions never see a
    /// value that hasn't been persisted.
    ledger_history_pruning_watermark: Arc<AtomicU64>,
    /// True iff this rpc-index DB was built with ledger history indexing
    /// enabled. Derived once at open from the persisted `settings` CF and used
    /// as the gate for forward indexing and pruning.
    ledger_history_enabled: bool,
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
        pruning_watermark: Arc<std::sync::atomic::AtomicU64>,
        rpc_config: sui_config::RpcConfig,
    ) -> Self {
        let events_filter = EventsCompactionFilter::new(pruning_watermark);
        // Internal-only tx-seq floor, hydrated from disk on open.
        let ledger_history_pruning_watermark = Arc::new(AtomicU64::new(0));
        let index_options = IndexStoreOptions {
            events_compaction_filter: Some(events_filter),
            pruning_tx_seq_exclusive: ledger_history_pruning_watermark,
        };

        Self::new_with_options(
            dir,
            authority_store,
            checkpoint_store,
            epoch_store,
            package_store,
            index_options,
            rpc_config,
        )
        .await
    }

    pub async fn new_with_options(
        dir: &Path,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
        index_options: IndexStoreOptions,
        rpc_config: sui_config::RpcConfig,
    ) -> Self {
        let path = Self::db_path(dir);
        let index_config = rpc_config.index_initialization_config();

        let ledger_history_atomic = index_options.pruning_tx_seq_exclusive.clone();

        let tables = {
            let tables = IndexStoreTables::open_with_index_options(&path, index_options.clone());

            // Rebuild if the schema or watermarks are stale, or ledger history
            // is being enabled (which needs a backfill).
            if tables
                .needs_to_do_initialization(checkpoint_store, rpc_config.ledger_history_indexing())
            {
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
                            debug!(
                                "Calculated buffer_size: {} bytes, buffer_count: {} (based on {} CPUs)",
                                buffer_size, buffer_count, target_buffer_count
                            );
                            (buffer_size, buffer_count)
                        }
                        _ => {
                            panic!(
                                "indexing-cf-write-buffer-size and indexing-cf-max-write-buffer-number must both be specified or both be omitted"
                            );
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
                        debug!(
                            "Calculated batch_size_limit: {} bytes (min of half_buffer={} and default_limit={})",
                            limit, half_buffer, default_limit
                        );
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

                    table_config_map.insert(
                        "events_by_stream".to_string(),
                        events_table_options(index_options.events_compaction_filter.clone()),
                    );

                    let bitmap_filter_tx = BitmapCompactionFilter::new(
                        index_options.pruning_tx_seq_exclusive.clone(),
                        BitmapKind::Transaction,
                    );
                    let bitmap_filter_event = BitmapCompactionFilter::new(
                        index_options.pruning_tx_seq_exclusive.clone(),
                        BitmapKind::Event,
                    );
                    let mut transaction_bitmap_opts = cf_options.clone();
                    transaction_bitmap_opts = transaction_bitmap_opts
                        .set_merge_operator_associative(
                            "bitmap_union_merge",
                            bitmap_union_merge_operator,
                        );
                    transaction_bitmap_opts.options.set_compaction_filter(
                        "transaction_bitmap_filter",
                        move |_level, key, value| bitmap_filter_tx.filter(key, value),
                    );
                    table_config_map
                        .insert("transaction_bitmap".to_string(), transaction_bitmap_opts);

                    let mut event_bitmap_opts = cf_options.clone();
                    event_bitmap_opts = event_bitmap_opts.set_merge_operator_associative(
                        "bitmap_union_merge",
                        bitmap_union_merge_operator,
                    );
                    event_bitmap_opts
                        .options
                        .set_compaction_filter("event_bitmap_filter", move |_level, key, value| {
                            bitmap_filter_event.filter(key, value)
                        });
                    table_config_map.insert("event_bitmap".to_string(), event_bitmap_opts);

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
                        &rpc_config,
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
                let reopened_tables =
                    IndexStoreTables::open_with_index_options(&path, index_options);

                // Sanity check: verify the schema version and feature settings
                // were persisted correctly.
                let stored_version = reopened_tables
                    .meta
                    .get(&())
                    .expect("Failed to read metadata from reopened database")
                    .expect("Metadata not found in reopened database");
                assert_eq!(
                    stored_version.version, CURRENT_DB_VERSION,
                    "Database version mismatch after flush and reopen: expected {:#x}, found {:#x}",
                    CURRENT_DB_VERSION, stored_version.version
                );
                assert_eq!(
                    reopened_tables.persisted_ledger_history_indexing(),
                    rpc_config.ledger_history_indexing(),
                    "ledger-history setting mismatch after flush and reopen"
                );

                reopened_tables
            } else {
                // No rebuild needed. If ledger history was on and is now off,
                // drop just those CFs rather than reindexing everything.
                if tables.persisted_ledger_history_indexing()
                    && !rpc_config.ledger_history_indexing()
                {
                    tables
                        .disable_ledger_history_indexing()
                        .expect("unable to disable ledger history indexing");
                }
                tables
            }
        };

        // Hydrate before compaction filters can observe the default 0 floor.
        Self::hydrate_ledger_history_pruning_atomic(&tables, &ledger_history_atomic);

        // `ledger_history_enabled` is derived from the persisted `settings` CF,
        // not directly from config.
        let ledger_history_enabled = tables.persisted_ledger_history_indexing();
        debug_assert_eq!(
            ledger_history_enabled,
            rpc_config.ledger_history_indexing(),
            "ledger_history_enabled (from settings CF) must match the configured ledger_history_indexing flag"
        );

        Self {
            tables,
            pending_updates: Default::default(),
            rpc_config,
            ledger_history_pruning_watermark: ledger_history_atomic,
            ledger_history_enabled,
        }
    }

    /// Hydrate the tx-seq pruning floor from the first live `tx_seq_digest` key.
    fn hydrate_ledger_history_pruning_atomic(tables: &IndexStoreTables, atomic: &Arc<AtomicU64>) {
        let persisted = tables.first_tx_seq_digest_key().ok().flatten().unwrap_or(0);
        atomic.store(persisted, Ordering::Relaxed);
    }

    pub fn new_without_init(dir: &Path) -> Self {
        let path = Self::db_path(dir);

        // Keep already-built ledger history indexes prunable in offline paths.
        let ledger_history_atomic = Arc::new(AtomicU64::new(0));
        let index_options = IndexStoreOptions {
            events_compaction_filter: None,
            pruning_tx_seq_exclusive: ledger_history_atomic.clone(),
        };
        let tables = IndexStoreTables::open_with_index_options(path, index_options);

        let ledger_history_enabled = tables.persisted_ledger_history_indexing();
        Self::hydrate_ledger_history_pruning_atomic(&tables, &ledger_history_atomic);

        Self {
            tables,
            pending_updates: Default::default(),
            ledger_history_pruning_watermark: ledger_history_atomic,
            rpc_config: sui_config::RpcConfig::default(),
            ledger_history_enabled,
        }
    }

    pub fn prune(
        &self,
        pruned_checkpoint_watermark: u64,
        pruned_tx_seq_exclusive: u64,
    ) -> Result<(), TypedStoreError> {
        let new_exclusive = self.tables.prune(
            pruned_checkpoint_watermark,
            pruned_tx_seq_exclusive,
            self.ledger_history_enabled,
        )?;

        // Advance the compaction-filter atomic ONLY after the batch
        // commits, so a compaction can never observe a watermark that
        // hasn't been persisted.
        if let Some(new_exclusive) = new_exclusive {
            self.ledger_history_pruning_watermark
                .store(new_exclusive, Ordering::Relaxed);
        }
        Ok(())
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
            .index_checkpoint(
                checkpoint,
                resolver,
                &self.rpc_config,
                self.ledger_history_enabled,
            )
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

    pub fn event_iter(
        &self,
        stream_id: SuiAddress,
        start_checkpoint: u64,
        start_accumulator_version: u64,
        start_transaction_idx: u32,
        start_event_idx: u32,
        end_checkpoint: u64,
        limit: u32,
    ) -> Result<impl Iterator<Item = Result<EventIndexKey, TypedStoreError>> + '_, TypedStoreError>
    {
        self.tables.event_iter(
            stream_id,
            start_checkpoint,
            start_accumulator_version,
            start_transaction_idx,
            start_event_idx,
            end_checkpoint,
            limit,
        )
    }

    pub fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        self.tables.watermark.get(&Watermark::Indexed)
    }

    fn ensure_ledger_history_enabled(&self) -> Result<(), TypedStoreError> {
        if self.ledger_history_enabled {
            Ok(())
        } else {
            Err(TypedStoreError::SerializationError(
                "ledger history indexing is disabled".to_owned(),
            ))
        }
    }

    pub fn ledger_tx_seq_digest(
        &self,
        tx_seq: u64,
    ) -> Result<Option<LedgerTxSeqDigest>, TypedStoreError> {
        self.ensure_ledger_history_enabled()?;
        Ok(self
            .tables
            .tx_seq_digest
            .get(&tx_seq)?
            .map(|info| ledger_tx_seq_digest(tx_seq, info)))
    }

    pub fn ledger_tx_seq_digest_multi_get(
        &self,
        tx_seqs: &[u64],
    ) -> Result<Vec<Option<LedgerTxSeqDigest>>, TypedStoreError> {
        self.ensure_ledger_history_enabled()?;
        let rows = self
            .tables
            .tx_seq_digest
            .multi_get(tx_seqs)?
            .into_iter()
            .zip_debug_eq(tx_seqs.iter().copied())
            .map(|(info, tx_seq)| info.map(|info| ledger_tx_seq_digest(tx_seq, info)))
            .collect();
        Ok(rows)
    }

    pub fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerTxSeqDigestIterator<'_>, TypedStoreError> {
        self.ensure_ledger_history_enabled()?;
        if start >= end_exclusive {
            return Ok(Box::new(std::iter::empty()));
        }

        let iter = if descending {
            let upper = end_exclusive - 1;
            self.tables
                .tx_seq_digest
                .reversed_safe_iter_with_bounds(Some(start), Some(upper))?
        } else {
            self.tables
                .tx_seq_digest
                .safe_iter_with_bounds(Some(start), Some(end_exclusive))
        };

        Ok(Box::new(iter.map(|result| {
            result.map(|(tx_seq, info)| ledger_tx_seq_digest(tx_seq, info))
        })))
    }

    pub fn transaction_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerBitmapBucketIterator<'_>, TypedStoreError> {
        self.ensure_ledger_history_enabled()?;
        Self::bitmap_bucket_iter(
            &self.tables.transaction_bitmap,
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }

    pub fn event_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerBitmapBucketIterator<'_>, TypedStoreError> {
        self.ensure_ledger_history_enabled()?;
        Self::bitmap_bucket_iter(
            &self.tables.event_bitmap,
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }

    fn bitmap_bucket_iter(
        table: &DBMap<BitmapIndexKey, BitmapBlob>,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerBitmapBucketIterator<'_>, TypedStoreError> {
        if start_bucket >= end_bucket_exclusive {
            return Ok(Box::new(std::iter::empty()));
        }

        let lower = BitmapIndexKey {
            dimension_key: dimension_key.clone(),
            bucket_id: start_bucket,
        };
        let upper_exclusive = BitmapIndexKey {
            dimension_key,
            bucket_id: end_bucket_exclusive,
        };
        let upper_inclusive = BitmapIndexKey {
            dimension_key: upper_exclusive.dimension_key.clone(),
            bucket_id: end_bucket_exclusive - 1,
        };

        let iter: Box<
            dyn Iterator<Item = Result<(BitmapIndexKey, BitmapBlob), TypedStoreError>> + '_,
        > = if descending {
            table.reversed_safe_iter_with_bounds(Some(lower), Some(upper_inclusive))?
        } else {
            table.safe_iter_with_bounds(Some(lower), Some(upper_exclusive))
        };

        Ok(Box::new(iter.map(|result| {
            result.and_then(|(key, blob)| decode_ledger_bitmap_bucket(key, blob))
        })))
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
                    let balance_info = BalanceIndexInfo {
                        coin_balance_delta: value.into(),
                        address_balance_delta: 0,
                    };
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

                // Index address balances
                if parent == SUI_ACCUMULATOR_ROOT_OBJECT_ID.into()
                    && let Some((owner, coin_type, balance)) = get_address_balance_info(&object)
                {
                    let balance_key = BalanceKey { owner, coin_type };
                    let balance_info = BalanceIndexInfo {
                        coin_balance_delta: 0,
                        address_balance_delta: balance,
                    };
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
                .write_opt(bulk_ingestion_write_options())?;
        }

        Ok(())
    }

    fn finish(mut self) -> Result<(), StorageError> {
        self.batch.partial_merge_batch(
            &self.tables.balance,
            std::mem::take(&mut self.balance_changes),
        )?;
        self.batch.write_opt(bulk_ingestion_write_options())?;
        Ok(())
    }
}

// TODO figure out a way to dedup this logic. Today we'd need to do quite a bit of refactoring to
// make it possible.
/// Load full `CheckpointData` for `checkpoint` from local storage. Sibling
/// of [`sparse_checkpoint_data_for_epoch_backfill`] that returns data for
/// every cp (not just genesis / EoE) and always loads transaction events.
///
/// Returns `Ok(None)` if the cp's summary or contents are not present
/// locally (e.g. pruned out of the underlying store).
fn full_checkpoint_data_for_backfill(
    authority_store: &AuthorityStore,
    checkpoint_store: &CheckpointStore,
    checkpoint: u64,
) -> Result<Option<CheckpointData>, StorageError> {
    let Some(summary) = checkpoint_store.get_checkpoint_by_sequence_number(checkpoint)? else {
        return Ok(None);
    };
    let Some(contents) = checkpoint_store.get_checkpoint_contents(&summary.content_digest)? else {
        return Ok(None);
    };

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

    // Always load events: event-space dimensions need them, and tx-space
    // dimensions include EmitModule / EventType / EventStreamHead which are
    // sourced from events too.
    let events = authority_store
        .multi_get_events(&transaction_digests)
        .map_err(|e| StorageError::custom(e.to_string()))?;

    let mut full_transactions = Vec::with_capacity(transactions.len());
    for ((tx, fx), ev) in transactions
        .into_iter()
        .zip_debug_eq(effects)
        .zip_debug_eq(events)
    {
        let input_objects =
            sui_types::storage::get_transaction_input_objects(authority_store, &fx)?;
        let output_objects =
            sui_types::storage::get_transaction_output_objects(authority_store, &fx)?;

        full_transactions.push(CheckpointTransaction {
            transaction: tx.into(),
            effects: fx,
            events: ev,
            input_objects,
            output_objects,
        });
    }

    Ok(Some(CheckpointData {
        checkpoint_summary: summary.into(),
        checkpoint_contents: contents,
        transactions: full_transactions,
    }))
}

fn sparse_checkpoint_data_for_epoch_backfill(
    authority_store: &AuthorityStore,
    checkpoint_store: &CheckpointStore,
    checkpoint: u64,
    load_events: bool,
) -> Result<Option<CheckpointData>, StorageError> {
    let summary = checkpoint_store
        .get_checkpoint_by_sequence_number(checkpoint)?
        .ok_or_else(|| StorageError::missing(format!("missing checkpoint {checkpoint}")))?;

    // Only load genesis and end of epoch checkpoints
    if summary.end_of_epoch_data.is_none() && summary.sequence_number != 0 {
        return Ok(None);
    }

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

    let events = if load_events {
        authority_store
            .multi_get_events(&transaction_digests)
            .map_err(|e| StorageError::custom(e.to_string()))?
    } else {
        vec![None; transaction_digests.len()]
    };

    let mut full_transactions = Vec::with_capacity(transactions.len());
    for ((tx, fx), ev) in transactions
        .into_iter()
        .zip_debug_eq(effects)
        .zip_debug_eq(events)
    {
        let input_objects =
            sui_types::storage::get_transaction_input_objects(authority_store, &fx)?;
        let output_objects =
            sui_types::storage::get_transaction_output_objects(authority_store, &fx)?;

        let full_transaction = CheckpointTransaction {
            transaction: tx.into(),
            effects: fx,
            events: ev,
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

    Ok(Some(checkpoint_data))
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

fn get_address_balance_info(object: &Object) -> Option<(SuiAddress, StructTag, i128)> {
    let move_object = object.data.try_as_move()?;

    let TypeTag::Struct(coin_type) = move_object.type_().balance_accumulator_field_type_maybe()?
    else {
        return None;
    };

    let (key, value): (
        sui_types::accumulator_root::AccumulatorKey,
        sui_types::accumulator_root::AccumulatorValue,
    ) = move_object.try_into().ok()?;

    let balance = value.as_u128()? as i128;
    if balance <= 0 {
        return None;
    }

    Some((key.owner, *coin_type, balance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use sui_types::base_types::SuiAddress;

    #[tokio::test]
    async fn test_events_compaction_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path();
        let db_path = path.join("rpc-index");

        let pruning_watermark = Arc::new(AtomicU64::new(5));
        let compaction_filter = EventsCompactionFilter::new(pruning_watermark.clone());

        let index_options = IndexStoreOptions {
            events_compaction_filter: Some(compaction_filter),
            pruning_tx_seq_exclusive: Arc::new(AtomicU64::new(0)),
        };

        let tables = IndexStoreTables::open_with_index_options(&db_path, index_options);
        let stream_id = SuiAddress::random_for_testing_only();
        let test_events: Vec<EventIndexKey> = [1, 3, 5, 10, 15]
            .iter()
            .map(|&checkpoint_seq| EventIndexKey {
                stream_id,
                checkpoint_seq,
                accumulator_version: 0,
                transaction_idx: 0,
                event_index: 0,
            })
            .collect();

        let mut batch = tables.events_by_stream.batch();
        for key in &test_events {
            batch
                .insert_batch(&tables.events_by_stream, [(key.clone(), ())])
                .unwrap();
        }
        batch.write().unwrap();

        tables.events_by_stream.flush().unwrap();
        let mut events_before_compaction = 0;
        for result in tables.events_by_stream.safe_iter() {
            if result.is_ok() {
                events_before_compaction += 1;
            }
        }
        assert_eq!(
            events_before_compaction, 5,
            "Should have 5 events before compaction"
        );
        let start_key = EventIndexKey {
            stream_id: SuiAddress::ZERO,
            checkpoint_seq: 0,
            accumulator_version: 0,
            transaction_idx: 0,
            event_index: 0,
        };
        let end_key = EventIndexKey {
            stream_id: SuiAddress::random_for_testing_only(),
            checkpoint_seq: u64::MAX,
            accumulator_version: u64::MAX,
            transaction_idx: u32::MAX,
            event_index: u32::MAX,
        };

        tables
            .events_by_stream
            .compact_range(&start_key, &end_key)
            .unwrap();
        let mut events_after_compaction = Vec::new();
        for (key, _event) in tables.events_by_stream.safe_iter().flatten() {
            events_after_compaction.push(key);
        }

        println!("Events after compaction: {}", events_after_compaction.len());
        assert!(
            events_after_compaction.len() >= 2,
            "Should have at least the events that shouldn't be pruned"
        );
        pruning_watermark.store(20, std::sync::atomic::Ordering::Relaxed);
        let watermark_after = pruning_watermark.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(watermark_after, 20, "Watermark should be updated");
    }

    #[test]
    fn test_events_compaction_filter_logic() {
        let watermark = Arc::new(AtomicU64::new(100));
        let filter = EventsCompactionFilter::new(watermark.clone());

        let old_key = EventIndexKey {
            stream_id: SuiAddress::random_for_testing_only(),
            checkpoint_seq: 50,
            accumulator_version: 0,
            transaction_idx: 0,
            event_index: 0,
        };
        let old_key_bytes = bcs::to_bytes(&old_key).unwrap();
        let decision = filter.filter(&old_key_bytes, &[]).unwrap();
        assert!(
            matches!(decision, Decision::Remove),
            "Event with checkpoint 50 should be removed when watermark is 100"
        );
        let new_key = EventIndexKey {
            stream_id: SuiAddress::random_for_testing_only(),
            checkpoint_seq: 150,
            accumulator_version: 0,
            transaction_idx: 0,
            event_index: 0,
        };
        let new_key_bytes = bcs::to_bytes(&new_key).unwrap();
        let decision = filter.filter(&new_key_bytes, &[]).unwrap();
        assert!(
            matches!(decision, Decision::Keep),
            "Event with checkpoint 150 should be kept when watermark is 100"
        );
        let boundary_key = EventIndexKey {
            stream_id: SuiAddress::random_for_testing_only(),
            checkpoint_seq: 100,
            accumulator_version: 0,
            transaction_idx: 0,
            event_index: 0,
        };
        let boundary_key_bytes = bcs::to_bytes(&boundary_key).unwrap();
        let decision = filter.filter(&boundary_key_bytes, &[]).unwrap();
        assert!(
            matches!(decision, Decision::Remove),
            "Event with checkpoint equal to watermark should be removed"
        );
    }

    /// Every column family opened via `open_with_index_options` must have the
    /// `disable_write_throttling` override applied. The typed-store derive
    /// macro silently falls back to bare `default_db_options()` for any CF
    /// missing from `tables_db_options_override`, reverting to RocksDB's
    /// default stall triggers (slowdown=20, stop=36) — small enough that ~80
    /// L0 files would stop writes entirely. RocksDB persists the effective
    /// per-CF options to an `OPTIONS-NNNNNN` file at open; parse it to verify
    /// every CF received the override.
    #[tokio::test]
    async fn open_with_index_options_overrides_every_cf() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");

        let _tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Iterate the CFs RocksDB actually wrote to the OPTIONS file rather
        // than the schema-declared set, since deprecated CFs are dropped at
        // open time and never appear in OPTIONS. RocksDB always writes a
        // `default` CF; we exclude it because typed-store doesn't store data
        // there and we don't configure it.
        let per_cf = parse_cf_options(&db_path);
        assert!(
            !per_cf.is_empty(),
            "expected at least one CFOptions section in OPTIONS file"
        );
        for (cf_name, opts) in &per_cf {
            if cf_name == "default" {
                continue;
            }
            for (key, expected) in [
                ("level0_slowdown_writes_trigger", "512"),
                ("level0_stop_writes_trigger", "1024"),
                ("soft_pending_compaction_bytes_limit", "0"),
                ("hard_pending_compaction_bytes_limit", "0"),
            ] {
                let actual = opts
                    .get(key)
                    .unwrap_or_else(|| panic!("cf `{cf_name}` missing `{key}`"));
                assert_eq!(
                    actual, expected,
                    "cf `{cf_name}` has `{key}={actual}`, expected `{expected}` — \
                     the typed-store override map likely doesn't cover this CF"
                );
            }
        }
    }

    #[test]
    fn checked_encode_event_seq_rejects_unrepresentable_values() {
        assert!(
            checked_encode_event_seq(0, MAX_EVENTS_PER_TX).is_err(),
            "event_idx at MAX_EVENTS_PER_TX must be rejected"
        );
        assert!(
            checked_encode_event_seq(MAX_TX_SEQ + 1, 0).is_err(),
            "tx_seq past MAX_TX_SEQ must be rejected"
        );
    }

    /// Compaction filter math: tx-bitmap whole-bucket removability around
    /// the tx == 0 boundary. With the pruning atomic at 1, only tx_seq 0
    /// is gone. Bucket 0 spans tx_seqs [0, 65_536), so it is NOT entirely
    /// pruned and must be kept. This is exactly the off-by-one case the
    /// exclusive floor is supposed to make explicit.
    #[test]
    fn bitmap_filter_keeps_bucket_with_live_tx_above_zero_watermark() {
        let watermark = Arc::new(AtomicU64::new(1));
        let filter = BitmapCompactionFilter::new(watermark.clone(), BitmapKind::Transaction);

        let key = typed_store::be_fix_int_ser(&BitmapIndexKey {
            dimension_key: vec![1, 2, 3],
            bucket_id: 0,
        });
        assert!(matches!(filter.filter(&key, &[]), Decision::Keep));

        // Once the watermark advances to TX_BUCKET_SIZE, bucket 0 becomes
        // fully prunable (highest tx in bucket 0 is 65_535, exclusive of
        // 65_536 means the next bucket starts there — everything below is
        // pruned).
        watermark.store(TX_BUCKET_SIZE, Ordering::Relaxed);
        assert!(matches!(filter.filter(&key, &[]), Decision::Remove));
    }

    /// Compaction filter math: event-bitmap removability uses
    /// `event_seq_lo(pruned_exclusive)` as the threshold, so its math is
    /// scaled by EVENT_BITS relative to tx-bitmap.
    #[test]
    fn bitmap_filter_event_bucket_uses_event_seq_lo() {
        let watermark = Arc::new(AtomicU64::new(0));
        let filter = BitmapCompactionFilter::new(watermark.clone(), BitmapKind::Event);

        let key = typed_store::be_fix_int_ser(&BitmapIndexKey {
            dimension_key: vec![5],
            bucket_id: 0,
        });
        // Watermark 0: nothing pruned → keep.
        assert!(matches!(filter.filter(&key, &[]), Decision::Keep));

        // The highest tx whose event_seq can fall in bucket 0 is
        // (EVENT_BUCKET_SIZE / MAX_EVENTS_PER_TX) - 1. Need watermark to
        // exceed that for bucket 0 to be fully prunable.
        let txs_per_bucket = EVENT_BUCKET_SIZE / MAX_EVENTS_PER_TX as u64;
        watermark.store(txs_per_bucket, Ordering::Relaxed);
        assert!(matches!(filter.filter(&key, &[]), Decision::Remove));
    }

    /// Malformed keys must never be silently `Remove`d — silent data loss
    /// is much worse than a stuck row. A too-short key and a key with a
    /// bucket_id that would overflow the bucket-hi computation should
    /// both be kept.
    #[test]
    fn bitmap_filter_keeps_malformed_keys() {
        let watermark = Arc::new(AtomicU64::new(u64::MAX));
        let filter = BitmapCompactionFilter::new(watermark.clone(), BitmapKind::Transaction);

        assert!(matches!(filter.filter(b"short", &[]), Decision::Keep));
        assert!(matches!(filter.filter(&[], &[]), Decision::Keep));

        // bucket_id near u64::MAX would overflow `(b+1)*TX_BUCKET_SIZE`.
        // The checked math returns None → keep.
        let huge = typed_store::be_fix_int_ser(&BitmapIndexKey {
            dimension_key: vec![],
            bucket_id: u64::MAX - 1,
        });
        assert!(huge.len() >= 8);
        assert!(matches!(filter.filter(&huge, &[]), Decision::Keep));
    }

    /// Round-trip the typed-store encoding: a `BitmapIndexKey` encoded via
    /// `be_fix_int_ser` ends with the bucket_id as 8 big-endian bytes that
    /// the compaction filter reads back. Guards against silent drift if
    /// typed-store changes its key serializer.
    #[test]
    fn bitmap_filter_decodes_typed_store_keys() {
        let watermark = Arc::new(AtomicU64::new(0));
        let filter = BitmapCompactionFilter::new(watermark.clone(), BitmapKind::Transaction);

        // Build a key with a bucket_id that, after advancing the watermark
        // far enough, would be removable. Confirm the filter agrees.
        let bucket_id = 7u64;
        let key = typed_store::be_fix_int_ser(&BitmapIndexKey {
            dimension_key: vec![0xAA, 0xBB, 0xCC],
            bucket_id,
        });
        // First, with watermark = 0, definitely keep.
        assert!(matches!(filter.filter(&key, &[]), Decision::Keep));

        // Advance watermark past (bucket_id + 1) * TX_BUCKET_SIZE → remove.
        watermark.store((bucket_id + 1) * TX_BUCKET_SIZE, Ordering::Relaxed);
        assert!(matches!(filter.filter(&key, &[]), Decision::Remove));
    }

    /// The merge operator must OR multiple operands into a single bitmap —
    /// not last-write-wins, which is what we'd get without it.
    #[test]
    fn bitmap_merge_operator_unions_operands() {
        let mut bm_a = RoaringBitmap::new();
        bm_a.insert(1);
        bm_a.insert(5);
        let blob_a = encode_bitmap_blob(&bm_a);

        let mut bm_b = RoaringBitmap::new();
        bm_b.insert(5);
        bm_b.insert(7);
        let blob_b = encode_bitmap_blob(&bm_b);

        let mut bm_c = RoaringBitmap::new();
        bm_c.insert(100);
        let blob_c = encode_bitmap_blob(&bm_c);

        // Simulate rocksdb feeding [blob_b, blob_c] as operands with no
        // existing on-disk value (which is what happens on first merge into
        // a new key).
        //
        // We can't easily construct a `MergeOperands` from outside rocksdb,
        // so test the decode/encode round-trip via the helpers directly and
        // assert the union over decoded bitmaps. This validates the data
        // path the merge operator depends on; the operator's loop is a
        // trivial `acc |= bm` on top.
        let decoded_a = decode_bitmap_blob(&blob_a).expect("decode a");
        let decoded_b = decode_bitmap_blob(&blob_b).expect("decode b");
        let decoded_c = decode_bitmap_blob(&blob_c).expect("decode c");
        let unioned = decoded_a | decoded_b | decoded_c;
        let mut expected = RoaringBitmap::new();
        for b in [1, 5, 7, 100] {
            expected.insert(b);
        }
        assert_eq!(unioned, expected);
    }

    /// End-to-end: write merge operands across multiple "checkpoints" into a
    /// real DBMap and confirm the merge operator unions them when read back.
    /// This is the only test that exercises the in-rocksdb merge path,
    /// since `bitmap_merge_operator_unions_operands` only round-trips the
    /// helpers.
    #[tokio::test]
    async fn bitmap_merge_operator_unions_across_writes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        let key = BitmapIndexKey {
            dimension_key: vec![1, 2, 3],
            bucket_id: 0,
        };

        // Write three merge operands targeting the same key. Without the
        // merge operator, the last write would clobber the first two; with
        // it, all bits should be present.
        for bits in [vec![1u32, 2], vec![3, 4], vec![5, 6, 7]] {
            let mut bm = RoaringBitmap::new();
            for b in bits {
                bm.insert(b);
            }
            let mut batch = tables.transaction_bitmap.batch();
            batch
                .partial_merge_batch(
                    &tables.transaction_bitmap,
                    [(key.clone(), BitmapBlob::from(bm))],
                )
                .unwrap();
            batch.write().unwrap();
        }

        let blob = tables
            .transaction_bitmap
            .get(&key)
            .unwrap()
            .expect("merged row should exist");
        let bm = RoaringBitmap::deserialize_from(&blob.0[..]).unwrap();
        let got: Vec<u32> = bm.iter().collect();
        assert_eq!(got, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    /// Whole-bucket compaction-filter removal: write bits to buckets 0 and
    /// 1, advance the pruning watermark past bucket 0 only, force a
    /// compaction, then assert bucket 0 is gone and bucket 1 survives.
    #[tokio::test]
    async fn bitmap_filter_removes_whole_bucket_after_compaction() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");

        let watermark = Arc::new(AtomicU64::new(0));
        let index_options = IndexStoreOptions {
            events_compaction_filter: None,
            pruning_tx_seq_exclusive: watermark.clone(),
        };
        let tables = IndexStoreTables::open_with_index_options(&db_path, index_options);

        let dim_key = vec![0x01, 0xAA];
        let k0 = BitmapIndexKey {
            dimension_key: dim_key.clone(),
            bucket_id: 0,
        };
        let k1 = BitmapIndexKey {
            dimension_key: dim_key.clone(),
            bucket_id: 1,
        };
        let mut bm = RoaringBitmap::new();
        bm.insert(0);

        // Use a direct insert rather than a merge so we exercise the
        // compaction filter on a regular value. (Merge interaction is
        // covered by `bitmap_merge_operator_unions_across_writes`.)
        let blob = BitmapBlob::from(bm);
        let mut batch = tables.transaction_bitmap.batch();
        batch
            .insert_batch(
                &tables.transaction_bitmap,
                [(k0.clone(), blob.clone()), (k1.clone(), blob)],
            )
            .unwrap();
        batch.write().unwrap();
        tables.transaction_bitmap.flush().unwrap();

        // Sanity-check: both buckets are present before advancing the
        // watermark.
        assert!(tables.transaction_bitmap.get(&k0).unwrap().is_some());
        assert!(tables.transaction_bitmap.get(&k1).unwrap().is_some());

        // Advance watermark past bucket 0 but not past bucket 1.
        watermark.store(TX_BUCKET_SIZE, Ordering::Relaxed);

        // Compact the entire keyspace with raw byte bounds to ensure we
        // cover every encoded BitmapIndexKey, regardless of typed-store's
        // length-prefix width.
        tables
            .transaction_bitmap
            .compact_range_raw("transaction_bitmap", vec![], vec![0xFF; 128])
            .unwrap();

        assert!(
            tables.transaction_bitmap.get(&k0).unwrap().is_none(),
            "bucket 0 should have been removed by the compaction filter"
        );
        assert!(
            tables.transaction_bitmap.get(&k1).unwrap().is_some(),
            "bucket 1 should still be present (only bucket 0 was below the watermark)"
        );
    }

    /// The retained backfill range must surface missing cps as an error,
    /// not silently leave a permanent hole. This tests the
    /// `ok_or_else(...)` conversion isolated from the full init path.
    #[test]
    fn backfill_missing_cp_in_retained_range_is_error() {
        // Mirror the backfill closure: take the
        // `Result<Option<CheckpointData>>` from the loader, and require
        // `Some(cp_data)` for every cp in the retained range.
        let checkpoint_range = 5u64..=10u64;
        let seq = 7u64;
        let loaded: Result<Option<CheckpointData>, StorageError> = Ok(None);

        let result: Result<(), StorageError> = (|| {
            let cp_data = loaded?.ok_or_else(|| {
                StorageError::missing(format!(
                    "ledger history backfill: checkpoint {seq} is missing from local storage \
                     but falls inside the retained backfill range {checkpoint_range:?}"
                ))
            })?;
            let _ = cp_data;
            Ok(())
        })();

        let err = result.expect_err("missing cp must error out, not silently succeed");
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("checkpoint {seq}")),
            "error should name the missing cp: {msg}"
        );
        assert!(
            msg.contains("5..=10"),
            "error should name the retained range: {msg}"
        );
    }

    /// Existing watermark rows on disk encode `Indexed` and `Pruned` as serde
    /// indexes 0 and 1. Reordering the enum (or inserting a variant before
    /// them) would shift those discriminants and silently misread on-disk
    /// rows. Hardcode the legacy bytes and confirm they still round-trip.
    #[test]
    fn legacy_watermark_bytes_still_deserialize() {
        // BCS encodes a unit enum variant as a ULEB128 of its index.
        let indexed_bytes = bcs::to_bytes(&Watermark::Indexed).unwrap();
        let pruned_bytes = bcs::to_bytes(&Watermark::Pruned).unwrap();
        assert_eq!(
            indexed_bytes,
            vec![0],
            "Watermark::Indexed must encode as 0"
        );
        assert_eq!(pruned_bytes, vec![1], "Watermark::Pruned must encode as 1");

        // Feed the canonical legacy bytes to the deserializer and confirm
        // they still arrive at the right variants. This is the test that
        // would catch an accidental reorder of the enum.
        let decoded_indexed: Watermark = bcs::from_bytes(&[0]).unwrap();
        let decoded_pruned: Watermark = bcs::from_bytes(&[1]).unwrap();
        assert!(matches!(decoded_indexed, Watermark::Indexed));
        assert!(matches!(decoded_pruned, Watermark::Pruned));
    }

    /// The schema version and the ledger-history feature flag are tracked
    /// independently: the flag round-trips through the `settings` CF, and
    /// *enabling* the feature forces a reinit while the schema version is
    /// unchanged. *Disabling* it does not (that is handled in place).
    #[tokio::test]
    async fn settings_cf_round_trips_and_drives_reinit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Stamp a current-schema DB built with ledger history disabled.
        tables
            .meta
            .insert(
                &(),
                &MetadataInfo {
                    version: CURRENT_DB_VERSION,
                },
            )
            .unwrap();
        tables
            .settings
            .insert(
                &(),
                &IndexSettings {
                    ledger_history_indexing: false,
                },
            )
            .unwrap();
        assert!(!tables.persisted_ledger_history_indexing());

        // An empty checkpoint store keeps the watermark check out of the way so
        // we isolate the schema-version / feature-toggle logic.
        let checkpoint_store = CheckpointStore::new_for_tests();

        // Same schema, same feature setting => no reinit.
        assert!(!tables.needs_to_do_initialization(&checkpoint_store, false));
        // Same schema, but the feature was toggled on => reinit.
        assert!(tables.needs_to_do_initialization(&checkpoint_store, true));

        // Flip the persisted flag; it round-trips.
        tables
            .settings
            .insert(
                &(),
                &IndexSettings {
                    ledger_history_indexing: true,
                },
            )
            .unwrap();
        assert!(tables.persisted_ledger_history_indexing());
        // Already enabled => no reinit.
        assert!(!tables.needs_to_do_initialization(&checkpoint_store, true));
        // Disabling does NOT force a rebuild — the history CFs are dropped in
        // place instead.
        assert!(!tables.needs_to_do_initialization(&checkpoint_store, false));
    }

    /// `disable_ledger_history_indexing` empties every ledger-history CF (not
    /// all-but-the-last, the trap with end-exclusive range deletes) and clears
    /// the persisted flag, while leaving the base indexes untouched.
    #[tokio::test]
    async fn disable_ledger_history_indexing_drops_history_cfs_in_place() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Seed a ledger-history DB with several rows in each history CF.
        tables
            .settings
            .insert(
                &(),
                &IndexSettings {
                    ledger_history_indexing: true,
                },
            )
            .unwrap();
        for tx_seq in 0..5u64 {
            tables
                .tx_seq_digest
                .insert(
                    &tx_seq,
                    &TxSeqDigestInfo {
                        digest: TransactionDigest::new([0; 32]),
                        event_count: 0,
                        checkpoint_number: 0,
                    },
                )
                .unwrap();
        }
        for bucket_id in 0..5u64 {
            let key = BitmapIndexKey {
                dimension_key: vec![1, 2, 3],
                bucket_id,
            };
            tables
                .transaction_bitmap
                .insert(&key, &BitmapBlob(vec![0xab]))
                .unwrap();
            tables
                .event_bitmap
                .insert(&key, &BitmapBlob(vec![0xcd]))
                .unwrap();
        }
        // A non-history CF that must survive the drop.
        tables.watermark.insert(&Watermark::Indexed, &42).unwrap();

        tables.disable_ledger_history_indexing().unwrap();

        // Every history row is gone — including the last key in each CF.
        assert!(tables.tx_seq_digest.is_empty());
        assert!(tables.transaction_bitmap.is_empty());
        assert!(tables.event_bitmap.is_empty());
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), None);

        // The flag is cleared and the base index is untouched.
        assert!(!tables.persisted_ledger_history_indexing());
        assert_eq!(tables.watermark.get(&Watermark::Indexed).unwrap(), Some(42));
    }

    /// `prune()` advances `Watermark::Pruned` and deletes the exact
    /// tx_seq_digest range below the new tx-seq floor when enabled.
    #[tokio::test]
    async fn prune_maintains_ledger_history_state_when_active() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Seed rows to simulate an indexed ledger history subsystem.
        let mut batch = tables.tx_seq_digest.batch();
        for tx_seq in 0..5u64 {
            batch
                .insert_batch(
                    &tables.tx_seq_digest,
                    [(
                        tx_seq,
                        TxSeqDigestInfo {
                            digest: TransactionDigest::new([0; 32]),
                            event_count: 0,
                            checkpoint_number: 0,
                        },
                    )],
                )
                .unwrap();
        }
        batch.write().unwrap();

        // Prune cp 1 with an absolute tx-seq floor of 3 → rows 0..3 should
        // be deleted, the derived floor advances from 0 to 3.
        let new_exclusive = tables
            .prune(1, 3, /*ledger_history_enabled=*/ true)
            .unwrap();
        assert_eq!(new_exclusive, Some(3));

        assert_eq!(tables.watermark.get(&Watermark::Pruned).unwrap(), Some(1));
        for tx_seq in 0..3u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_none());
        }
        for tx_seq in 3..5u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_some());
        }
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), Some(3));
    }

    /// When disabled, `prune` must advance only `Watermark::Pruned`.
    #[tokio::test]
    async fn prune_skips_ledger_history_state_when_inactive() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Seed a tx_seq_digest row so we can confirm prune leaves it alone.
        tables
            .tx_seq_digest
            .insert(
                &0u64,
                &TxSeqDigestInfo {
                    digest: TransactionDigest::new([0; 32]),
                    event_count: 0,
                    checkpoint_number: 0,
                },
            )
            .unwrap();

        let new_exclusive = tables
            .prune(5, 3, /*ledger_history_enabled=*/ false)
            .unwrap();
        assert_eq!(new_exclusive, None);
        assert_eq!(
            tables.watermark.get(&Watermark::Pruned).unwrap(),
            Some(5),
            "base pruning must still advance"
        );
        assert!(
            tables.tx_seq_digest.get(&0u64).unwrap().is_some(),
            "tx_seq_digest rows must remain untouched when inactive"
        );
    }

    /// Forward `index_checkpoint` writes ledger history rows only when enabled.
    #[tokio::test]
    async fn index_checkpoint_gates_on_ledger_history_enabled() {
        use sui_types::layout_resolver::LayoutResolver;
        use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        // Non-zero cp seq to skip the genesis path in `index_epoch` (which
        // expects a real system state object).
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();
        let checkpoint_data: CheckpointData = checkpoint.into();

        let rpc_config = sui_config::RpcConfig::default();

        struct PanicResolver;
        impl LayoutResolver for PanicResolver {
            fn get_annotated_layout(
                &mut self,
                _struct_tag: &move_core_types::language_storage::StructTag,
            ) -> Result<
                move_core_types::annotated_value::MoveDatatypeLayout,
                sui_types::error::SuiError,
            > {
                panic!("layout resolver should not be invoked by ledger history indexing");
            }
        }

        // Disabled: no ledger history writes.
        let batch = tables
            .index_checkpoint(
                &checkpoint_data,
                &mut PanicResolver,
                &rpc_config,
                /*ledger_history_enabled=*/ false,
            )
            .expect("index_checkpoint failed");
        batch.write().expect("batch write failed");
        assert_eq!(tables.tx_seq_digest.safe_iter().count(), 0);
        assert_eq!(tables.transaction_bitmap.safe_iter().count(), 0);
        assert_eq!(tables.event_bitmap.safe_iter().count(), 0);

        // Enabled → forward writes land.
        let checkpoint2 = TestCheckpointBuilder::new(2)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();
        let checkpoint_data2: CheckpointData = checkpoint2.into();
        let batch = tables
            .index_checkpoint(
                &checkpoint_data2,
                &mut PanicResolver,
                &rpc_config,
                /*ledger_history_enabled=*/ true,
            )
            .expect("index_checkpoint failed");
        batch.write().expect("batch write failed");
        assert!(
            tables.tx_seq_digest.safe_iter().count() > 0,
            "tx_seq_digest must have rows when ledger_history_enabled=true"
        );
    }

    /// `prune()` commits the tx_seq_digest range delete and the
    /// `Watermark::Pruned` advance in one atomic batch. After a successful
    /// prune both halves are present: no tx_seq_digest row remains in the
    /// deleted range, AND `Watermark::Pruned` is at the new value.
    #[tokio::test]
    async fn prune_commits_deletes_and_watermark_atomically() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        let mut batch = tables.tx_seq_digest.batch();
        for tx_seq in 0..4u64 {
            batch
                .insert_batch(
                    &tables.tx_seq_digest,
                    [(
                        tx_seq,
                        TxSeqDigestInfo {
                            digest: TransactionDigest::new([0; 32]),
                            event_count: 0,
                            checkpoint_number: 0,
                        },
                    )],
                )
                .unwrap();
        }
        batch.write().unwrap();

        tables
            .prune(1, 2, /*ledger_history_enabled=*/ true)
            .unwrap();

        // After the single atomic batch lands, both halves are present: the
        // deleted rows AND the advanced `Watermark::Pruned`.
        assert_eq!(tables.watermark.get(&Watermark::Pruned).unwrap(), Some(1));
        for tx_seq in 0..2u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_none());
        }
        for tx_seq in 2..4u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_some());
        }
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), Some(2));
    }

    /// Replaying a prune with an unchanged floor is a no-op: the second call
    /// returns `None` (floor did not advance) and leaves rows + watermark
    /// untouched. Covers crash-replay where the pruner re-issues the same prune.
    #[tokio::test]
    async fn prune_idempotent_replay() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        let mut batch = tables.tx_seq_digest.batch();
        for tx_seq in 0..5u64 {
            batch
                .insert_batch(
                    &tables.tx_seq_digest,
                    [(
                        tx_seq,
                        TxSeqDigestInfo {
                            digest: TransactionDigest::new([0; 32]),
                            event_count: 0,
                            checkpoint_number: 0,
                        },
                    )],
                )
                .unwrap();
        }
        batch.write().unwrap();

        assert_eq!(tables.prune(1, 3, true).unwrap(), Some(3));
        // Same floor again: nothing new to delete, floor already at 3.
        assert_eq!(tables.prune(1, 3, true).unwrap(), None);
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), Some(3));
        for tx_seq in 3..5u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_some());
        }
    }

    /// Consecutive prunes advance the floor across an existing range tombstone:
    /// the second prune derives `prev_exclusive` from `first_tx_seq_digest_key`,
    /// which must read *through* the first tombstone (only possible because the
    /// CF is opened with `ignore_range_deletions = false`).
    #[tokio::test]
    async fn prune_consecutive_ranges_advance_floor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        let tables =
            IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());

        let mut batch = tables.tx_seq_digest.batch();
        for tx_seq in 0..6u64 {
            batch
                .insert_batch(
                    &tables.tx_seq_digest,
                    [(
                        tx_seq,
                        TxSeqDigestInfo {
                            digest: TransactionDigest::new([0; 32]),
                            event_count: 0,
                            checkpoint_number: 0,
                        },
                    )],
                )
                .unwrap();
        }
        batch.write().unwrap();

        assert_eq!(tables.prune(1, 3, true).unwrap(), Some(3));
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), Some(3));
        // Second prune must see floor 3 (through the tombstone) and extend it to 5.
        assert_eq!(tables.prune(2, 5, true).unwrap(), Some(5));
        for tx_seq in 0..5u64 {
            assert!(tables.tx_seq_digest.get(&tx_seq).unwrap().is_none());
        }
        assert_eq!(tables.first_tx_seq_digest_key().unwrap(), Some(5));
    }

    /// `new_without_init` must honor an already-built ledger history DB.
    #[tokio::test]
    async fn new_without_init_enables_ledger_history_for_db_with_ledger_history_setting() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");

        // Seed a DB built with ledger history indexing.
        {
            let tables =
                IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());
            tables
                .meta
                .insert(
                    &(),
                    &MetadataInfo {
                        version: CURRENT_DB_VERSION,
                    },
                )
                .unwrap();
            tables
                .settings
                .insert(
                    &(),
                    &IndexSettings {
                        ledger_history_indexing: true,
                    },
                )
                .unwrap();
            let mut batch = tables.tx_seq_digest.batch();
            for tx_seq in 100..105u64 {
                batch
                    .insert_batch(
                        &tables.tx_seq_digest,
                        [(
                            tx_seq,
                            TxSeqDigestInfo {
                                digest: TransactionDigest::new([0; 32]),
                                event_count: 0,
                                checkpoint_number: 0,
                            },
                        )],
                    )
                    .unwrap();
            }
            batch.write().unwrap();
        }

        let store = RpcIndexStore::new_without_init(temp_dir.path());
        assert!(
            store.ledger_history_enabled,
            "new_without_init on a ledger-history DB must enable ledger history indexing"
        );
        let atomic = &store.ledger_history_pruning_watermark;
        assert_eq!(
            atomic.load(Ordering::Relaxed),
            100,
            "pruning atomic must be hydrated from the first tx_seq_digest key"
        );

        // Pruning through tx-seq floor 103 deletes rows [100, 103).
        store.prune(7, 103).unwrap();

        for tx_seq in 100..103u64 {
            assert!(store.tables.tx_seq_digest.get(&tx_seq).unwrap().is_none());
        }
        for tx_seq in 103..105u64 {
            assert!(store.tables.tx_seq_digest.get(&tx_seq).unwrap().is_some());
        }
        assert_eq!(
            store.tables.first_tx_seq_digest_key().unwrap(),
            Some(103),
            "prune must advance the derived tx-seq floor"
        );
        assert_eq!(
            atomic.load(Ordering::Relaxed),
            103,
            "prune must advance the compaction-filter atomic"
        );
    }

    /// A DB without the ledger-history setting stays disabled in `new_without_init`.
    #[tokio::test]
    async fn new_without_init_disables_ledger_history_for_db_without_ledger_history_setting() {
        // Case 1: fresh/empty DB.
        let temp_dir = tempfile::tempdir().unwrap();
        let store = RpcIndexStore::new_without_init(temp_dir.path());
        assert!(
            !store.ledger_history_enabled,
            "new_without_init on a fresh DB must leave ledger history indexing disabled"
        );

        store.prune(5, 0).unwrap();
        assert_eq!(
            store.tables.watermark.get(&Watermark::Pruned).unwrap(),
            Some(5)
        );
        assert_eq!(
            store.tables.first_tx_seq_digest_key().unwrap(),
            None,
            "disabled ledger history indexing must leave tx_seq_digest untouched"
        );

        // Case 2: a current-schema DB with no `settings` row (e.g. a pre-feature
        // DB). The missing row must read as ledger history disabled.
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rpc-index");
        {
            let tables =
                IndexStoreTables::open_with_index_options(&db_path, IndexStoreOptions::default());
            tables
                .meta
                .insert(
                    &(),
                    &MetadataInfo {
                        version: CURRENT_DB_VERSION,
                    },
                )
                .unwrap();
        }
        let store = RpcIndexStore::new_without_init(temp_dir.path());
        assert!(
            !store.ledger_history_enabled,
            "new_without_init on a DB with no settings row must leave ledger history indexing disabled"
        );
    }

    /// Parse the newest `OPTIONS-NNNNNN` file in `db_path` into a map keyed by
    /// column family name. RocksDB writes one such file on every open with a
    /// section per CF in INI-like format.
    fn parse_cf_options(db_path: &Path) -> HashMap<String, HashMap<String, String>> {
        let mut options_file: Option<(u64, PathBuf)> = None;
        for entry in std::fs::read_dir(db_path).expect("read_dir failed") {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(rest) = name.strip_prefix("OPTIONS-") else {
                continue;
            };
            // Skip transient files like `OPTIONS-NNNNNN.dbtmp`.
            let Ok(seq) = rest.parse::<u64>() else {
                continue;
            };
            if options_file.as_ref().is_none_or(|(s, _)| seq > *s) {
                options_file = Some((seq, entry.path()));
            }
        }
        let (_, path) = options_file.expect("no OPTIONS-* file written");
        let content = std::fs::read_to_string(&path).expect("read OPTIONS failed");

        let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_cf: Option<String> = None;
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("[CFOptions \"") {
                let cf_name = rest.trim_end_matches("\"]").to_string();
                current_cf = Some(cf_name);
            } else if line.starts_with('[') {
                // Any other section ends the CFOptions block.
                current_cf = None;
            } else if let Some(cf) = current_cf.as_ref()
                && let Some((k, v)) = line.split_once('=')
            {
                result
                    .entry(cf.clone())
                    .or_default()
                    .insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        result
    }
}
