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
        let cursor_object_id = cursor.as_ref().map(|c| c.object_id);
        let iter = match object_type.as_ref() {
            Some(struct_tag) => {
                // `iter_objects_owned_by_address_of_type` borrows
                // its TypeFilter; build it owned and leak its
                // lifetime through the boxed iterator below.
                let filter = TypeFilter::Type(struct_tag.clone());
                let filter = Box::leak(Box::new(filter));
                self.schema()
                    .iter_objects_owned_by_address_of_type(owner, filter)
                    .map_err(sui_types::storage::error::Error::custom)?
            }
            None => self
                .schema()
                .iter_objects_owned_by_address(owner)
                .map_err(sui_types::storage::error::Error::custom)?,
        };

        let mapped = iter
            .map(move |row| {
                let (key, value) = row.map_err(to_typed_store_err)?;
                Ok(OwnedObjectInfo {
                    owner,
                    object_type: key.type_,
                    balance: key.inverted_balance.map(|b| !b),
                    object_id: key.object_id,
                    version: sui_types::base_types::SequenceNumber::from_u64(value.0),
                })
            })
            // Skip-past-cursor: drop while the row's object_id is
            // <= the cursor's. Inexact relative to the natural
            // (type, balance, id) ordering of the index, but
            // matches the validator-store contract for opaque
            // cursors.
            .skip_while(
                move |entry: &Result<OwnedObjectInfo, TypedStoreError>| match entry {
                    Ok(info) => cursor_object_id
                        .map(|c| info.object_id == c)
                        .unwrap_or(false),
                    Err(_) => false,
                },
            );

        Ok(Box::new(mapped))
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> StorageResult<Box<dyn Iterator<Item = DynamicFieldIteratorItem> + '_>> {
        // Dynamic fields are `Field<Name, Value>` objects whose
        // owner is `Owner::ObjectOwner(parent_id_as_address)`. A
        // prefix scan on `object_by_owner` with
        // `(ObjectOwner, parent)` enumerates them.
        let iter = self
            .schema()
            .iter_objects_owned_by_object(parent.into())
            .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = iter
            .map(move |row| {
                let (key, _value) = row.map_err(to_typed_store_err)?;
                Ok(DynamicFieldKey {
                    parent,
                    field_id: key.object_id,
                })
            })
            .skip_while(
                move |entry: &Result<DynamicFieldKey, TypedStoreError>| match entry {
                    Ok(info) => cursor.map(|c| info.field_id == c).unwrap_or(false),
                    Err(_) => false,
                },
            );

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
        let cursor_coin_type = cursor
            .map(|(_, tag)| move_core_types::language_storage::TypeTag::Struct(Box::new(tag)));
        let iter = self
            .schema()
            .iter_balances_owned_by(*owner)
            .map_err(sui_types::storage::error::Error::custom)?;

        let mapped = iter.filter_map(move |row| {
            let (key, value) = match row {
                Ok(pair) => pair,
                Err(e) => return Some(Err(sui_types::storage::error::Error::custom(e))),
            };
            // Project the merged proto value back into the typed
            // `Balance` view, then onto the trait's `BalanceInfo`.
            let inner = value.into_inner();
            let coin = i128::from_le_bytes((&inner.coin[..]).try_into().unwrap_or_default());
            let address = i128::from_le_bytes((&inner.address[..]).try_into().unwrap_or_default());
            // Report the coin and address halves independently; the caller
            // sums them for the total (reporting the total here would
            // double-count the address half).
            let info = BalanceInfo {
                coin_balance: coin.clamp(0, u64::MAX as i128) as u64,
                address_balance: address.clamp(0, u64::MAX as i128) as u64,
            };
            // Skip-past-cursor.
            if let Some(c) = cursor_coin_type.as_ref()
                && key.coin_type == *c
            {
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
        cursor: Option<u64>,
    ) -> StorageResult<PackageVersionsIterator<'_>> {
        let iter = self
            .schema()
            .iter_package_versions(original_id)
            .map_err(sui_types::storage::error::Error::custom)?;
        let mapped = iter
            .map(move |row| {
                let (key, value) = row.map_err(to_typed_store_err)?;
                // Decode storage_id (32 bytes).
                let storage_id_bytes: [u8; 32] = (&value.into_inner().storage_id[..])
                    .try_into()
                    .map_err(|_| {
                        TypedStoreError::SerializationError(
                            "package_versions storage_id length".into(),
                        )
                    })?;
                Ok((key.version, ObjectID::new(storage_id_bytes)))
            })
            // The cursor passed in by `list_package_versions` is
            // the version of the first row that should appear on
            // the next page — the previous page popped its
            // `page_size + 1`th row to derive this token, so we
            // want to resume *at* it (inclusive). `filter` (not
            // `skip_while`) is correct here because the
            // underlying iterator yields versions in ascending
            // order but `skip_while` would only suppress a leading
            // run that matches, leaving every earlier row in the
            // output.
            .filter(
                move |entry: &Result<(u64, ObjectID), TypedStoreError>| match entry {
                    Ok((v, _)) => cursor.map(|c| *v >= c).unwrap_or(true),
                    Err(_) => true,
                },
            );
        Ok(Box::new(mapped))
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        // Min across all registered pipeline watermarks: every CF
        // has caught up to at least this checkpoint, so reads
        // against any of them are coherent through here.
        let framework = sui_consistent_store::FrameworkSchema::new(self.db().clone());
        let mut min: Option<u64> = None;
        for entry in framework
            .watermarks
            .iter(..)
            .map_err(sui_types::storage::error::Error::custom)?
        {
            let (_, watermark) = entry.map_err(sui_types::storage::error::Error::custom)?;
            let hi = watermark.checkpoint_hi_inclusive;
            min = Some(min.map_or(hi, |m| m.min(hi)));
        }
        Ok(min)
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
        use crate::schema::keys::U64Be;
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
            dimension_key,
            bucket: end_bucket_exclusive,
        };
        if descending {
            let iter = map
                .iter_rev(lower..upper)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project_bitmap_row)))
        } else {
            let iter = map
                .iter(lower..upper)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project_bitmap_row)))
        }
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
            dimension_key,
            bucket: end_bucket_exclusive,
        };
        if descending {
            let iter = map
                .iter_rev(lower..upper)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project_event_bitmap_row)))
        } else {
            let iter = map
                .iter(lower..upper)
                .map_err(sui_types::storage::error::Error::custom)?;
            Ok(Box::new(iter.map(project_event_bitmap_row)))
        }
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

    #[test]
    fn get_coin_info_finds_metadata_and_treasury_objects() {
        use move_core_types::language_storage::StructTag;

        use crate::schema::keys::U64Varint;
        use crate::schema::object_by_type;

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
}
