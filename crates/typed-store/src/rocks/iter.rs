// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::marker::PhantomData;
use std::sync::Arc;

use bincode::Options;
use prometheus::{Histogram, HistogramTimer};
use rocksdb::Direction;

use super::RocksDBRawIter;
use crate::metrics::RocksDBPerfContext;
use crate::DBMetrics;
use serde::de::DeserializeOwned;

/// An iterator over all key-value pairs in a data map.
pub struct Iter<'a, K, V> {
    cf_name: String,
    db_iter: RocksDBRawIter<'a>,
    // *const here is an equivalent to `impl !Send for Iter` (which is not a stable feature at the moment)
    _phantom: PhantomData<*const (K, V)>,
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

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iter<'a, K, V> {
    pub(super) fn new(
        cf_name: String,
        db_iter: RocksDBRawIter<'a>,
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

impl<K: DeserializeOwned, V: DeserializeOwned> Iterator for Iter<'_, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        // implicitly set iterator to the first entry in the column family if it hasn't been initialized
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
            let key = config.deserialize(raw_key).ok();
            let value = bcs::from_bytes(raw_value).ok();
            match self.direction {
                Direction::Forward => self.db_iter.next(),
                Direction::Reverse => self.db_iter.prev(),
            }
            key.and_then(|k| value.map(|v| (k, v)))
        } else {
            None
        }
    }
}

impl<K, V> Drop for Iter<'_, K, V> {
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
