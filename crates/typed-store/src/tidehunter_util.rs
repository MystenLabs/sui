// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::Options;
use serde::de::DeserializeOwned;
use tidehunter::iterators::db_iterator::DbIterator;
use typed_store_error::TypedStoreError;

pub(crate) fn apply_range_bounds(
    iterator: &mut DbIterator,
    lower_bound: Option<Vec<u8>>,
    upper_bound: Option<Vec<u8>>,
) {
    if let Some(lower_bound) = lower_bound {
        iterator.set_lower_bound(lower_bound);
    }
    if let Some(upper_bound) = upper_bound {
        iterator.set_upper_bound(upper_bound);
    }
}

pub(crate) fn transform_th_iterator<K, V>(
    iterator: impl Iterator<
        Item = Result<
            (tidehunter::minibytes::Bytes, tidehunter::minibytes::Bytes),
            tidehunter::db::DbError,
        >,
    >,
) -> impl Iterator<Item = Result<(K, V), TypedStoreError>>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    let config = bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding();
    iterator.map(move |item| {
        item.map_err(|e| TypedStoreError::RocksDBError(format!("tidehunter error {:?}", e)))
            .and_then(|(raw_key, raw_value)| {
                let key = config.deserialize(&raw_key);
                let value = bcs::from_bytes(&raw_value);
                match (key, value) {
                    (Ok(k), Ok(v)) => Ok((k, v)),
                    (Err(e), _) => Err(TypedStoreError::SerializationError(e.to_string())),
                    (_, Err(e)) => Err(TypedStoreError::SerializationError(e.to_string())),
                }
            })
    })
}

pub(crate) fn typed_store_error_from_th_error(err: tidehunter::db::DbError) -> TypedStoreError {
    TypedStoreError::RocksDBError(format!("tidehunter error: {:?}", err))
}
