// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`RpcIndexes`] adapter — owned-object, dynamic-field, balance,
//! package-versions, epoch-info, ledger-history, and coin-info
//! lookups.
//!
//! Trait methods returning `Result` over [`typed_store_error::TypedStoreError`]
//! wrap our [`sui_consistent_store::error::Error`] via
//! [`TypedStoreError::RocksDBError`].

use std::ops::Bound;

use move_core_types::language_storage::StructTag;
use sui_consistent_store::Encode;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldFilter;
use sui_types::storage::DynamicFieldIteratorItem;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::IndexedPackageVersion;
use sui_types::storage::LedgerBitmapBucket;
use sui_types::storage::LedgerBitmapBucketIter;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::ObjectVersionsIterator;
use sui_types::storage::OwnedObjectFilter;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::PackagePublishKey;
use sui_types::storage::PackageVersionsIterator;
use sui_types::storage::PackagesByCheckpointIterator;
use sui_types::storage::RpcIndexes;
use sui_types::storage::error::Result as StorageResult;
use typed_store_error::TypedStoreError;

use crate::reader::RpcStoreReader;
use crate::reader::key_ranges::FilteredScan;
use crate::reader::key_ranges::dynamic_field_ranges;
use crate::reader::key_ranges::owned_object_ranges;
use crate::schema::type_filter::TypeFilter;

enum BoundedBitmapIter<'d, K, V> {
    Fwd(sui_consistent_store::Iter<'d, K, V>),
    Rev(sui_consistent_store::RevIter<'d, K, V>),
}

impl<K, V> BoundedBitmapIter<'_, K, V> {
    fn seek(&mut self, probe: impl AsRef<[u8]>) {
        match self {
            Self::Fwd(iter) => iter.seek(probe),
            Self::Rev(iter) => iter.seek(probe),
        }
    }
}

struct TransactionBitmapBucketIter<'d> {
    inner: BoundedBitmapIter<
        'd,
        crate::schema::transaction_bitmap::Key,
        crate::schema::transaction_bitmap::Value,
    >,
    dimension_key: Vec<u8>,
}

impl Iterator for TransactionBitmapBucketIter<'_> {
    type Item = Result<LedgerBitmapBucket, TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = match &mut self.inner {
            BoundedBitmapIter::Fwd(iter) => iter.next(),
            BoundedBitmapIter::Rev(iter) => iter.next(),
        };
        row.map(project_bitmap_row)
    }
}

impl LedgerBitmapBucketIter for TransactionBitmapBucketIter<'_> {
    fn seek_bucket(&mut self, bucket_id: u64) {
        let probe = crate::schema::transaction_bitmap::Key {
            dimension_key: self.dimension_key.clone(),
            bucket: bucket_id,
        }
        .encode()
        .expect("bitmap key encodes infallibly: raw bytes + u64 BE");
        self.inner.seek(probe);
    }
}

struct EventBitmapBucketIter<'d> {
    inner:
        BoundedBitmapIter<'d, crate::schema::event_bitmap::Key, crate::schema::event_bitmap::Value>,
    dimension_key: Vec<u8>,
}

impl Iterator for EventBitmapBucketIter<'_> {
    type Item = Result<LedgerBitmapBucket, TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = match &mut self.inner {
            BoundedBitmapIter::Fwd(iter) => iter.next(),
            BoundedBitmapIter::Rev(iter) => iter.next(),
        };
        row.map(project_event_bitmap_row)
    }
}

impl LedgerBitmapBucketIter for EventBitmapBucketIter<'_> {
    fn seek_bucket(&mut self, bucket_id: u64) {
        let probe = crate::schema::event_bitmap::Key {
            dimension_key: self.dimension_key.clone(),
            bucket: bucket_id,
        }
        .encode()
        .expect("bitmap key encodes infallibly: raw bytes + u64 BE");
        self.inner.seek(probe);
    }
}

fn to_typed_store_err(e: sui_consistent_store::error::Error) -> TypedStoreError {
    TypedStoreError::RocksDBError(format!("{e:#}"))
}

/// Find the object id of the first row whose Move type matches the
/// pinned `struct_tag`. The `object_by_type` index sorts by
/// `(type, id)`, so the first prefix-scan row IS the lowest-id
/// match — there should only be at most one in practice for the
/// coin-wrapper types this is used with (CoinMetadata, TreasuryCap,
/// RegulatedCoinMetadata are all unique per coin type).
fn first_object_of_type<R: Reader + Send + Sync>(
    reader: &RpcStoreReader<R>,
    struct_tag: move_core_types::language_storage::StructTag,
) -> StorageResult<Option<ObjectID>> {
    let filter = TypeFilter::Type(struct_tag);
    let mut iter = reader
        .schema()
        .iter_objects_of_type(&filter)
        .map_err(sui_types::storage::error::Error::custom)?;
    match iter.next() {
        Some(Ok((key, _value))) => Ok(Some(key.object_id)),
        Some(Err(e)) => Err(sui_types::storage::error::Error::custom(e)),
        None => Ok(None),
    }
}

impl<R: Reader + Send + Sync> RpcIndexes for RpcStoreReader<R> {
    fn get_epoch_info(
        &self,
        epoch: sui_types::committee::EpochId,
    ) -> StorageResult<Option<EpochInfo>> {
        self.schema()
            .get_epoch(epoch)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        filter: Option<&OwnedObjectFilter>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>>
    {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        let map = &self.schema().object_by_owner;
        let kind = OwnerKind::AddressOwner(owner);
        let ranges =
            owned_object_ranges(kind, filter).map_err(sui_types::storage::error::Error::custom)?;
        // When resuming, the page token carries the cursor object's full sort
        // position -- type, balance, and id -- so the scan seeks straight to it
        // (inclusive).
        let from = cursor.map(|c| Key {
            kind,
            type_: c.object_type,
            inverted_balance: c.balance.map(|b| !b),
            object_id: c.object_id,
        });
        let rows = FilteredScan::open(map, ranges, from.as_ref())
            .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = rows.map(move |row| {
            let (key, value) = row.map_err(to_typed_store_err)?;
            Ok(OwnedObjectInfo {
                owner,
                object_type: key.type_,
                balance: key.inverted_balance.map(|b| !b),
                object_id: key.object_id,
                version: sui_types::base_types::SequenceNumber::from_u64(value.0),
            })
        });

        Ok(Box::new(mapped))
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        filter: Option<&DynamicFieldFilter>,
        cursor: Option<DynamicFieldKey>,
    ) -> StorageResult<Box<dyn Iterator<Item = DynamicFieldIteratorItem> + '_>> {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        // Dynamic fields are `Field<Name, Value>` objects owned (in the
        // object-owner sense) by `parent`, so they share the `object_by_owner`
        // index with address-owned objects. The cursor carries the field's
        // type and id -- its full sort position -- so the scan seeks straight
        // to it. Field objects are never coins, so the balance component is
        // always absent.
        let map = &self.schema().object_by_owner;
        let kind = OwnerKind::ObjectOwner(parent.into());
        let ranges =
            dynamic_field_ranges(kind, filter).map_err(sui_types::storage::error::Error::custom)?;
        let from = cursor.map(|c| Key {
            kind,
            type_: c.object_type,
            inverted_balance: None,
            object_id: c.field_id,
        });
        let rows = FilteredScan::open(map, ranges, from.as_ref())
            .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = rows.map(move |row| {
            let (key, _value) = row.map_err(to_typed_store_err)?;
            Ok(DynamicFieldKey {
                parent,
                field_id: key.object_id,
                object_type: key.type_,
            })
        });

        Ok(Box::new(mapped))
    }

    fn get_coin_info(&self, coin_type: &StructTag) -> StorageResult<Option<CoinInfo>> {
        // Coin metadata / treasury cap / regulated coin metadata
        // are typed objects whose Move type wraps the requested
        // `coin_type`. Discover each via an `object_by_type`
        // prefix scan keyed on the corresponding wrapper struct
        // tag and take the first match.
        let coin_metadata_object_id = first_object_of_type(
            self,
            sui_types::coin::CoinMetadata::type_(coin_type.clone()),
        )?;
        let treasury_object_id =
            first_object_of_type(self, sui_types::coin::TreasuryCap::type_(coin_type.clone()))?;
        let regulated_coin_metadata_object_id = first_object_of_type(
            self,
            sui_types::coin::RegulatedCoinMetadata::type_(coin_type.clone()),
        )?;

        if coin_metadata_object_id.is_none()
            && treasury_object_id.is_none()
            && regulated_coin_metadata_object_id.is_none()
        {
            return Ok(None);
        }

        Ok(Some(CoinInfo {
            coin_metadata_object_id,
            treasury_object_id,
            regulated_coin_metadata_object_id,
        }))
    }

    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> StorageResult<Option<BalanceInfo>> {
        let balance = self
            .schema()
            .get_balance(*owner, coin_type.clone().into())
            .map_err(sui_types::storage::error::Error::custom)?;
        // Report the coin and address halves independently (each clamped to
        // non-negative); the caller sums them for the total. Reporting the
        // total as `coin_balance` would double-count the address half once
        // the caller adds them back together.
        Ok(balance.map(|b| BalanceInfo {
            coin_balance: b.coin.clamp(0, u64::MAX as i128) as u64,
            address_balance: b.address.clamp(0, u64::MAX as i128) as u64,
        }))
    }

    fn balance_iter(
        &self,
        owner: &SuiAddress,
        cursor: Option<(SuiAddress, StructTag)>,
    ) -> StorageResult<BalanceIterator<'_>> {
        use crate::schema::balance::{Key, OwnerPrefix};
        let map = &self.schema().balance;
        // When resuming, the page token carries the cursor's coin type -- the
        // index's sort key after the owner -- so the scan seeks straight to it
        // (inclusive) and stops at the end of the owner's balances.
        let from = cursor.map(|(_, tag)| Key {
            owner: *owner,
            coin_type: move_core_types::language_storage::TypeTag::Struct(Box::new(tag)),
        });
        let iter = match &from {
            Some(from) => map.iter_prefix_from(&OwnerPrefix(*owner), from),
            None => map.iter_prefix(&OwnerPrefix(*owner)),
        }
        .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = iter.filter_map(move |row| {
            let (key, value) = match row {
                Ok(pair) => pair,
                Err(e) => return Some(Err(sui_types::storage::error::Error::custom(e))),
            };
            // Project the merged proto value back into the typed
            // `Balance` view through the same decoder `get_balance`
            // uses, so a malformed payload surfaces as an error here
            // too rather than being silently read as zero.
            let balance = match crate::schema::balance::Balance::from_delta(&value.into_inner()) {
                Ok(b) => b,
                Err(e) => return Some(Err(sui_types::storage::error::Error::custom(e))),
            };
            // Report the coin and address halves independently; the caller
            // sums them for the total (reporting the total here would
            // double-count the address half).
            let info = BalanceInfo {
                coin_balance: balance.coin.clamp(0, u64::MAX as i128) as u64,
                address_balance: balance.address.clamp(0, u64::MAX as i128) as u64,
            };
            // Merged balance deltas that net to zero remain in RocksDB until compaction drops
            // them; hide them from listings just like deleted keys.
            if info.coin_balance == 0 && info.address_balance == 0 {
                return None;
            }
            let struct_tag = match key.coin_type {
                move_core_types::language_storage::TypeTag::Struct(b) => *b,
                _ => return None,
            };
            Some(Ok((struct_tag, info)))
        });

        Ok(Box::new(mapped))
    }

    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        start_version: u64,
        end_version_exclusive: u64,
        descending: bool,
    ) -> StorageResult<PackageVersionsIterator<'_>> {
        use crate::schema::package_versions::Key;
        if start_version >= end_version_exclusive {
            return Ok(Box::new(std::iter::empty()));
        }
        // Keys sort by `(original_id, version)` with the version big-endian, so this key range
        // holds exactly the lineage's versions in `[start_version, end_version_exclusive)`.
        let range = Key {
            original_id,
            version: start_version,
        }..Key {
            original_id,
            version: end_version_exclusive,
        };
        let map = &self.schema().package_versions;
        let to_storage_err = sui_types::storage::error::Error::custom;
        let rows: Box<dyn Iterator<Item = _>> = if descending {
            Box::new(map.iter_rev(range).map_err(to_storage_err)?)
        } else {
            Box::new(map.iter(range).map_err(to_storage_err)?)
        };
        let mapped = rows.map(move |row| {
            let (key, value) = row.map_err(to_typed_store_err)?;
            let info = value.into_inner();
            // Decode storage_id (32 bytes).
            let storage_id_bytes: [u8; 32] = (&info.storage_id[..]).try_into().map_err(|_| {
                TypedStoreError::SerializationError("package_versions storage_id length".into())
            })?;
            Ok(IndexedPackageVersion {
                version: key.version,
                storage_id: ObjectID::new(storage_id_bytes),
                checkpoint: info.checkpoint,
            })
        });
        Ok(Box::new(mapped))
    }

    fn packages_by_checkpoint_iter(
        &self,
        lower: Bound<PackagePublishKey>,
        upper: Bound<PackagePublishKey>,
        descending: bool,
    ) -> StorageResult<PackagesByCheckpointIterator<'_>> {
        use crate::schema::packages_by_checkpoint::{Key, StorageId};
        let empty = match (&lower, &upper) {
            (Bound::Included(lower), Bound::Included(upper)) => lower > upper,
            (
                Bound::Included(lower) | Bound::Excluded(lower),
                Bound::Included(upper) | Bound::Excluded(upper),
            ) => lower >= upper,
            _ => false,
        };
        if empty {
            return Ok(Box::new(std::iter::empty()));
        }
        // Keys encode `(checkpoint, original_id, version)` big-endian, so key order is
        // `PackagePublishKey` order and the typed bounds carry over unchanged.
        let range = (lower.map(Key::from), upper.map(Key::from));
        let map = &self.schema().packages_by_checkpoint;
        let to_storage_err = sui_types::storage::error::Error::custom;
        let rows: Box<dyn Iterator<Item = _>> = if descending {
            Box::new(map.iter_rev(range).map_err(to_storage_err)?)
        } else {
            Box::new(map.iter(range).map_err(to_storage_err)?)
        };
        Ok(Box::new(rows.map(|row| {
            let (key, StorageId(storage_id)) = row.map_err(to_typed_store_err)?;
            Ok((PackagePublishKey::from(key), storage_id))
        })))
    }

    fn get_package_version_storage_id(
        &self,
        original_id: ObjectID,
        version: u64,
    ) -> StorageResult<Option<ObjectID>> {
        self.schema()
            .get_package_storage_id(original_id, version)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_package_at_checkpoint(
        &self,
        original_id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
    ) -> StorageResult<Option<(u64, ObjectID)>> {
        self.schema()
            .get_package_at_checkpoint(original_id, checkpoint)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_object_version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
    ) -> StorageResult<Option<SequenceNumber>> {
        self.schema()
            .get_object_version_at_checkpoint(object_id, checkpoint)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn first_checkpoint_with_object_version(
        &self,
        object_id: ObjectID,
        checkpoints: std::ops::RangeInclusive<CheckpointSequenceNumber>,
    ) -> StorageResult<Option<(CheckpointSequenceNumber, SequenceNumber)>> {
        self.schema()
            .first_checkpoint_with_object_version(object_id, checkpoints)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn object_versions_iter(
        &self,
        object_id: ObjectID,
        versions: std::ops::RangeInclusive<SequenceNumber>,
        descending: bool,
    ) -> StorageResult<ObjectVersionsIterator<'_>> {
        let versions = self
            .schema()
            .object_versions_iter(object_id, versions, descending)
            .map_err(sui_types::storage::error::Error::custom)?;
        Ok(Box::new(versions.map(|version| {
            version.map_err(sui_types::storage::error::Error::custom)
        })))
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        // Min across the registered pipelines' watermarks: every CF
        // this reader serves has caught up to at least this
        // checkpoint, so reads against any of them are coherent
        // through here. Bounded to the registered set -- rather than
        // every row in the framework CF -- so a stale watermark left
        // behind by a pipeline that is no longer registered cannot
        // pin the reported tip forever. `None` while any registered
        // pipeline has no watermark yet: nothing is fully indexed,
        // so nothing is served (fail closed).
        self.min_watermark(self.pipelines.iter().copied(), |w| {
            w.checkpoint_hi_inclusive
        })
    }

    fn is_history_backfill_pending(&self) -> StorageResult<bool> {
        crate::history_backfill_pending(self.db()).map_err(sui_types::storage::error::Error::custom)
    }

    fn get_highest_live_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        // Only the live cohort (owned objects, types, balances); the
        // ledger-history cohort backfills independently and is excluded so a
        // node is healthy as soon as its live-object reads are caught up.
        self.highest_live_committed_checkpoint()
    }

    fn get_highest_indexed_checkpoint_timestamp_ms(&self) -> StorageResult<Option<u64>> {
        // Same pipelines as `get_highest_indexed_checkpoint_seq_number`.
        // Checkpoint timestamps never decrease, so the lowest watermark
        // timestamp is that checkpoint's, except that a watermark written
        // without a timestamp contributes 0 (see `min_watermark`).
        self.min_watermark(self.pipelines.iter().copied(), |w| {
            w.timestamp_ms_hi_inclusive
        })
    }

    fn get_highest_live_indexed_checkpoint_timestamp_ms(&self) -> StorageResult<Option<u64>> {
        self.min_watermark(crate::LIVE_COHORT.iter().copied(), |w| {
            w.timestamp_ms_hi_inclusive
        })
    }

    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> StorageResult<Option<LedgerTxSeqDigest>> {
        let meta = self
            .schema()
            .get_tx_metadata_by_seq(tx_seq)
            .map_err(sui_types::storage::error::Error::custom)?;
        Ok(meta.map(|m| LedgerTxSeqDigest {
            tx_sequence_number: tx_seq,
            digest: m.digest,
            event_count: m.event_count,
            tx_offset: m.ckpt_position,
            checkpoint_number: m.checkpoint_seq,
        }))
    }

    fn ledger_tx_seq_by_digest(&self, digest: &TransactionDigest) -> StorageResult<Option<u64>> {
        self.schema()
            .get_tx_seq_by_digest(digest)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerTxSeqDigestIterator<'_>> {
        use crate::schema::primitives::U64Be;
        let range = (
            Bound::Included(U64Be(start)),
            Bound::Excluded(U64Be(end_exclusive)),
        );
        let map = &self.schema().tx_metadata_by_seq;
        let project = move |row: Result<
            (U64Be, crate::schema::tx_metadata_by_seq::Value),
            sui_consistent_store::error::Error,
        >| {
            let (U64Be(seq), value) = row.map_err(to_typed_store_err)?;
            let stored = value.into_inner();
            let digest_bytes: [u8; 32] = (&stored.digest[..]).try_into().map_err(|_| {
                TypedStoreError::SerializationError("tx_metadata digest length".into())
            })?;
            Ok(LedgerTxSeqDigest {
                tx_sequence_number: seq,
                digest: sui_types::digests::TransactionDigest::new(digest_bytes),
                event_count: stored.event_count,
                tx_offset: stored.ckpt_position,
                checkpoint_number: stored.checkpoint_seq,
            })
        };
        if descending {
            let iter = map
                .iter_rev(range)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project)))
        } else {
            let iter = map
                .iter(range)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project)))
        }
    }

    fn transaction_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        let map = &self.schema().transaction_bitmap;
        let lower = crate::schema::transaction_bitmap::Key {
            dimension_key: dimension_key.clone(),
            bucket: start_bucket,
        };
        let upper = crate::schema::transaction_bitmap::Key {
            dimension_key: dimension_key.clone(),
            bucket: end_bucket_exclusive,
        };
        let inner = if descending {
            BoundedBitmapIter::Rev(
                map.iter_rev(lower..upper)
                    .map_err(sui_types::storage::error::Error::custom)?,
            )
        } else {
            BoundedBitmapIter::Fwd(
                map.iter(lower..upper)
                    .map_err(sui_types::storage::error::Error::custom)?,
            )
        };
        Ok(Box::new(TransactionBitmapBucketIter {
            inner,
            dimension_key,
        }))
    }

    fn event_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        let map = &self.schema().event_bitmap;
        let lower = crate::schema::event_bitmap::Key {
            dimension_key: dimension_key.clone(),
            bucket: start_bucket,
        };
        let upper = crate::schema::event_bitmap::Key {
            dimension_key: dimension_key.clone(),
            bucket: end_bucket_exclusive,
        };
        let inner = if descending {
            BoundedBitmapIter::Rev(
                map.iter_rev(lower..upper)
                    .map_err(sui_types::storage::error::Error::custom)?,
            )
        } else {
            BoundedBitmapIter::Fwd(
                map.iter(lower..upper)
                    .map_err(sui_types::storage::error::Error::custom)?,
            )
        };
        Ok(Box::new(EventBitmapBucketIter {
            inner,
            dimension_key,
        }))
    }
}

/// Project a `(transaction_bitmap::Key, Protobuf<BitmapBlob>)`
/// row into a [`LedgerBitmapBucket`], deserializing the raw
/// RoaringBitmap bytes off the protobuf payload.
fn project_bitmap_row(
    row: Result<
        (
            crate::schema::transaction_bitmap::Key,
            crate::schema::transaction_bitmap::Value,
        ),
        sui_consistent_store::error::Error,
    >,
) -> Result<sui_types::storage::LedgerBitmapBucket, TypedStoreError> {
    let (key, value) = row.map_err(to_typed_store_err)?;
    let bitmap = roaring::RoaringBitmap::deserialize_from(value.into_inner().data.as_ref())
        .map_err(|e| TypedStoreError::SerializationError(format!("RoaringBitmap: {e}")))?;
    Ok(sui_types::storage::LedgerBitmapBucket {
        bucket_id: key.bucket,
        bitmap,
    })
}

/// Project an `(event_bitmap::Key, Protobuf<BitmapBlob>)` row into
/// a [`LedgerBitmapBucket`]. Same shape as
/// [`project_bitmap_row`] but typed against the distinct event-CF
/// key.
fn project_event_bitmap_row(
    row: Result<
        (
            crate::schema::event_bitmap::Key,
            crate::schema::event_bitmap::Value,
        ),
        sui_consistent_store::error::Error,
    >,
) -> Result<sui_types::storage::LedgerBitmapBucket, TypedStoreError> {
    let (key, value) = row.map_err(to_typed_store_err)?;
    let bitmap = roaring::RoaringBitmap::deserialize_from(value.into_inner().data.as_ref())
        .map_err(|e| TypedStoreError::SerializationError(format!("RoaringBitmap: {e}")))?;
    Ok(sui_types::storage::LedgerBitmapBucket {
        bucket_id: key.bucket,
        bitmap,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
    use sui_types::base_types::ObjectID;
    use sui_types::storage::IndexedPackageVersion;
    use sui_types::storage::PackagePublishKey;
    use sui_types::storage::RpcIndexes;

    use crate::RpcStoreSchema;
    use crate::reader::RpcStoreReader;
    use crate::schema::transaction_bitmap;

    fn setup() -> (tempfile::TempDir, Db, RpcStoreReader) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        (dir, db, reader)
    }

    #[test]
    fn transaction_bitmap_bucket_iter_walks_range_ascending() {
        let (_dir, db, reader) = setup();
        let dim = b"sender:alice".to_vec();

        let mut batch = db.batch();
        for tx_seq in [
            1u64,
            transaction_bitmap::TX_BUCKET_SIZE + 5,
            3 * transaction_bitmap::TX_BUCKET_SIZE + 9,
        ] {
            let (k, v) = transaction_bitmap::store_match(dim.clone(), tx_seq);
            batch
                .merge(&reader.schema().transaction_bitmap, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let buckets: Vec<u64> = reader
            .transaction_bitmap_bucket_iter(dim.clone(), 0, 5, false)
            .unwrap()
            .map(|res| res.unwrap().bucket_id)
            .collect();
        assert_eq!(buckets, vec![0, 1, 3]);
    }

    #[test]
    fn transaction_bitmap_bucket_iter_respects_bucket_range_bounds() {
        let (_dir, db, reader) = setup();
        let dim = b"sender:alice".to_vec();

        let mut batch = db.batch();
        for tx_seq in [
            1u64,
            transaction_bitmap::TX_BUCKET_SIZE + 5,
            3 * transaction_bitmap::TX_BUCKET_SIZE + 9,
        ] {
            let (k, v) = transaction_bitmap::store_match(dim.clone(), tx_seq);
            batch
                .merge(&reader.schema().transaction_bitmap, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        // Buckets `[1, 3)` — only the middle bucket survives.
        let buckets: Vec<u64> = reader
            .transaction_bitmap_bucket_iter(dim.clone(), 1, 3, false)
            .unwrap()
            .map(|res| res.unwrap().bucket_id)
            .collect();
        assert_eq!(buckets, vec![1]);
    }

    #[test]
    fn transaction_bitmap_bucket_iter_descending_reverses_order() {
        let (_dir, db, reader) = setup();
        let dim = b"sender:alice".to_vec();

        let mut batch = db.batch();
        for tx_seq in [
            1u64,
            transaction_bitmap::TX_BUCKET_SIZE + 5,
            3 * transaction_bitmap::TX_BUCKET_SIZE + 9,
        ] {
            let (k, v) = transaction_bitmap::store_match(dim.clone(), tx_seq);
            batch
                .merge(&reader.schema().transaction_bitmap, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let buckets: Vec<u64> = reader
            .transaction_bitmap_bucket_iter(dim, 0, 5, true)
            .unwrap()
            .map(|res| res.unwrap().bucket_id)
            .collect();
        assert_eq!(buckets, vec![3, 1, 0]);
    }

    #[test]
    fn transaction_bitmap_bucket_iter_isolates_dimension() {
        let (_dir, db, reader) = setup();
        let alice = b"sender:alice".to_vec();
        let bob = b"sender:bob".to_vec();

        let mut batch = db.batch();
        let (k_a, v_a) = transaction_bitmap::store_match(alice.clone(), 1);
        let (k_b, v_b) = transaction_bitmap::store_match(bob, 1);
        batch
            .merge(&reader.schema().transaction_bitmap, &k_a, &v_a)
            .unwrap();
        batch
            .merge(&reader.schema().transaction_bitmap, &k_b, &v_b)
            .unwrap();
        batch.commit().unwrap();

        let buckets: Vec<u64> = reader
            .transaction_bitmap_bucket_iter(alice, 0, 5, false)
            .unwrap()
            .map(|res| res.unwrap().bucket_id)
            .collect();
        // Bob's bucket is not visible under Alice's dimension.
        assert_eq!(buckets, vec![0]);
    }

    /// The indexed tip is bounded by the registered pipeline set: a
    /// stale watermark from a pipeline that is no longer registered
    /// (e.g. `transactions`, disabled on the embedded deployment) must
    /// not pin the reported tip, and a registered pipeline with no
    /// watermark yet reads as "nothing fully indexed".
    #[test]
    fn highest_indexed_bounds_to_registered_pipelines() {
        use sui_consistent_store::FrameworkSchema;
        use sui_consistent_store::PipelineTaskKey;
        use sui_consistent_store::Watermark;

        let (_dir, db, reader) = setup();

        let framework = FrameworkSchema::new(db.clone());
        let stamp = |names: &[&str], hi: u64| {
            let mut batch = db.batch();
            for name in names {
                batch
                    .put(
                        &framework.watermarks,
                        &PipelineTaskKey::new(*name),
                        &Watermark::for_checkpoint(hi),
                    )
                    .unwrap();
            }
            batch.commit().unwrap();
        };

        // Nothing committed yet: nothing is fully indexed. A stale row
        // from an unregistered pipeline does not change that.
        assert_eq!(
            reader.get_highest_indexed_checkpoint_seq_number().unwrap(),
            None,
        );
        stamp(&["transactions"], 3);
        assert_eq!(
            reader.get_highest_indexed_checkpoint_seq_number().unwrap(),
            None,
        );

        // Only part of the registered set committed: still nothing
        // fully indexed (fail closed).
        stamp(crate::LIVE_COHORT, 40);
        assert_eq!(
            reader.get_highest_indexed_checkpoint_seq_number().unwrap(),
            None,
        );

        // The whole registered set committed: the slowest registered
        // pipeline bounds the tip, and the stale `transactions` row at
        // 3 stays excluded.
        stamp(crate::HISTORY_COHORT, 40);
        stamp(&[crate::HISTORY_COHORT[0]], 25);
        assert_eq!(
            reader.get_highest_indexed_checkpoint_seq_number().unwrap(),
            Some(25),
        );

        // A reader for a different deployment bounds by exactly its
        // own registered set.
        let custom = reader.clone().with_pipelines(["transactions"]);
        assert_eq!(
            custom.get_highest_indexed_checkpoint_seq_number().unwrap(),
            Some(3),
        );
    }

    /// The indexed-tip timestamps come from the watermarks' chain
    /// timestamps, not their checkpoint numbers: the live timestamp
    /// is bounded by the live cohort alone, the all-pipelines one by
    /// the slowest registered pipeline, and either is `None` while a
    /// pipeline it covers has no watermark.
    #[test]
    fn highest_indexed_timestamps_follow_watermarks() {
        use sui_consistent_store::FrameworkSchema;
        use sui_consistent_store::PipelineTaskKey;
        use sui_consistent_store::Watermark;

        let (_dir, db, reader) = setup();

        let framework = FrameworkSchema::new(db.clone());
        let stamp = |names: &[&str], hi: u64, timestamp_ms: u64| {
            let mut batch = db.batch();
            for name in names {
                batch
                    .put(
                        &framework.watermarks,
                        &PipelineTaskKey::new(*name),
                        &Watermark::for_checkpoint(hi).with_timestamp_ms_hi_inclusive(timestamp_ms),
                    )
                    .unwrap();
            }
            batch.commit().unwrap();
        };

        // Live cohort committed, history not yet seeded: the live
        // timestamp is known, the all-pipelines one is not.
        stamp(crate::LIVE_COHORT, 40, 1_700_000_040_000);
        assert_eq!(
            reader
                .get_highest_live_indexed_checkpoint_timestamp_ms()
                .unwrap(),
            Some(1_700_000_040_000),
        );
        assert_eq!(
            reader
                .get_highest_indexed_checkpoint_timestamp_ms()
                .unwrap(),
            None,
        );

        // History backfilling behind the live frontier: the slowest
        // history pipeline bounds the all-pipelines timestamp, and the
        // live timestamp is unaffected.
        stamp(crate::HISTORY_COHORT, 40, 1_700_000_040_000);
        stamp(&[crate::HISTORY_COHORT[0]], 25, 1_700_000_025_000);
        assert_eq!(
            reader
                .get_highest_indexed_checkpoint_timestamp_ms()
                .unwrap(),
            Some(1_700_000_025_000),
        );
        assert_eq!(
            reader
                .get_highest_live_indexed_checkpoint_timestamp_ms()
                .unwrap(),
            Some(1_700_000_040_000),
        );
    }

    #[test]
    fn transaction_bitmap_bucket_iter_seeks_within_range() {
        let (_dir, db, reader) = setup();
        let dimension_key = b"sender:alice".to_vec();
        let mut batch = db.batch();
        for bucket in [0, 3, 7] {
            let (key, value) = transaction_bitmap::store_match(
                dimension_key.clone(),
                bucket * transaction_bitmap::TX_BUCKET_SIZE + 1,
            );
            batch
                .merge(&reader.schema().transaction_bitmap, &key, &value)
                .unwrap();
        }
        batch.commit().unwrap();

        let mut iter = reader
            .transaction_bitmap_bucket_iter(dimension_key, 0, 8, false)
            .unwrap();
        iter.seek_bucket(2);
        assert_eq!(iter.next().unwrap().unwrap().bucket_id, 3);
        iter.seek_bucket(7);
        assert_eq!(iter.next().unwrap().unwrap().bucket_id, 7);
        iter.seek_bucket(9);
        assert!(iter.next().is_none());
    }

    #[test]
    fn transaction_bitmap_bucket_iter_seeks_descending() {
        let (_dir, db, reader) = setup();
        let dimension_key = b"sender:alice".to_vec();
        let mut batch = db.batch();
        for bucket in [0, 3, 7] {
            let (key, value) = transaction_bitmap::store_match(
                dimension_key.clone(),
                bucket * transaction_bitmap::TX_BUCKET_SIZE + 1,
            );
            batch
                .merge(&reader.schema().transaction_bitmap, &key, &value)
                .unwrap();
        }
        batch.commit().unwrap();

        let mut iter = reader
            .transaction_bitmap_bucket_iter(dimension_key, 0, 8, true)
            .unwrap();
        iter.seek_bucket(5);
        assert_eq!(iter.next().unwrap().unwrap().bucket_id, 3);
        iter.seek_bucket(0);
        assert_eq!(iter.next().unwrap().unwrap().bucket_id, 0);
    }

    #[test]
    fn bitmap_bucket_iter_seek_respects_dimension_isolation() {
        let (_dir, db, reader) = setup();
        let alice = b"sender:alice".to_vec();
        let bob = b"sender:bob".to_vec();
        let mut batch = db.batch();
        for (dimension_key, bucket) in [(&alice, 0), (&alice, 7), (&bob, 3), (&bob, 7)] {
            let (key, value) = transaction_bitmap::store_match(
                dimension_key.clone(),
                bucket * transaction_bitmap::TX_BUCKET_SIZE + 1,
            );
            batch
                .merge(&reader.schema().transaction_bitmap, &key, &value)
                .unwrap();
        }
        batch.commit().unwrap();

        let mut iter = reader
            .transaction_bitmap_bucket_iter(alice, 0, 8, false)
            .unwrap();
        iter.seek_bucket(3);
        assert_eq!(iter.next().unwrap().unwrap().bucket_id, 7);
        assert!(iter.next().is_none());
    }

    #[test]
    fn get_coin_info_finds_metadata_and_treasury_objects() {
        use move_core_types::language_storage::StructTag;

        use crate::schema::object_by_type;
        use crate::schema::primitives::U64Varint;

        let (_dir, db, reader) = setup();

        // Construct a synthetic coin type and seed
        // `object_by_type` rows for its CoinMetadata and
        // TreasuryCap wrappers. The real on-chain pipeline writes
        // these rows for every Move object it sees; the test
        // bypasses pipelines and writes the rows directly.
        let coin_type = StructTag {
            address: move_core_types::account_address::AccountAddress::new([2u8; 32]),
            module: move_core_types::identifier::Identifier::new("sui").unwrap(),
            name: move_core_types::identifier::Identifier::new("SUI").unwrap(),
            type_params: vec![],
        };
        let metadata_type = sui_types::coin::CoinMetadata::type_(coin_type.clone());
        let treasury_type = sui_types::coin::TreasuryCap::type_(coin_type.clone());

        let metadata_object_id = ObjectID::from_single_byte(0xA1);
        let treasury_object_id = ObjectID::from_single_byte(0xA2);

        let mut batch = db.batch();
        batch
            .put(
                &reader.schema().object_by_type,
                &object_by_type::Key {
                    type_: metadata_type,
                    object_id: metadata_object_id,
                },
                &U64Varint(1),
            )
            .unwrap();
        batch
            .put(
                &reader.schema().object_by_type,
                &object_by_type::Key {
                    type_: treasury_type,
                    object_id: treasury_object_id,
                },
                &U64Varint(1),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = reader
            .get_coin_info(&coin_type)
            .unwrap()
            .expect("coin info present");
        assert_eq!(info.coin_metadata_object_id, Some(metadata_object_id));
        assert_eq!(info.treasury_object_id, Some(treasury_object_id));
        assert_eq!(info.regulated_coin_metadata_object_id, None);
    }

    #[test]
    fn get_coin_info_returns_none_when_no_wrappers_indexed() {
        use move_core_types::language_storage::StructTag;

        let (_dir, _db, reader) = setup();

        let coin_type = StructTag {
            address: move_core_types::account_address::AccountAddress::new([3u8; 32]),
            module: move_core_types::identifier::Identifier::new("custom").unwrap(),
            name: move_core_types::identifier::Identifier::new("COIN").unwrap(),
            type_params: vec![],
        };
        assert!(reader.get_coin_info(&coin_type).unwrap().is_none());
    }

    #[test]
    fn transaction_bitmap_bucket_iter_returns_decoded_bitmap() {
        let (_dir, db, reader) = setup();
        let dim = b"sender:alice".to_vec();

        let mut batch = db.batch();
        for tx_seq in [1u64, 17, 256] {
            let (k, v) = transaction_bitmap::store_match(dim.clone(), tx_seq);
            batch
                .merge(&reader.schema().transaction_bitmap, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let first = reader
            .transaction_bitmap_bucket_iter(dim, 0, 1, false)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let bits: Vec<u32> = first.bitmap.iter().collect();
        assert_eq!(bits, vec![1, 17, 256]);
    }

    // Pagination regression coverage for the cursor-skip iterators. The
    // `list_*` handlers page by taking `page_size` rows and then peeking the
    // next row as the opaque page token; a broken skip-past-cursor predicate
    // re-yields earlier rows on every page (duplicates, and a cursor that never
    // advances). Each test walks every page and asserts each row appears once.

    fn a_type() -> move_core_types::language_storage::StructTag {
        move_core_types::language_storage::StructTag {
            address: move_core_types::account_address::AccountAddress::TWO,
            module: move_core_types::identifier::Identifier::new("m").unwrap(),
            name: move_core_types::identifier::Identifier::new("T").unwrap(),
            type_params: vec![],
        }
    }

    #[test]
    fn owned_objects_iter_paginates_each_object_once() {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        use crate::schema::primitives::U64Varint;
        use sui_types::base_types::SuiAddress;

        let (_dir, db, reader) = setup();
        let owner = SuiAddress::ZERO;
        let ids: Vec<ObjectID> = (1u8..=5).map(ObjectID::from_single_byte).collect();

        let mut batch = db.batch();
        for id in &ids {
            batch
                .put(
                    &reader.schema().object_by_owner,
                    &Key {
                        kind: OwnerKind::AddressOwner(owner),
                        type_: a_type(),
                        inverted_balance: None,
                        object_id: *id,
                    },
                    &U64Varint(1),
                )
                .unwrap();
        }
        batch.commit().unwrap();

        let mut seen = Vec::new();
        let mut cursor = None;
        // Bounded so a non-advancing cursor surfaces as a failed assertion
        // rather than an infinite loop.
        for _ in 0..ids.len() + 2 {
            let mut iter = reader
                .owned_objects_iter(owner, None, cursor.take())
                .unwrap();
            for _ in 0..2 {
                match iter.next() {
                    Some(res) => seen.push(res.unwrap().object_id),
                    None => break,
                }
            }
            cursor = iter.next().transpose().unwrap();
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(seen.len(), ids.len(), "each object yielded exactly once");
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique, ids, "every object, no gaps or duplicates");
    }

    #[test]
    fn dynamic_field_iter_paginates_each_field_once() {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        use crate::schema::primitives::U64Varint;

        let (_dir, db, reader) = setup();
        let parent = ObjectID::from_single_byte(0xAA);
        let fields: Vec<ObjectID> = (1u8..=5).map(ObjectID::from_single_byte).collect();

        let mut batch = db.batch();
        for id in &fields {
            batch
                .put(
                    &reader.schema().object_by_owner,
                    &Key {
                        kind: OwnerKind::ObjectOwner(parent.into()),
                        type_: a_type(),
                        inverted_balance: None,
                        object_id: *id,
                    },
                    &U64Varint(1),
                )
                .unwrap();
        }
        batch.commit().unwrap();

        let mut seen = Vec::new();
        let mut cursor = None;
        for _ in 0..fields.len() + 2 {
            let mut iter = reader
                .dynamic_field_iter(parent, None, cursor.take())
                .unwrap();
            for _ in 0..2 {
                match iter.next() {
                    Some(res) => seen.push(res.unwrap().field_id),
                    None => break,
                }
            }
            cursor = iter.next().transpose().unwrap();
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(seen.len(), fields.len(), "each field yielded exactly once");
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique, fields, "every field, no gaps or duplicates");
    }

    /// Pages through `owner`'s objects that `filter` matches, `page_size` at a
    /// time, resuming from the peeked next row as the handlers do.
    fn page_owned_objects(
        reader: &RpcStoreReader,
        owner: sui_types::base_types::SuiAddress,
        filter: &sui_types::storage::OwnedObjectFilter,
        page_size: usize,
    ) -> Vec<ObjectID> {
        let mut seen = Vec::new();
        let mut cursor = None;
        for _ in 0..100 {
            let mut iter = reader
                .owned_objects_iter(owner, Some(filter), cursor.take())
                .unwrap();
            seen.extend(
                iter.by_ref()
                    .take(page_size)
                    .map(|res| res.unwrap().object_id),
            );
            cursor = iter.next().transpose().unwrap();
            if cursor.is_none() {
                return seen;
            }
        }
        panic!("pagination did not terminate");
    }

    #[test]
    fn owned_objects_iter_excludes_negated_types_across_pages() {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        use crate::schema::primitives::U64Varint;
        use sui_types::base_types::SuiAddress;
        use sui_types::storage::{FilterLiteral, OwnedObjectFilter};

        let (_dir, db, reader) = setup();
        let owner = SuiAddress::from(ObjectID::from_single_byte(0x10));
        let neighbour = SuiAddress::from(ObjectID::from_single_byte(0x11));
        let tag = |s: &str| sui_types::parse_sui_struct_tag(s).unwrap();

        let mut batch = db.batch();
        let mut put = |owner, type_: &str, balance: Option<u64>, id: u8| {
            batch
                .put(
                    &reader.schema().object_by_owner,
                    &Key {
                        kind: OwnerKind::AddressOwner(owner),
                        type_: tag(type_),
                        inverted_balance: balance.map(|b| !b),
                        object_id: ObjectID::from_single_byte(id),
                    },
                    &U64Varint(1),
                )
                .unwrap();
        };
        // Kept objects sort before and after the coins.
        put(owner, "0x1::a::A", None, 1);
        for id in 10..60 {
            put(owner, "0x2::coin::Coin<0x2::sui::SUI>", Some(id as u64), id);
        }
        put(owner, "0x2::coin::CoinMetadata<0x2::sui::SUI>", None, 2);
        for id in 60..70 {
            put(
                owner,
                "0x2::coin::Coin<0xa::usdc::USDC>",
                Some(id as u64),
                id,
            );
        }
        put(owner, "0xff::z::Z", None, 3);
        put(neighbour, "0x1::a::A", None, 4);
        batch.commit().unwrap();

        let not_coin = OwnedObjectFilter {
            terms: vec![vec![FilterLiteral {
                negated: true,
                predicate: "0x2::coin::Coin".parse().unwrap(),
            }]],
        };
        let expected: Vec<ObjectID> = [1, 2, 3].map(ObjectID::from_single_byte).to_vec();
        assert_eq!(page_owned_objects(&reader, owner, &not_coin, 1), expected);
        assert_eq!(page_owned_objects(&reader, owner, &not_coin, 2), expected);

        let usdc = OwnedObjectFilter::object_type(tag("0x2::coin::Coin<0xa::usdc::USDC>"));
        // Richest coin first.
        let expected: Vec<ObjectID> = (60..70).rev().map(ObjectID::from_single_byte).collect();
        assert_eq!(page_owned_objects(&reader, owner, &usdc, 3), expected);

        let literal = |negated, s: &str| FilterLiteral {
            negated,
            predicate: s.parse().unwrap(),
        };
        let either_end = OwnedObjectFilter {
            terms: vec![
                vec![literal(false, "0xff::z::Z")],
                vec![literal(false, "0x1::a")],
            ],
        };
        let expected: Vec<ObjectID> = [1, 3].map(ObjectID::from_single_byte).to_vec();
        assert_eq!(page_owned_objects(&reader, owner, &either_end, 1), expected);
        let contradiction = OwnedObjectFilter {
            terms: vec![vec![
                literal(false, "0x2::coin::Coin"),
                literal(true, "0x2::coin::Coin"),
            ]],
        };
        assert_eq!(
            page_owned_objects(&reader, owner, &contradiction, 1),
            vec![]
        );

        // A cursor past the filter's last range, e.g. a token from an
        // unfiltered listing resumed under a narrower filter, yields nothing.
        let last_object = reader
            .owned_objects_iter(owner, None, None)
            .unwrap()
            .last()
            .unwrap()
            .unwrap();
        assert_eq!(last_object.object_id, ObjectID::from_single_byte(3));
        let only_a = OwnedObjectFilter::object_type(tag("0x1::a::A"));
        assert_eq!(
            reader
                .owned_objects_iter(owner, Some(&only_a), Some(last_object))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn dynamic_field_iter_matches_unwrapped_name_types_across_pages() {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        use crate::schema::primitives::U64Varint;
        use move_core_types::language_storage::TypeTag;
        use sui_types::dynamic_field::DynamicFieldInfo;
        use sui_types::storage::{DynamicFieldFilter, DynamicFieldPredicate, FilterLiteral};

        let (_dir, db, reader) = setup();
        let parent = ObjectID::from_single_byte(0xAA);
        let type_tag = |s: &str| sui_types::parse_sui_type_tag(s).unwrap();
        let dynamic_field =
            |name: &str| DynamicFieldInfo::dynamic_field_type(type_tag(name), TypeTag::U64);
        let dynamic_object_field = |name: &str| {
            DynamicFieldInfo::dynamic_field_type(
                TypeTag::Struct(Box::new(DynamicFieldInfo::dynamic_object_field_wrapper(
                    type_tag(name),
                ))),
                type_tag("0x2::object::ID"),
            )
        };

        let fields = [
            (dynamic_object_field("0xabc::m::Key"), 1),
            (dynamic_object_field("0xabc::m::Key"), 2),
            (dynamic_object_field("0x2::kiosk::Item"), 3),
            (dynamic_object_field("0x2::kiosk::Item"), 4),
            (dynamic_field("0x2::kiosk::Listing"), 5),
            (dynamic_field("u64"), 6),
            // The value object of a dynamic object field, owned by its `Field`.
            (
                sui_types::parse_sui_struct_tag("0x2::coin::Coin<0x2::sui::SUI>").unwrap(),
                7,
            ),
        ];
        let mut batch = db.batch();
        for (type_, id) in &fields {
            batch
                .put(
                    &reader.schema().object_by_owner,
                    &Key {
                        kind: OwnerKind::ObjectOwner(parent.into()),
                        type_: type_.clone(),
                        inverted_balance: None,
                        object_id: ObjectID::from_single_byte(*id),
                    },
                    &U64Varint(1),
                )
                .unwrap();
        }
        batch.commit().unwrap();

        let page = |filter: &DynamicFieldFilter| {
            let mut seen = Vec::new();
            let mut cursor = None;
            for _ in 0..100 {
                let mut iter = reader
                    .dynamic_field_iter(parent, Some(filter), cursor.take())
                    .unwrap();
                seen.extend(iter.next().map(|res| res.unwrap().field_id));
                cursor = iter.next().transpose().unwrap();
                if cursor.is_none() {
                    let mut ids: Vec<u8> =
                        seen.iter().map(|id: &ObjectID| id.as_ref()[31]).collect();
                    ids.sort();
                    return ids;
                }
            }
            panic!("pagination did not terminate");
        };
        let name = |s: &str| DynamicFieldFilter {
            terms: vec![vec![FilterLiteral {
                negated: false,
                predicate: DynamicFieldPredicate::NameType(s.parse().unwrap()),
            }]],
        };

        // Every dynamic object field's `Wrapper` is at `0x2`; the filter
        // matches the wrapped name type, so the `0xabc` names stay out.
        assert_eq!(page(&name("0x2")), vec![3, 4, 5]);
        assert_eq!(page(&name("0x2::kiosk::Item")), vec![3, 4]);
        assert_eq!(page(&name("u64")), vec![6]);
        let inline_values = DynamicFieldFilter {
            terms: vec![vec![FilterLiteral {
                negated: true,
                predicate: DynamicFieldPredicate::DynamicObjectField,
            }]],
        };
        assert_eq!(page(&inline_values), vec![5, 6]);
    }

    #[test]
    fn balance_iter_paginates_each_coin_type_once() {
        use crate::schema::balance::coin_delta;
        use move_core_types::language_storage::{StructTag, TypeTag};
        use sui_types::base_types::SuiAddress;

        let coin_type = |name: &str| -> TypeTag {
            TypeTag::Struct(Box::new(StructTag {
                address: move_core_types::account_address::AccountAddress::TWO,
                module: move_core_types::identifier::Identifier::new("coin").unwrap(),
                name: move_core_types::identifier::Identifier::new(name).unwrap(),
                type_params: vec![],
            }))
        };

        let (_dir, db, reader) = setup();
        let owner = SuiAddress::ZERO;
        let names = ["c0", "c1", "c2", "c3", "c4"];

        let mut batch = db.batch();
        for name in names {
            let (k, v) = coin_delta(owner, coin_type(name), 100);
            batch.merge(&reader.schema().balance, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let mut seen: Vec<StructTag> = Vec::new();
        let mut cursor: Option<(SuiAddress, StructTag)> = None;
        for _ in 0..names.len() + 2 {
            let mut iter = reader.balance_iter(&owner, cursor.take()).unwrap();
            for _ in 0..2 {
                match iter.next() {
                    Some(res) => seen.push(res.unwrap().0),
                    None => break,
                }
            }
            cursor = iter
                .next()
                .transpose()
                .unwrap()
                .map(|(tag, _)| (owner, tag));
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(
            seen.len(),
            names.len(),
            "each coin type yielded exactly once"
        );
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "no duplicate coin types");
    }

    #[test]
    fn balance_iter_skips_zero_sum_merged_rows() {
        use crate::schema::balance::{address_delta, coin_delta};
        use move_core_types::language_storage::{StructTag, TypeTag};
        use sui_types::base_types::SuiAddress;

        let tag = |name: &str| -> StructTag {
            StructTag {
                address: move_core_types::account_address::AccountAddress::TWO,
                module: move_core_types::identifier::Identifier::new("coin").unwrap(),
                name: move_core_types::identifier::Identifier::new(name).unwrap(),
                type_params: vec![],
            }
        };

        let (_dir, db, reader) = setup();
        let owner = SuiAddress::ZERO;
        let a = tag("A");
        let b = tag("B");
        let c = tag("C");
        let d = tag("D");
        let e = tag("E");

        let mut batch = db.batch();
        for (t, v) in [(&a, 100i128), (&b, 50), (&d, 200)] {
            let (k, delta) = coin_delta(owner, TypeTag::Struct(Box::new(t.clone())), v);
            batch.merge(&reader.schema().balance, &k, &delta).unwrap();
        }
        for (t, v) in [(&c, 75i128), (&e, 30)] {
            let (k, delta) = address_delta(owner, TypeTag::Struct(Box::new(t.clone())), v);
            batch.merge(&reader.schema().balance, &k, &delta).unwrap();
        }
        batch.commit().unwrap();

        // Cancel B, C (between A and D) and E (trailing) to zero in a second uncompacted batch.
        let mut cancel = db.batch();
        let (kb, vb) = coin_delta(owner, TypeTag::Struct(Box::new(b.clone())), -50);
        cancel.merge(&reader.schema().balance, &kb, &vb).unwrap();
        for (t, v) in [(&c, -75i128), (&e, -30)] {
            let (k, delta) = address_delta(owner, TypeTag::Struct(Box::new(t.clone())), v);
            cancel.merge(&reader.schema().balance, &k, &delta).unwrap();
        }
        cancel.commit().unwrap();

        let all: Vec<_> = reader
            .balance_iter(&owner, None)
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, a);
        assert_eq!(all[0].1.coin_balance, 100);
        assert_eq!(all[1].0, d);
        assert_eq!(all[1].1.coin_balance, 200);

        // Resuming from a cursor pointing at a zero-sum row skips directly to D, and
        // lookahead after D stops without surfacing trailing E.
        let resumed: Vec<_> = reader
            .balance_iter(&owner, Some((owner, b)))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(resumed, vec![d.clone()]);

        let mut page1 = reader.balance_iter(&owner, None).unwrap();
        assert_eq!(page1.next().unwrap().unwrap().0, a);
        let next_cursor = page1.next().unwrap().unwrap().0;
        assert_eq!(next_cursor, d);

        let mut page2 = reader
            .balance_iter(&owner, Some((owner, next_cursor)))
            .unwrap();
        assert_eq!(page2.next().unwrap().unwrap().0, d);
        assert!(page2.next().is_none());
    }

    #[test]
    fn package_versions_iter_lists_version_range_in_either_direction() {
        let (_dir, db, reader) = setup();
        let original = ObjectID::from_single_byte(0xBB);

        // The lineages on either side of `original` in key order must never leak into its scans.
        let mut batch = db.batch();
        for (lineage, versions) in [
            (ObjectID::from_single_byte(0xBA), 1..=3),
            (original, 1..=5),
            (ObjectID::from_single_byte(0xBC), 1..=3),
        ] {
            for version in versions {
                let (k, v) = crate::schema::package_versions::store(
                    lineage,
                    version,
                    ObjectID::from_single_byte(version as u8),
                    version,
                );
                batch
                    .put(&reader.schema().package_versions, &k, &v)
                    .unwrap();
            }
        }
        batch.commit().unwrap();

        for (start, end, descending, expected) in [
            (0, u64::MAX, false, vec![1, 2, 3, 4, 5]),
            (0, u64::MAX, true, vec![5, 4, 3, 2, 1]),
            (2, 4, false, vec![2, 3]),
            (2, 4, true, vec![3, 2]),
            (5, u64::MAX, true, vec![5]),
            (0, 1, false, vec![]),
            (6, u64::MAX, true, vec![]),
            (3, 3, false, vec![]),
            (4, 2, true, vec![]),
        ] {
            let versions: Vec<u64> = reader
                .package_versions_iter(original, start, end, descending)
                .unwrap()
                .map(|entry| entry.unwrap().version)
                .collect();
            assert_eq!(
                versions, expected,
                "[{start}, {end}) descending={descending}"
            );
        }
    }

    #[test]
    fn package_versions_iter_reports_publish_checkpoint_unset_for_restored_rows() {
        let (_dir, db, reader) = setup();
        let original = ObjectID::from_single_byte(0xBD);
        let s1 = ObjectID::from_single_byte(0x01);
        let s2 = ObjectID::from_single_byte(0x02);

        let mut batch = db.batch();
        let (k, v) = crate::schema::package_versions::store_restored(original, 1, s1);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        let (k, v) = crate::schema::package_versions::store(original, 2, s2, 42);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        batch.commit().unwrap();

        let entries: Vec<IndexedPackageVersion> = reader
            .package_versions_iter(original, 0, u64::MAX, false)
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            entries,
            vec![
                IndexedPackageVersion {
                    version: 1,
                    storage_id: s1,
                    checkpoint: None,
                },
                IndexedPackageVersion {
                    version: 2,
                    storage_id: s2,
                    checkpoint: Some(42),
                },
            ]
        );
    }

    #[test]
    fn packages_by_checkpoint_iter_lists_key_range_in_either_direction() {
        use std::ops::Bound::{Excluded, Included, Unbounded};

        let (_dir, db, reader) = setup();
        let key = |checkpoint, original_id, version| PackagePublishKey {
            checkpoint,
            original_id,
            version,
        };
        let a = ObjectID::from_single_byte(0x0A);
        let b = ObjectID::from_single_byte(0x0B);
        let c = ObjectID::from_single_byte(0x0C);
        let system = SUI_FRAMEWORK_PACKAGE_ID;
        // In key order: two packages in checkpoint 5, two in-place upgrades of a system package
        // (one storage id) in checkpoints 7 and 9, an upgrade of `a` sharing checkpoint 9, and a
        // last package in checkpoint 12.
        let publishes = vec![
            (key(5, a, 1), a),
            (key(5, b, 1), b),
            (key(7, system, 2), system),
            (key(9, system, 3), system),
            (key(9, a, 2), ObjectID::from_single_byte(0xA2)),
            (key(12, c, 1), c),
        ];
        let mut batch = db.batch();
        for (key, storage_id) in &publishes {
            let (k, v) = crate::schema::packages_by_checkpoint::store(
                key.checkpoint,
                key.original_id,
                key.version,
                *storage_id,
            );
            batch
                .put(&reader.schema().packages_by_checkpoint, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let list = |lower, upper, descending| -> Vec<(PackagePublishKey, ObjectID)> {
            reader
                .packages_by_checkpoint_iter(lower, upper, descending)
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        let reversed = |items: &[(PackagePublishKey, ObjectID)]| {
            items.iter().rev().copied().collect::<Vec<_>>()
        };

        assert_eq!(list(Unbounded, Unbounded, false), publishes);
        assert_eq!(list(Unbounded, Unbounded, true), reversed(&publishes));

        // Checkpoints 7 through 9, from the lowest possible key at 7 to the highest at 9.
        let checkpoints = (
            Included(key(7, ObjectID::ZERO, 0)),
            Included(key(9, ObjectID::MAX, u64::MAX)),
        );
        assert_eq!(
            list(checkpoints.0, checkpoints.1, false),
            publishes[2..5].to_vec()
        );
        assert_eq!(
            list(checkpoints.0, checkpoints.1, true),
            reversed(&publishes[2..5])
        );

        // Bounds at keys inside checkpoints 5 and 9, as cursors set them, list the same versions
        // in either direction.
        let (after, before) = (publishes[1].0, publishes[4].0);
        assert_eq!(
            list(Excluded(after), Excluded(before), false),
            publishes[2..4].to_vec()
        );
        assert_eq!(
            list(Excluded(after), Excluded(before), true),
            reversed(&publishes[2..4])
        );
        assert_eq!(
            list(Included(after), Included(before), true),
            reversed(&publishes[1..5])
        );

        // Empty and inverted ranges list nothing.
        assert!(list(Excluded(after), Included(after), false).is_empty());
        assert!(list(Included(before), Excluded(after), true).is_empty());
        assert!(list(Included(before), Included(after), false).is_empty());
        assert!(list(Excluded(publishes[5].0), Unbounded, false).is_empty());
    }

    #[test]
    fn package_lookups_resolve_exact_version_and_checkpoint_bound() {
        let (_dir, db, reader) = setup();
        let original = ObjectID::from_single_byte(0xCC);
        let s1 = ObjectID::from_single_byte(0x01);
        let s2 = ObjectID::from_single_byte(0x02);

        let mut batch = db.batch();
        let (k, v) = crate::schema::package_versions::store(original, 1, s1, 1);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        let (k, v) = crate::schema::package_versions::store(original, 2, s2, 10);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            reader.get_package_version_storage_id(original, 1).unwrap(),
            Some(s1),
        );
        assert_eq!(
            reader.get_package_version_storage_id(original, 2).unwrap(),
            Some(s2),
        );
        assert_eq!(
            reader.get_package_version_storage_id(original, 3).unwrap(),
            None,
        );

        assert_eq!(reader.get_package_at_checkpoint(original, 0).unwrap(), None);
        assert_eq!(
            reader.get_package_at_checkpoint(original, 1).unwrap(),
            Some((1, s1)),
        );
        assert_eq!(
            reader.get_package_at_checkpoint(original, 9).unwrap(),
            Some((1, s1)),
        );
        assert_eq!(
            reader.get_package_at_checkpoint(original, 10).unwrap(),
            Some((2, s2)),
        );
        assert_eq!(
            reader
                .get_package_at_checkpoint(original, u64::MAX)
                .unwrap(),
            Some((2, s2)),
        );

        let unknown = ObjectID::from_single_byte(0xDD);
        assert_eq!(
            reader.get_package_version_storage_id(unknown, 1).unwrap(),
            None,
        );
        assert_eq!(reader.get_package_at_checkpoint(unknown, 5).unwrap(), None);

        // A restore-floor row (no recorded publish checkpoint) counts
        // as always existed: before any real publish it is the only
        // version, and it is superseded by the first real publish.
        let floor_original = ObjectID::from_single_byte(0xEE);
        let floor_storage = ObjectID::from_single_byte(0x03);
        let mut batch = db.batch();
        let (k, v) =
            crate::schema::package_versions::store_restored(floor_original, 1, floor_storage);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        let (k, v) = crate::schema::package_versions::store(floor_original, 2, s2, 10);
        batch
            .put(&reader.schema().package_versions, &k, &v)
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            reader.get_package_at_checkpoint(floor_original, 0).unwrap(),
            Some((1, floor_storage)),
        );
        assert_eq!(
            reader
                .get_package_at_checkpoint(floor_original, 10)
                .unwrap(),
            Some((2, s2)),
        );
    }
}
