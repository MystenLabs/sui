// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{marker::PhantomData, sync::Arc};

use bincode::Options;
use prometheus::{Histogram, HistogramTimer};
use rocksdb::{DBWithThreadMode, Direction, MultiThreaded};

#[cfg(not(test))]
use mysten_common::debug_fatal;

use crate::metrics::{DBMetrics, RocksDBPerfContext};

use super::TypedStoreError;
use serde::de::DeserializeOwned;

/// An iterator over all key-value pairs in a data map.
pub struct SafeIter<'a, K, V> {
    cf_name: String,
    db_iter: rocksdb::DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>,
    _phantom: PhantomData<(K, V)>,
    direction: Direction,
    is_initialized: bool,
    _timer: Option<HistogramTimer>,
    _perf_ctx: Option<RocksDBPerfContext>,
    bytes_scanned: Option<Histogram>,
    keys_scanned: Option<Histogram>,
    db_metrics: Option<Arc<DBMetrics>>,
    bytes_scanned_counter: usize,
    keys_returned_counter: usize,
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> SafeIter<'a, K, V> {
    pub(super) fn new(
        cf_name: String,
        db_iter: rocksdb::DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>,
        _timer: Option<HistogramTimer>,
        _perf_ctx: Option<RocksDBPerfContext>,
        bytes_scanned: Option<Histogram>,
        keys_scanned: Option<Histogram>,
        db_metrics: Option<Arc<DBMetrics>>,
    ) -> Self {
        Self {
            cf_name,
            db_iter,
            _phantom: PhantomData,
            direction: Direction::Forward,
            is_initialized: false,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            db_metrics,
            bytes_scanned_counter: 0,
            keys_returned_counter: 0,
        }
    }
}

impl<K: DeserializeOwned, V: DeserializeOwned> Iterator for SafeIter<'_, K, V> {
    type Item = Result<(K, V), TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (raw_key, raw_value) = match current_entry(&mut self.db_iter, &mut self.is_initialized)
        {
            Ok(Some(entry)) => entry,
            Ok(None) => return None,
            Err(e) => return Some(Err(e)),
        };
        self.bytes_scanned_counter += raw_key.len() + raw_value.len();
        self.keys_returned_counter += 1;

        let key = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .deserialize(raw_key);
        let value = bcs::from_bytes(raw_value);

        advance(&mut self.db_iter, self.direction);

        match (key, value) {
            (Ok(key), Ok(value)) => Some(Ok((key, value))),
            (key, value) => Some(Err(deserialization_error(
                &self.cf_name,
                key.err().map(|e| e.to_string()),
                value.err().map(|e| e.to_string()),
            ))),
        }
    }
}

// The helpers below do not depend on the key and value types, so they are compiled once instead
// of once per table type.

type RawIter<'a> = rocksdb::DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>;
type RawEntry<'b> = (&'b [u8], &'b [u8]);

/// Returns the entry the iterator points at, or `None` at the end of the iteration.
fn current_entry<'b>(
    db_iter: &'b mut RawIter<'_>,
    is_initialized: &mut bool,
) -> Result<Option<RawEntry<'b>>, TypedStoreError> {
    // Implicitly set iterator to the first entry in the column family if it hasn't been initialized
    // used for backward compatibility
    if !*is_initialized {
        db_iter.seek_to_first();
        *is_initialized = true;
    }
    if db_iter.valid() {
        let raw_key = db_iter.key().expect("Valid iterator failed to get key");
        let raw_value = db_iter.value().expect("Valid iterator failed to get value");
        Ok(Some((raw_key, raw_value)))
    } else {
        match db_iter.status() {
            Ok(_) => Ok(None),
            Err(err) => Err(TypedStoreError::RocksDBError(format!("{err}"))),
        }
    }
}

fn advance(db_iter: &mut RawIter<'_>, direction: Direction) {
    match direction {
        Direction::Forward => db_iter.next(),
        Direction::Reverse => db_iter.prev(),
    }
}

/// Reports key and value deserialization failures, and returns the error for the key if the key
/// failed, or else the error for the value.
#[cold]
#[inline(never)]
fn deserialization_error(
    cf_name: &str,
    key_err: Option<String>,
    value_err: Option<String>,
) -> TypedStoreError {
    #[cfg(not(test))]
    {
        if let Some(e) = &key_err {
            debug_fatal!("Failed to deserialize key in cf {}: {e}", cf_name);
        }
        if let Some(e) = &value_err {
            debug_fatal!("Failed to deserialize value in cf {}: {e}", cf_name);
        }
    }
    #[cfg(test)]
    let _ = cf_name;
    match (key_err, value_err) {
        (Some(e), _) => {
            TypedStoreError::SerializationError(format!("Failed to deserialize key: {e}"))
        }
        (None, Some(e)) => {
            TypedStoreError::SerializationError(format!("Failed to deserialize value: {e}"))
        }
        (None, None) => unreachable!("deserialization_error requires at least one error"),
    }
}

impl<K, V> Drop for SafeIter<'_, K, V> {
    fn drop(&mut self) {
        if let Some(bytes_scanned) = self.bytes_scanned.take() {
            bytes_scanned.observe(self.bytes_scanned_counter as f64);
        }
        if let Some(keys_scanned) = self.keys_scanned.take() {
            keys_scanned.observe(self.keys_returned_counter as f64);
        }
        if let Some(db_metrics) = self.db_metrics.take() {
            db_metrics
                .read_perf_ctx_metrics
                .report_metrics(&self.cf_name);
        }
    }
}

/// An iterator with a reverted direction to the original. The `RevIter`
/// is hosting an iteration which is consuming in the opposing direction.
/// It's not possible to do further manipulation (ex re-reverse) to the
/// iterator.
pub struct SafeRevIter<'a, K, V> {
    iter: SafeIter<'a, K, V>,
}

impl<'a, K, V> SafeRevIter<'a, K, V> {
    pub(crate) fn new(mut iter: SafeIter<'a, K, V>, upper_bound: Option<Vec<u8>>) -> Self {
        iter.is_initialized = true;
        iter.direction = Direction::Reverse;
        match upper_bound {
            None => iter.db_iter.seek_to_last(),
            Some(key) => iter.db_iter.seek_for_prev(&key),
        }
        Self { iter }
    }
}

impl<K: DeserializeOwned, V: DeserializeOwned> Iterator for SafeRevIter<'_, K, V> {
    type Item = Result<(K, V), TypedStoreError>;

    /// Will give the next item backwards
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
