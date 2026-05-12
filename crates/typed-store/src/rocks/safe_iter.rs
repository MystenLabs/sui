// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{marker::PhantomData, sync::Arc};

use bincode::Options;
use prometheus::{Histogram, HistogramTimer};
use rocksdb::{DBWithThreadMode, Direction, MultiThreaded};

#[cfg(not(test))]
use mysten_common::debug_fatal;

use crate::metrics::{DBMetrics, RocksDBPerfContext};
use crate::util::be_fix_int_ser;

use super::TypedStoreError;
use serde::Serialize;
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
        // Implicitly set iterator to the first entry in the column family if it hasn't been initialized
        // used for backward compatibility
        if !self.is_initialized {
            self.db_iter.seek_to_first();
            self.is_initialized = true;
        }
        if self.db_iter.valid() {
            let config = bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding();
            let raw_key = self
                .db_iter
                .key()
                .expect("Valid iterator failed to get key");
            let raw_value = self
                .db_iter
                .value()
                .expect("Valid iterator failed to get value");
            self.bytes_scanned_counter += raw_key.len() + raw_value.len();
            self.keys_returned_counter += 1;

            let key = config.deserialize(raw_key);
            let value = bcs::from_bytes(raw_value);

            match self.direction {
                Direction::Forward => self.db_iter.next(),
                Direction::Reverse => self.db_iter.prev(),
            }

            #[cfg(not(test))]
            {
                if let Err(e) = &key {
                    debug_fatal!("Failed to deserialize key in cf {}: {e}", self.cf_name);
                }
                if let Err(e) = &value {
                    debug_fatal!("Failed to deserialize value in cf {}: {e}", self.cf_name);
                }
            }

            match (key, value) {
                (Ok(key), Ok(value)) => Some(Ok((key, value))),
                (Err(e), _) => Some(Err(TypedStoreError::SerializationError(format!(
                    "Failed to deserialize key: {e}"
                )))),
                (_, Err(e)) => Some(Err(TypedStoreError::SerializationError(format!(
                    "Failed to deserialize value: {e}"
                )))),
            }
        } else {
            match self.db_iter.status() {
                Ok(_) => None,
                Err(err) => Some(Err(TypedStoreError::RocksDBError(format!("{err}")))),
            }
        }
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

/// A raw, position-controlled iterator over a column family.
///
/// Unlike `SafeIter`, this iterator never deserializes the stored value: the
/// caller drives the cursor via [`Self::seek_to_first`], [`Self::seek`],
/// [`Self::seek_for_prev`], and [`Self::next`], and reads the current row with
/// [`Self::key`] / [`Self::value`]. The value is returned as a borrowed slice
/// directly from the underlying RocksDB iterator, allowing callers to decode it
/// in place (or skip the decode entirely for tombstone-style enum values).
///
/// The iterator only supports the RocksDB storage backend; see
/// `DBMap::safe_raw_iter_with_bounds` for the construction API.
pub struct SafeRawIter<'a, K> {
    cf_name: String,
    db_iter: rocksdb::DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>,
    _phantom: PhantomData<K>,
    _timer: Option<HistogramTimer>,
    _perf_ctx: Option<RocksDBPerfContext>,
    bytes_scanned: Option<Histogram>,
    keys_scanned: Option<Histogram>,
    db_metrics: Option<Arc<DBMetrics>>,
    bytes_scanned_counter: usize,
    keys_returned_counter: usize,
}

impl<'a, K> SafeRawIter<'a, K> {
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
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            db_metrics,
            bytes_scanned_counter: 0,
            keys_returned_counter: 0,
        }
    }

    /// Position the cursor on the first row of the column family (subject to any
    /// configured iterate bounds).
    pub fn seek_to_first(&mut self) {
        self.db_iter.seek_to_first();
    }

    /// Position the cursor on the smallest key `>= key`.
    pub fn seek(&mut self, key: &K)
    where
        K: Serialize,
    {
        let key_buf = be_fix_int_ser(key);
        self.db_iter.seek(&key_buf);
    }

    /// Position the cursor on the largest key `<= key`.
    pub fn seek_for_prev(&mut self, key: &K)
    where
        K: Serialize,
    {
        let key_buf = be_fix_int_ser(key);
        self.db_iter.seek_for_prev(&key_buf);
    }

    /// Advance the cursor to the next row.
    pub fn next(&mut self) {
        self.db_iter.next();
    }

    /// Step the cursor back to the previous row.
    pub fn prev(&mut self) {
        self.db_iter.prev();
    }

    /// Whether the cursor currently points to a valid row.
    pub fn valid(&self) -> bool {
        self.db_iter.valid()
    }

    /// Deserialize and return the key at the current position. Returns `None`
    /// when the cursor is not valid.
    pub fn key(&mut self) -> Option<Result<K, TypedStoreError>>
    where
        K: DeserializeOwned,
    {
        let key_len = self.db_iter.key()?.len();
        self.bytes_scanned_counter += key_len;
        let raw_key = self
            .db_iter
            .key()
            .expect("valid since previous key() succeeded");
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        let result = config.deserialize::<K>(raw_key).map_err(|e| {
            #[cfg(not(test))]
            debug_fatal!("Failed to deserialize key in cf {}: {e}", self.cf_name);
            TypedStoreError::SerializationError(format!("Failed to deserialize key: {e}"))
        });
        Some(result)
    }

    /// Borrow the raw value bytes at the current position. Returns `None` when
    /// the cursor is not valid. The slice is valid until the cursor is moved.
    pub fn value(&mut self) -> Option<&[u8]> {
        let value_len = self.db_iter.value()?.len();
        self.bytes_scanned_counter += value_len;
        self.keys_returned_counter += 1;
        self.db_iter.value()
    }

    /// Return any error reported by the underlying RocksDB iterator.
    pub fn status(&self) -> Result<(), TypedStoreError> {
        self.db_iter
            .status()
            .map_err(|e| TypedStoreError::RocksDBError(format!("{e}")))
    }
}

impl<K> Drop for SafeRawIter<'_, K> {
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
