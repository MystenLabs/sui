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
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldIteratorItem;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::LedgerBitmapBucket;
use sui_types::storage::LedgerBitmapBucketIter;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::RpcIndexes;

/// Local alias matching the inaccessible
/// `sui_types::storage::read_store::PackageVersionsIterator`.
type PackageVersionsIterator<'a> =
    Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + 'a>;
use sui_types::storage::error::Result as StorageResult;
use typed_store_error::TypedStoreError;

use crate::reader::RpcStoreReader;
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
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>>
    {
        use crate::schema::object_by_owner::{Key, OwnerKind};
        let map = &self.schema().object_by_owner;
        let kind = OwnerKind::AddressOwner(owner);
        // When resuming, the page token carries the cursor object's full sort
        // position -- type, balance, and id -- so the scan seeks straight to it
        // (inclusive) and stops at the end of the prefix. No post-filtering.
        let from = cursor.map(|c| Key {
            kind,
            type_: c.object_type,
            inverted_balance: c.balance.map(|b| !b),
            object_id: c.object_id,
        });
        let iter = match (object_type, &from) {
            (Some(struct_tag), Some(from)) => {
                map.iter_prefix_from(&(kind, TypeFilter::Type(struct_tag)), from)
            }
            (Some(struct_tag), None) => map.iter_prefix(&(kind, TypeFilter::Type(struct_tag))),
            (None, Some(from)) => map.iter_prefix_from(&kind, from),
            (None, None) => map.iter_prefix(&kind),
        }
        .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = iter.map(move |row| {
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
        let from = cursor.map(|c| Key {
            kind,
            type_: c.object_type,
            inverted_balance: None,
            object_id: c.field_id,
        });
        let iter = match &from {
            Some(from) => map.iter_prefix_from(&kind, from),
            None => map.iter_prefix(&kind),
        }
        .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = iter.map(move |row| {
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
        cursor: Option<u64>,
    ) -> StorageResult<PackageVersionsIterator<'_>> {
        use crate::schema::package_versions::{Key, OriginalIdPrefix};
        // The cursor passed in by `list_package_versions` is the version of the
        // first row of the next page (the previous page popped its
        // `page_size + 1`th row to derive it), so seek straight to it and
        // resume inclusively. Versions sort ascending within a package, so the
        // seek lands on the first row with `version >= cursor`.
        let map = &self.schema().package_versions;
        let iter = match cursor {
            Some(version) => map.iter_prefix_from(
                &OriginalIdPrefix(original_id),
                &Key {
                    original_id,
                    version,
                },
            ),
            None => map.iter_prefix(&OriginalIdPrefix(original_id)),
        }
        .map_err(sui_types::storage::error::Error::custom)?;
        let mapped = iter.map(move |row| {
            let (key, value) = row.map_err(to_typed_store_err)?;
            // Decode storage_id (32 bytes).
            let storage_id_bytes: [u8; 32] = (&value.into_inner().storage_id[..])
                .try_into()
                .map_err(|_| {
                    TypedStoreError::SerializationError("package_versions storage_id length".into())
                })?;
            Ok((key.version, ObjectID::new(storage_id_bytes)))
        });
        Ok(Box::new(mapped))
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
        self.min_committed(self.pipelines.iter().copied())
    }

    fn get_highest_live_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        // Only the live cohort (owned objects, types, balances); the
        // ledger-history cohort backfills independently and is excluded so a
        // node is healthy as soon as its live-object reads are caught up.
        self.highest_live_committed_checkpoint()
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
    use sui_types::base_types::ObjectID;
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
            let mut iter = reader.dynamic_field_iter(parent, cursor.take()).unwrap();
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
    fn package_versions_iter_paginates_each_version_once() {
        let (_dir, db, reader) = setup();
        let original = ObjectID::from_single_byte(0xBB);
        let versions: Vec<u64> = (1..=5).collect();

        let mut batch = db.batch();
        for &version in &versions {
            let (k, v) = crate::schema::package_versions::store(
                original,
                version,
                ObjectID::from_single_byte(version as u8),
                version,
            );
            batch
                .put(&reader.schema().package_versions, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let mut seen: Vec<u64> = Vec::new();
        let mut cursor: Option<u64> = None;
        for _ in 0..versions.len() + 2 {
            let mut iter = reader
                .package_versions_iter(original, cursor.take())
                .unwrap();
            for _ in 0..2 {
                match iter.next() {
                    Some(res) => seen.push(res.unwrap().0),
                    None => break,
                }
            }
            cursor = iter.next().transpose().unwrap().map(|(v, _)| v);
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(
            seen.len(),
            versions.len(),
            "each version yielded exactly once"
        );
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique, versions, "every version, no gaps or duplicates");
    }
}
