// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod errors;
mod options;
mod rocks_util;
pub(crate) mod safe_iter;

use crate::memstore::{InMemoryBatch, InMemoryDB};
use crate::rocks::errors::typed_store_err_from_bcs_err;
use crate::rocks::errors::typed_store_err_from_rocks_err;
pub use crate::rocks::options::{
    DBMapTableConfigMap, DBOptions, ReadWriteOptions, default_db_options, read_size_from_env,
};
use crate::rocks::safe_iter::{SafeIter, SafeRevIter};
#[cfg(tidehunter)]
use crate::tidehunter_util::{
    apply_range_bounds, transform_th_iterator, transform_th_key, typed_store_error_from_th_error,
};
use crate::util::{be_fix_int_ser, iterator_bounds, iterator_bounds_with_range};
use crate::{DbIterator, TypedStoreError};
use crate::{
    metrics::{DBMetrics, RocksDBPerfContext, SamplingInterval},
    traits::{Map, TableSummary},
};
use backoff::backoff::Backoff;
use fastcrypto::hash::{Digest, HashFunction};
use mysten_common::debug_fatal;
use prometheus::{Histogram, HistogramTimer};
use rocksdb::properties::num_files_at_level;
use rocksdb::{
    AsColumnFamilyRef, ColumnFamilyDescriptor, Error, MultiThreaded, ReadOptions, WriteBatch,
    properties,
};
use rocksdb::{DBPinnableSlice, LiveFile, checkpoint::Checkpoint};
use serde::{Serialize, de::DeserializeOwned};
use std::ops::{Bound, Deref};
use std::{
    borrow::Borrow,
    marker::PhantomData,
    ops::RangeBounds,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use std::{collections::HashSet, ffi::CStr};
use sui_macros::{fail_point, nondeterministic};
#[cfg(tidehunter)]
use tidehunter::{db::Db as TideHunterDb, key_shape::KeySpace};
use tokio::sync::oneshot;
use tracing::{debug, error, instrument, warn};

// TODO: remove this after Rust rocksdb has the TOTAL_BLOB_FILES_SIZE property built-in.
// From https://github.com/facebook/rocksdb/blob/bd80433c73691031ba7baa65c16c63a83aef201a/include/rocksdb/db.h#L1169
const ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked("rocksdb.total-blob-file-size\0".as_bytes()) };

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct RocksDB {
    pub underlying: rocksdb::DBWithThreadMode<MultiThreaded>,
}

impl Drop for RocksDB {
    fn drop(&mut self) {
        self.underlying.cancel_all_background_work(/* wait */ true);
    }
}

#[derive(Clone)]
pub enum ColumnFamily {
    Rocks(String),
    InMemory(String),
    #[cfg(tidehunter)]
    TideHunter((KeySpace, Option<Vec<u8>>)),
}

impl std::fmt::Debug for ColumnFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnFamily::Rocks(name) => write!(f, "RocksDB cf: {}", name),
            ColumnFamily::InMemory(name) => write!(f, "InMemory cf: {}", name),
            #[cfg(tidehunter)]
            ColumnFamily::TideHunter(_) => write!(f, "TideHunter column family"),
        }
    }
}

impl ColumnFamily {
    fn rocks_cf<'a>(&self, rocks_db: &'a RocksDB) -> Arc<rocksdb::BoundColumnFamily<'a>> {
        match &self {
            ColumnFamily::Rocks(name) => rocks_db
                .underlying
                .cf_handle(name)
                .expect("Map-keying column family should have been checked at DB creation"),
            _ => unreachable!("invariant is checked by the caller"),
        }
    }
}

pub enum Storage {
    Rocks(RocksDB),
    InMemory(InMemoryDB),
    #[cfg(tidehunter)]
    TideHunter(Arc<TideHunterDb>),
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Storage::Rocks(db) => write!(f, "RocksDB Storage {:?}", db),
            Storage::InMemory(db) => write!(f, "InMemoryDB Storage {:?}", db),
            #[cfg(tidehunter)]
            Storage::TideHunter(_) => write!(f, "TideHunterDB Storage"),
        }
    }
}

#[derive(Debug)]
pub struct Database {
    storage: Storage,
    metric_conf: MetricConf,
}

impl Drop for Database {
    fn drop(&mut self) {
        DBMetrics::get().decrement_num_active_dbs(&self.metric_conf.db_name);
    }
}

enum GetResult<'a> {
    Rocks(DBPinnableSlice<'a>),
    InMemory(Vec<u8>),
    #[cfg(tidehunter)]
    TideHunter(tidehunter::minibytes::Bytes),
}

impl Deref for GetResult<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            GetResult::Rocks(d) => d.deref(),
            GetResult::InMemory(d) => d.deref(),
            #[cfg(tidehunter)]
            GetResult::TideHunter(d) => d.deref(),
        }
    }
}

impl Database {
    pub fn new(storage: Storage, metric_conf: MetricConf) -> Self {
        DBMetrics::get().increment_num_active_dbs(&metric_conf.db_name);
        Self {
            storage,
            metric_conf,
        }
    }

    /// Flush all memtables to SST files on disk.
    pub fn flush(&self) -> Result<(), TypedStoreError> {
        match &self.storage {
            Storage::Rocks(rocks_db) => rocks_db.underlying.flush().map_err(|e| {
                TypedStoreError::RocksDBError(format!("Failed to flush database: {}", e))
            }),
            Storage::InMemory(_) => {
                // InMemory databases don't need flushing
                Ok(())
            }
            #[cfg(tidehunter)]
            Storage::TideHunter(_) => {
                // TideHunter doesn't support an explicit flush.
                Ok(())
            }
        }
    }

    fn get<K: AsRef<[u8]>>(
        &self,
        cf: &ColumnFamily,
        key: K,
        readopts: &ReadOptions,
    ) -> Result<Option<GetResult<'_>>, TypedStoreError> {
        match (&self.storage, cf) {
            (Storage::Rocks(db), ColumnFamily::Rocks(_)) => Ok(db
                .underlying
                .get_pinned_cf_opt(&cf.rocks_cf(db), key, readopts)
                .map_err(typed_store_err_from_rocks_err)?
                .map(GetResult::Rocks)),
            (Storage::InMemory(db), ColumnFamily::InMemory(cf_name)) => {
                Ok(db.get(cf_name, key).map(GetResult::InMemory))
            }
            #[cfg(tidehunter)]
            (Storage::TideHunter(db), ColumnFamily::TideHunter((ks, prefix))) => Ok(db
                .get(*ks, &transform_th_key(key.as_ref(), prefix))
                .map_err(typed_store_error_from_th_error)?
                .map(GetResult::TideHunter)),

            _ => Err(TypedStoreError::RocksDBError(
                "typed store invariant violation".to_string(),
            )),
        }
    }

    fn multi_get<I, K>(
        &self,
        cf: &ColumnFamily,
        keys: I,
        readopts: &ReadOptions,
    ) -> Vec<Result<Option<GetResult<'_>>, TypedStoreError>>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<[u8]>,
    {
        match (&self.storage, cf) {
            (Storage::Rocks(db), ColumnFamily::Rocks(_)) => {
                let keys_vec: Vec<K> = keys.into_iter().collect();
                let res = db.underlying.batched_multi_get_cf_opt(
                    &cf.rocks_cf(db),
                    keys_vec.iter(),
                    /* sorted_input */ false,
                    readopts,
                );
                res.into_iter()
                    .map(|r| {
                        r.map_err(typed_store_err_from_rocks_err)
                            .map(|item| item.map(GetResult::Rocks))
                    })
                    .collect()
            }
            (Storage::InMemory(db), ColumnFamily::InMemory(cf_name)) => db
                .multi_get(cf_name, keys)
                .into_iter()
                .map(|r| Ok(r.map(GetResult::InMemory)))
                .collect(),
            #[cfg(tidehunter)]
            (Storage::TideHunter(db), ColumnFamily::TideHunter((ks, prefix))) => {
                let res = keys.into_iter().map(|k| {
                    db.get(*ks, &transform_th_key(k.as_ref(), prefix))
                        .map_err(typed_store_error_from_th_error)
                });
                res.into_iter()
                    .map(|r| r.map(|item| item.map(GetResult::TideHunter)))
                    .collect()
            }
            _ => unreachable!("typed store invariant violation"),
        }
    }

    pub fn drop_cf(&self, name: &str) -> Result<(), rocksdb::Error> {
        match &self.storage {
            Storage::Rocks(db) => db.underlying.drop_cf(name),
            Storage::InMemory(db) => {
                db.drop_cf(name);
                Ok(())
            }
            #[cfg(tidehunter)]
            Storage::TideHunter(_) => {
                unimplemented!("TideHunter: deletion of column family on a fly not implemented")
            }
        }
    }

    pub fn delete_file_in_range<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        from: K,
        to: K,
    ) -> Result<(), rocksdb::Error> {
        match &self.storage {
            Storage::Rocks(rocks) => rocks.underlying.delete_file_in_range_cf(cf, from, to),
            _ => unimplemented!("delete_file_in_range is only supported for rocksdb backend"),
        }
    }

    fn delete_cf<K: AsRef<[u8]>>(&self, cf: &ColumnFamily, key: K) -> Result<(), TypedStoreError> {
        fail_point!("delete-cf-before");
        let ret = match (&self.storage, cf) {
            (Storage::Rocks(db), ColumnFamily::Rocks(_)) => db
                .underlying
                .delete_cf(&cf.rocks_cf(db), key)
                .map_err(typed_store_err_from_rocks_err),
            (Storage::InMemory(db), ColumnFamily::InMemory(cf_name)) => {
                db.delete(cf_name, key.as_ref());
                Ok(())
            }
            #[cfg(tidehunter)]
            (Storage::TideHunter(db), ColumnFamily::TideHunter((ks, prefix))) => db
                .remove(*ks, transform_th_key(key.as_ref(), prefix))
                .map_err(typed_store_error_from_th_error),
            _ => Err(TypedStoreError::RocksDBError(
                "typed store invariant violation".to_string(),
            )),
        };
        fail_point!("delete-cf-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    pub fn path_for_pruning(&self) -> &Path {
        match &self.storage {
            Storage::Rocks(rocks) => rocks.underlying.path(),
            _ => unimplemented!("method is only supported for rocksdb backend"),
        }
    }

    fn put_cf(
        &self,
        cf: &ColumnFamily,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), TypedStoreError> {
        fail_point!("put-cf-before");
        let ret = match (&self.storage, cf) {
            (Storage::Rocks(db), ColumnFamily::Rocks(_)) => db
                .underlying
                .put_cf(&cf.rocks_cf(db), key, value)
                .map_err(typed_store_err_from_rocks_err),
            (Storage::InMemory(db), ColumnFamily::InMemory(cf_name)) => {
                db.put(cf_name, key, value);
                Ok(())
            }
            #[cfg(tidehunter)]
            (Storage::TideHunter(db), ColumnFamily::TideHunter((ks, prefix))) => db
                .insert(*ks, transform_th_key(&key, prefix), value)
                .map_err(typed_store_error_from_th_error),
            _ => Err(TypedStoreError::RocksDBError(
                "typed store invariant violation".to_string(),
            )),
        };
        fail_point!("put-cf-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    pub fn key_may_exist_cf<K: AsRef<[u8]>>(
        &self,
        cf_name: &str,
        key: K,
        readopts: &ReadOptions,
    ) -> bool {
        match &self.storage {
            // [`rocksdb::DBWithThreadMode::key_may_exist_cf`] can have false positives,
            // but no false negatives. We use it to short-circuit the absent case
            Storage::Rocks(rocks) => {
                rocks
                    .underlying
                    .key_may_exist_cf_opt(&rocks_cf(rocks, cf_name), key, readopts)
            }
            _ => true,
        }
    }

    pub fn write(&self, batch: StorageWriteBatch) -> Result<(), TypedStoreError> {
        self.write_opt(batch, &rocksdb::WriteOptions::default())
    }

    pub fn write_opt(
        &self,
        batch: StorageWriteBatch,
        write_options: &rocksdb::WriteOptions,
    ) -> Result<(), TypedStoreError> {
        fail_point!("batch-write-before");
        let ret = match (&self.storage, batch) {
            (Storage::Rocks(rocks), StorageWriteBatch::Rocks(batch)) => rocks
                .underlying
                .write_opt(batch, write_options)
                .map_err(typed_store_err_from_rocks_err),
            (Storage::InMemory(db), StorageWriteBatch::InMemory(batch)) => {
                // InMemory doesn't support write options
                db.write(batch);
                Ok(())
            }
            #[cfg(tidehunter)]
            (Storage::TideHunter(db), StorageWriteBatch::TideHunter(batch)) => {
                // TideHunter doesn't support write options
                db.write_batch(batch)
                    .map_err(typed_store_error_from_th_error)
            }
            _ => Err(TypedStoreError::RocksDBError(
                "using invalid batch type for the database".to_string(),
            )),
        };
        fail_point!("batch-write-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    #[cfg(tidehunter)]
    pub fn start_relocation(&self) -> anyhow::Result<()> {
        if let Storage::TideHunter(db) = &self.storage {
            db.start_relocation()?;
        }
        Ok(())
    }

    pub fn compact_range_cf<K: AsRef<[u8]>>(
        &self,
        cf_name: &str,
        start: Option<K>,
        end: Option<K>,
    ) {
        if let Storage::Rocks(rocksdb) = &self.storage {
            rocksdb
                .underlying
                .compact_range_cf(&rocks_cf(rocksdb, cf_name), start, end);
        }
    }

    pub fn checkpoint(&self, path: &Path) -> Result<(), TypedStoreError> {
        // TODO: implement for other storage types
        if let Storage::Rocks(rocks) = &self.storage {
            let checkpoint =
                Checkpoint::new(&rocks.underlying).map_err(typed_store_err_from_rocks_err)?;
            checkpoint
                .create_checkpoint(path)
                .map_err(|e| TypedStoreError::RocksDBError(e.to_string()))?;
        }
        Ok(())
    }

    pub fn get_sampling_interval(&self) -> SamplingInterval {
        self.metric_conf.read_sample_interval.new_from_self()
    }

    pub fn multiget_sampling_interval(&self) -> SamplingInterval {
        self.metric_conf.read_sample_interval.new_from_self()
    }

    pub fn write_sampling_interval(&self) -> SamplingInterval {
        self.metric_conf.write_sample_interval.new_from_self()
    }

    pub fn iter_sampling_interval(&self) -> SamplingInterval {
        self.metric_conf.iter_sample_interval.new_from_self()
    }

    fn db_name(&self) -> String {
        let name = &self.metric_conf.db_name;
        if name.is_empty() {
            "default".to_string()
        } else {
            name.clone()
        }
    }

    pub fn live_files(&self) -> Result<Vec<LiveFile>, Error> {
        match &self.storage {
            Storage::Rocks(rocks) => rocks.underlying.live_files(),
            _ => Ok(vec![]),
        }
    }
}

fn rocks_cf<'a>(rocks_db: &'a RocksDB, cf_name: &str) -> Arc<rocksdb::BoundColumnFamily<'a>> {
    rocks_db
        .underlying
        .cf_handle(cf_name)
        .expect("Map-keying column family should have been checked at DB creation")
}

fn rocks_cf_from_db<'a>(
    db: &'a Database,
    cf_name: &str,
) -> Result<Arc<rocksdb::BoundColumnFamily<'a>>, TypedStoreError> {
    match &db.storage {
        Storage::Rocks(rocksdb) => Ok(rocksdb
            .underlying
            .cf_handle(cf_name)
            .expect("Map-keying column family should have been checked at DB creation")),
        _ => Err(TypedStoreError::RocksDBError(
            "using invalid batch type for the database".to_string(),
        )),
    }
}

#[derive(Debug, Default)]
pub struct MetricConf {
    pub db_name: String,
    pub read_sample_interval: SamplingInterval,
    pub write_sample_interval: SamplingInterval,
    pub iter_sample_interval: SamplingInterval,
}

impl MetricConf {
    pub fn new(db_name: &str) -> Self {
        if db_name.is_empty() {
            error!("A meaningful db name should be used for metrics reporting.")
        }
        Self {
            db_name: db_name.to_string(),
            read_sample_interval: SamplingInterval::default(),
            write_sample_interval: SamplingInterval::default(),
            iter_sample_interval: SamplingInterval::default(),
        }
    }

    pub fn with_sampling(self, read_interval: SamplingInterval) -> Self {
        Self {
            db_name: self.db_name,
            read_sample_interval: read_interval,
            write_sample_interval: SamplingInterval::default(),
            iter_sample_interval: SamplingInterval::default(),
        }
    }
}
const CF_METRICS_REPORT_PERIOD_SECS: u64 = 30;
const METRICS_ERROR: i64 = -1;

/// An interface to a rocksDB database, keyed by a columnfamily
#[derive(Clone, Debug)]
pub struct DBMap<K, V> {
    pub db: Arc<Database>,
    _phantom: PhantomData<fn(K) -> V>,
    column_family: ColumnFamily,
    // the column family under which the map is stored
    cf: String,
    pub opts: ReadWriteOptions,
    db_metrics: Arc<DBMetrics>,
    get_sample_interval: SamplingInterval,
    multiget_sample_interval: SamplingInterval,
    write_sample_interval: SamplingInterval,
    iter_sample_interval: SamplingInterval,
    _metrics_task_cancel_handle: Arc<oneshot::Sender<()>>,
}

unsafe impl<K: Send, V: Send> Send for DBMap<K, V> {}

impl<K, V> DBMap<K, V> {
    pub(crate) fn new(
        db: Arc<Database>,
        opts: &ReadWriteOptions,
        opt_cf: &str,
        column_family: ColumnFamily,
        is_deprecated: bool,
    ) -> Self {
        let db_cloned = Arc::downgrade(&db.clone());
        let db_metrics = DBMetrics::get();
        let db_metrics_cloned = db_metrics.clone();
        let cf = opt_cf.to_string();

        let (sender, mut recv) = tokio::sync::oneshot::channel();
        if !is_deprecated && matches!(db.storage, Storage::Rocks(_)) {
            tokio::task::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_secs(CF_METRICS_REPORT_PERIOD_SECS));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Some(db) = db_cloned.upgrade() {
                                let cf = cf.clone();
                                let db_metrics = db_metrics.clone();
                                if let Err(e) = tokio::task::spawn_blocking(move || {
                                    Self::report_rocksdb_metrics(&db, &cf, &db_metrics);
                                }).await {
                                    error!("Failed to log metrics with error: {}", e);
                                }
                            }
                        }
                        _ = &mut recv => break,
                    }
                }
                debug!("Returning the cf metric logging task for DBMap: {}", &cf);
            });
        }
        DBMap {
            db: db.clone(),
            opts: opts.clone(),
            _phantom: PhantomData,
            column_family,
            cf: opt_cf.to_string(),
            db_metrics: db_metrics_cloned,
            _metrics_task_cancel_handle: Arc::new(sender),
            get_sample_interval: db.get_sampling_interval(),
            multiget_sample_interval: db.multiget_sampling_interval(),
            write_sample_interval: db.write_sampling_interval(),
            iter_sample_interval: db.iter_sampling_interval(),
        }
    }

    /// Reopens an open database as a typed map operating under a specific column family.
    /// if no column family is passed, the default column family is used.
    #[instrument(level = "debug", skip(db), err)]
    pub fn reopen(
        db: &Arc<Database>,
        opt_cf: Option<&str>,
        rw_options: &ReadWriteOptions,
        is_deprecated: bool,
    ) -> Result<Self, TypedStoreError> {
        let cf_key = opt_cf
            .unwrap_or(rocksdb::DEFAULT_COLUMN_FAMILY_NAME)
            .to_owned();
        Ok(DBMap::new(
            db.clone(),
            rw_options,
            &cf_key,
            ColumnFamily::Rocks(cf_key.to_string()),
            is_deprecated,
        ))
    }

    #[cfg(tidehunter)]
    pub fn reopen_th(
        db: Arc<Database>,
        cf_name: &str,
        ks: KeySpace,
        prefix: Option<Vec<u8>>,
    ) -> Self {
        DBMap::new(
            db,
            &ReadWriteOptions::default(),
            cf_name,
            ColumnFamily::TideHunter((ks, prefix.clone())),
            false,
        )
    }

    pub fn cf_name(&self) -> &str {
        &self.cf
    }

    pub fn batch(&self) -> DBBatch {
        let batch = match &self.db.storage {
            Storage::Rocks(_) => StorageWriteBatch::Rocks(WriteBatch::default()),
            Storage::InMemory(_) => StorageWriteBatch::InMemory(InMemoryBatch::default()),
            #[cfg(tidehunter)]
            Storage::TideHunter(_) => {
                StorageWriteBatch::TideHunter(tidehunter::batch::WriteBatch::new())
            }
        };
        DBBatch::new(
            &self.db,
            batch,
            &self.db_metrics,
            &self.write_sample_interval,
        )
    }

    pub fn flush(&self) -> Result<(), TypedStoreError> {
        self.db.flush()
    }

    pub fn compact_range<J: Serialize>(&self, start: &J, end: &J) -> Result<(), TypedStoreError> {
        let from_buf = be_fix_int_ser(start);
        let to_buf = be_fix_int_ser(end);
        self.db
            .compact_range_cf(&self.cf, Some(from_buf), Some(to_buf));
        Ok(())
    }

    pub fn compact_range_raw(
        &self,
        cf_name: &str,
        start: Vec<u8>,
        end: Vec<u8>,
    ) -> Result<(), TypedStoreError> {
        self.db.compact_range_cf(cf_name, Some(start), Some(end));
        Ok(())
    }

    /// Returns a vector of raw values corresponding to the keys provided.
    fn multi_get_pinned<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<GetResult<'_>>>, TypedStoreError>
    where
        J: Borrow<K>,
        K: Serialize,
    {
        let _timer = self
            .db_metrics
            .op_metrics
            .rocksdb_multiget_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer();
        let perf_ctx = if self.multiget_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        let keys_bytes = keys.into_iter().map(|k| be_fix_int_ser(k.borrow()));
        let results: Result<Vec<_>, TypedStoreError> = self
            .db
            .multi_get(&self.column_family, keys_bytes, &self.opts.readopts())
            .into_iter()
            .collect();
        let entries = results?;
        let entry_size = entries
            .iter()
            .flatten()
            .map(|entry| entry.len())
            .sum::<usize>();
        self.db_metrics
            .op_metrics
            .rocksdb_multiget_bytes
            .with_label_values(&[&self.cf])
            .observe(entry_size as f64);
        if perf_ctx.is_some() {
            self.db_metrics
                .read_perf_ctx_metrics
                .report_metrics(&self.cf);
        }
        Ok(entries)
    }

    fn get_rocksdb_int_property(
        rocksdb: &RocksDB,
        cf: &impl AsColumnFamilyRef,
        property_name: &std::ffi::CStr,
    ) -> Result<i64, TypedStoreError> {
        match rocksdb.underlying.property_int_value_cf(cf, property_name) {
            Ok(Some(value)) => Ok(value.min(i64::MAX as u64).try_into().unwrap_or_default()),
            Ok(None) => Ok(0),
            Err(e) => Err(TypedStoreError::RocksDBError(e.into_string())),
        }
    }

    fn report_rocksdb_metrics(
        database: &Arc<Database>,
        cf_name: &str,
        db_metrics: &Arc<DBMetrics>,
    ) {
        let Storage::Rocks(rocksdb) = &database.storage else {
            return;
        };

        let Some(cf) = rocksdb.underlying.cf_handle(cf_name) else {
            tracing::warn!(
                "unable to report metrics for cf {cf_name:?} in db {:?}",
                database.db_name()
            );
            return;
        };

        db_metrics
            .cf_metrics
            .rocksdb_total_sst_files_size
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::TOTAL_SST_FILES_SIZE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_total_blob_files_size
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(
                    rocksdb,
                    &cf,
                    ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE,
                )
                .unwrap_or(METRICS_ERROR),
            );
        // 7 is the default number of levels in RocksDB. If we ever change the number of levels using `set_num_levels`,
        // we need to update here as well. Note that there isn't an API to query the DB to get the number of levels (yet).
        let total_num_files: i64 = (0..=6)
            .map(|level| {
                Self::get_rocksdb_int_property(rocksdb, &cf, &num_files_at_level(level))
                    .unwrap_or(METRICS_ERROR)
            })
            .sum();
        db_metrics
            .cf_metrics
            .rocksdb_total_num_files
            .with_label_values(&[cf_name])
            .set(total_num_files);
        db_metrics
            .cf_metrics
            .rocksdb_num_level0_files
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, &num_files_at_level(0))
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_current_size_active_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::CUR_SIZE_ACTIVE_MEM_TABLE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_size_all_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::SIZE_ALL_MEM_TABLES)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_snapshots
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::NUM_SNAPSHOTS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_oldest_snapshot_time
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::OLDEST_SNAPSHOT_TIME)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_actual_delayed_write_rate
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::ACTUAL_DELAYED_WRITE_RATE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_is_write_stopped
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::IS_WRITE_STOPPED)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_capacity
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::BLOCK_CACHE_CAPACITY)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_usage
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::BLOCK_CACHE_USAGE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_pinned_usage
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::BLOCK_CACHE_PINNED_USAGE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_table_readers_mem
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(
                    rocksdb,
                    &cf,
                    properties::ESTIMATE_TABLE_READERS_MEM,
                )
                .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimated_num_keys
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::ESTIMATE_NUM_KEYS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_immutable_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::NUM_IMMUTABLE_MEM_TABLE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_mem_table_flush_pending
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::MEM_TABLE_FLUSH_PENDING)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_compaction_pending
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::COMPACTION_PENDING)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_pending_compaction_bytes
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(
                    rocksdb,
                    &cf,
                    properties::ESTIMATE_PENDING_COMPACTION_BYTES,
                )
                .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_running_compactions
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::NUM_RUNNING_COMPACTIONS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_running_flushes
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::NUM_RUNNING_FLUSHES)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_oldest_key_time
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::ESTIMATE_OLDEST_KEY_TIME)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_background_errors
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::BACKGROUND_ERRORS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_base_level
            .with_label_values(&[cf_name])
            .set(
                Self::get_rocksdb_int_property(rocksdb, &cf, properties::BASE_LEVEL)
                    .unwrap_or(METRICS_ERROR),
            );
    }

    pub fn checkpoint_db(&self, path: &Path) -> Result<(), TypedStoreError> {
        self.db.checkpoint(path)
    }

    pub fn table_summary(&self) -> eyre::Result<TableSummary>
    where
        K: Serialize + DeserializeOwned,
        V: Serialize + DeserializeOwned,
    {
        let mut num_keys = 0;
        let mut key_bytes_total = 0;
        let mut value_bytes_total = 0;
        let mut key_hist = hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
        let mut value_hist = hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
        for item in self.safe_iter() {
            let (key, value) = item?;
            num_keys += 1;
            let key_len = be_fix_int_ser(key.borrow()).len();
            let value_len = bcs::to_bytes(value.borrow())?.len();
            key_bytes_total += key_len;
            value_bytes_total += value_len;
            key_hist.record(key_len as u64)?;
            value_hist.record(value_len as u64)?;
        }
        Ok(TableSummary {
            num_keys,
            key_bytes_total,
            value_bytes_total,
            key_hist,
            value_hist,
        })
    }

    fn start_iter_timer(&self) -> HistogramTimer {
        self.db_metrics
            .op_metrics
            .rocksdb_iter_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer()
    }

    // Creates metrics and context for tracking an iterator usage and performance.
    fn create_iter_context(
        &self,
    ) -> (
        Option<HistogramTimer>,
        Option<Histogram>,
        Option<Histogram>,
        Option<RocksDBPerfContext>,
    ) {
        let timer = self.start_iter_timer();
        let bytes_scanned = self
            .db_metrics
            .op_metrics
            .rocksdb_iter_bytes
            .with_label_values(&[&self.cf]);
        let keys_scanned = self
            .db_metrics
            .op_metrics
            .rocksdb_iter_keys
            .with_label_values(&[&self.cf]);
        let perf_ctx = if self.iter_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        (
            Some(timer),
            Some(bytes_scanned),
            Some(keys_scanned),
            perf_ctx,
        )
    }

    /// Creates a safe reversed iterator with optional bounds.
    /// Both upper bound and lower bound are included.
    #[allow(clippy::complexity)]
    pub fn reversed_safe_iter_with_bounds(
        &self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Result<DbIterator<'_, (K, V)>, TypedStoreError>
    where
        K: Serialize + DeserializeOwned,
        V: Serialize + DeserializeOwned,
    {
        let (it_lower_bound, it_upper_bound) = iterator_bounds_with_range::<K>((
            lower_bound
                .as_ref()
                .map(Bound::Included)
                .unwrap_or(Bound::Unbounded),
            upper_bound
                .as_ref()
                .map(Bound::Included)
                .unwrap_or(Bound::Unbounded),
        ));
        match &self.db.storage {
            Storage::Rocks(db) => {
                let readopts = rocks_util::apply_range_bounds(
                    self.opts.readopts(),
                    it_lower_bound,
                    it_upper_bound,
                );
                let upper_bound_key = upper_bound.as_ref().map(|k| be_fix_int_ser(&k));
                let db_iter = db
                    .underlying
                    .raw_iterator_cf_opt(&rocks_cf(db, &self.cf), readopts);
                let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
                let iter = SafeIter::new(
                    self.cf.clone(),
                    db_iter,
                    _timer,
                    _perf_ctx,
                    bytes_scanned,
                    keys_scanned,
                    Some(self.db_metrics.clone()),
                );
                Ok(Box::new(SafeRevIter::new(iter, upper_bound_key)))
            }
            Storage::InMemory(db) => {
                Ok(db.iterator(&self.cf, it_lower_bound, it_upper_bound, true))
            }
            #[cfg(tidehunter)]
            Storage::TideHunter(db) => match &self.column_family {
                ColumnFamily::TideHunter((ks, prefix)) => {
                    let mut iter = db.iterator(*ks);
                    apply_range_bounds(&mut iter, it_lower_bound, it_upper_bound);
                    iter.reverse();
                    Ok(Box::new(transform_th_iterator(
                        iter,
                        prefix,
                        self.start_iter_timer(),
                    )))
                }
                _ => unreachable!("storage backend invariant violation"),
            },
        }
    }
}

pub enum StorageWriteBatch {
    Rocks(rocksdb::WriteBatch),
    InMemory(InMemoryBatch),
    #[cfg(tidehunter)]
    TideHunter(tidehunter::batch::WriteBatch),
}

/// Provides a mutable struct to form a collection of database write operations, and execute them.
///
/// Batching write and delete operations is faster than performing them one by one and ensures their atomicity,
///  ie. they are all written or none is.
/// This is also true of operations across column families in the same database.
///
/// Serializations / Deserialization, and naming of column families is performed by passing a DBMap<K,V>
/// with each operation.
///
/// ```
/// use typed_store::rocks::*;
/// use tempfile::tempdir;
/// use typed_store::Map;
/// use typed_store::metrics::DBMetrics;
/// use prometheus::Registry;
/// use core::fmt::Error;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
/// let rocks = open_cf_opts(tempfile::tempdir().unwrap(), None, MetricConf::default(), &[("First_CF", rocksdb::Options::default()), ("Second_CF", rocksdb::Options::default())]).unwrap();
///
/// let db_cf_1 = DBMap::reopen(&rocks, Some("First_CF"), &ReadWriteOptions::default(), false)
///     .expect("Failed to open storage");
/// let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));
///
/// let db_cf_2 = DBMap::reopen(&rocks, Some("Second_CF"), &ReadWriteOptions::default(), false)
///     .expect("Failed to open storage");
/// let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));
///
/// let mut batch = db_cf_1.batch();
/// batch
///     .insert_batch(&db_cf_1, keys_vals_1.clone())
///     .expect("Failed to batch insert")
///     .insert_batch(&db_cf_2, keys_vals_2.clone())
///     .expect("Failed to batch insert");
///
/// let _ = batch.write().expect("Failed to execute batch");
/// for (k, v) in keys_vals_1 {
///     let val = db_cf_1.get(&k).expect("Failed to get inserted key");
///     assert_eq!(Some(v), val);
/// }
///
/// for (k, v) in keys_vals_2 {
///     let val = db_cf_2.get(&k).expect("Failed to get inserted key");
///     assert_eq!(Some(v), val);
/// }
/// Ok(())
/// }
/// ```
///
pub struct DBBatch {
    database: Arc<Database>,
    batch: StorageWriteBatch,
    db_metrics: Arc<DBMetrics>,
    write_sample_interval: SamplingInterval,
}

impl DBBatch {
    /// Create a new batch associated with a DB reference.
    ///
    /// Use `open_cf` to get the DB reference or an existing open database.
    pub fn new(
        dbref: &Arc<Database>,
        batch: StorageWriteBatch,
        db_metrics: &Arc<DBMetrics>,
        write_sample_interval: &SamplingInterval,
    ) -> Self {
        DBBatch {
            database: dbref.clone(),
            batch,
            db_metrics: db_metrics.clone(),
            write_sample_interval: write_sample_interval.clone(),
        }
    }

    /// Consume the batch and write its operations to the database
    #[instrument(level = "trace", skip_all, err)]
    pub fn write(self) -> Result<(), TypedStoreError> {
        self.write_opt(&rocksdb::WriteOptions::default())
    }

    /// Consume the batch and write its operations to the database with custom write options
    #[instrument(level = "trace", skip_all, err)]
    pub fn write_opt(self, write_options: &rocksdb::WriteOptions) -> Result<(), TypedStoreError> {
        let db_name = self.database.db_name();
        let timer = self
            .db_metrics
            .op_metrics
            .rocksdb_batch_commit_latency_seconds
            .with_label_values(&[&db_name])
            .start_timer();
        let batch_size = self.size_in_bytes();

        let perf_ctx = if self.write_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        self.database.write_opt(self.batch, write_options)?;
        self.db_metrics
            .op_metrics
            .rocksdb_batch_commit_bytes
            .with_label_values(&[&db_name])
            .observe(batch_size as f64);

        if perf_ctx.is_some() {
            self.db_metrics
                .write_perf_ctx_metrics
                .report_metrics(&db_name);
        }
        let elapsed = timer.stop_and_record();
        if elapsed > 1.0 {
            warn!(?elapsed, ?db_name, "very slow batch write");
            self.db_metrics
                .op_metrics
                .rocksdb_very_slow_batch_writes_count
                .with_label_values(&[&db_name])
                .inc();
            self.db_metrics
                .op_metrics
                .rocksdb_very_slow_batch_writes_duration_ms
                .with_label_values(&[&db_name])
                .inc_by((elapsed * 1000.0) as u64);
        }
        Ok(())
    }

    pub fn size_in_bytes(&self) -> usize {
        match self.batch {
            StorageWriteBatch::Rocks(ref b) => b.size_in_bytes(),
            StorageWriteBatch::InMemory(_) => 0,
            // TODO: implement size_in_bytes method
            #[cfg(tidehunter)]
            StorageWriteBatch::TideHunter(_) => 0,
        }
    }

    pub fn delete_batch<J: Borrow<K>, K: Serialize, V>(
        &mut self,
        db: &DBMap<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<(), TypedStoreError> {
        if !Arc::ptr_eq(&db.db, &self.database) {
            return Err(TypedStoreError::CrossDBBatch);
        }

        purged_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|k| {
                let k_buf = be_fix_int_ser(k.borrow());
                match (&mut self.batch, &db.column_family) {
                    (StorageWriteBatch::Rocks(b), ColumnFamily::Rocks(name)) => {
                        b.delete_cf(&rocks_cf_from_db(&self.database, name)?, k_buf)
                    }
                    (StorageWriteBatch::InMemory(b), ColumnFamily::InMemory(name)) => {
                        b.delete_cf(name, k_buf)
                    }
                    #[cfg(tidehunter)]
                    (StorageWriteBatch::TideHunter(b), ColumnFamily::TideHunter((ks, prefix))) => {
                        b.delete(*ks, transform_th_key(&k_buf, prefix))
                    }
                    _ => Err(TypedStoreError::RocksDBError(
                        "typed store invariant violation".to_string(),
                    ))?,
                }
                Ok(())
            })?;
        Ok(())
    }

    /// Deletes a range of keys between `from` (inclusive) and `to` (non-inclusive)
    /// by writing a range delete tombstone in the db map
    /// If the DBMap is configured with ignore_range_deletions set to false,
    /// the effect of this write will be visible immediately i.e. you won't
    /// see old values when you do a lookup or scan. But if it is configured
    /// with ignore_range_deletions set to true, the old value are visible until
    /// compaction actually deletes them which will happen sometime after. By
    /// default ignore_range_deletions is set to true on a DBMap (unless it is
    /// overridden in the config), so please use this function with caution
    pub fn schedule_delete_range<K: Serialize, V>(
        &mut self,
        db: &DBMap<K, V>,
        from: &K,
        to: &K,
    ) -> Result<(), TypedStoreError> {
        if !Arc::ptr_eq(&db.db, &self.database) {
            return Err(TypedStoreError::CrossDBBatch);
        }

        let from_buf = be_fix_int_ser(from);
        let to_buf = be_fix_int_ser(to);

        if let StorageWriteBatch::Rocks(b) = &mut self.batch {
            b.delete_range_cf(
                &rocks_cf_from_db(&self.database, db.cf_name())?,
                from_buf,
                to_buf,
            );
        }
        Ok(())
    }

    /// inserts a range of (key, value) pairs given as an iterator
    pub fn insert_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &DBMap<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<&mut Self, TypedStoreError> {
        if !Arc::ptr_eq(&db.db, &self.database) {
            return Err(TypedStoreError::CrossDBBatch);
        }
        let mut total = 0usize;
        new_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|(k, v)| {
                let k_buf = be_fix_int_ser(k.borrow());
                let v_buf = bcs::to_bytes(v.borrow()).map_err(typed_store_err_from_bcs_err)?;
                total += k_buf.len() + v_buf.len();
                if db.opts.log_value_hash {
                    let key_hash = default_hash(&k_buf);
                    let value_hash = default_hash(&v_buf);
                    debug!(
                        "Insert to DB table: {:?}, key_hash: {:?}, value_hash: {:?}",
                        db.cf_name(),
                        key_hash,
                        value_hash
                    );
                }
                match (&mut self.batch, &db.column_family) {
                    (StorageWriteBatch::Rocks(b), ColumnFamily::Rocks(name)) => {
                        b.put_cf(&rocks_cf_from_db(&self.database, name)?, k_buf, v_buf)
                    }
                    (StorageWriteBatch::InMemory(b), ColumnFamily::InMemory(name)) => {
                        b.put_cf(name, k_buf, v_buf)
                    }
                    #[cfg(tidehunter)]
                    (StorageWriteBatch::TideHunter(b), ColumnFamily::TideHunter((ks, prefix))) => {
                        b.write(*ks, transform_th_key(&k_buf, prefix), v_buf.to_vec())
                    }
                    _ => Err(TypedStoreError::RocksDBError(
                        "typed store invariant violation".to_string(),
                    ))?,
                }
                Ok(())
            })?;
        self.db_metrics
            .op_metrics
            .rocksdb_batch_put_bytes
            .with_label_values(&[&db.cf])
            .observe(total as f64);
        Ok(self)
    }

    pub fn partial_merge_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &DBMap<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<&mut Self, TypedStoreError> {
        if !Arc::ptr_eq(&db.db, &self.database) {
            return Err(TypedStoreError::CrossDBBatch);
        }
        new_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|(k, v)| {
                let k_buf = be_fix_int_ser(k.borrow());
                let v_buf = bcs::to_bytes(v.borrow()).map_err(typed_store_err_from_bcs_err)?;
                match &mut self.batch {
                    StorageWriteBatch::Rocks(b) => b.merge_cf(
                        &rocks_cf_from_db(&self.database, db.cf_name())?,
                        k_buf,
                        v_buf,
                    ),
                    _ => unimplemented!("merge operator is only implemented for RocksDB"),
                }
                Ok(())
            })?;
        Ok(self)
    }
}

impl<'a, K, V> Map<'a, K, V> for DBMap<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = TypedStoreError;

    #[instrument(level = "trace", skip_all, err)]
    fn contains_key(&self, key: &K) -> Result<bool, TypedStoreError> {
        let key_buf = be_fix_int_ser(key);
        let readopts = self.opts.readopts();
        Ok(self.db.key_may_exist_cf(&self.cf, &key_buf, &readopts)
            && self
                .db
                .get(&self.column_family, &key_buf, &readopts)?
                .is_some())
    }

    #[instrument(level = "trace", skip_all, err)]
    fn multi_contains_keys<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<bool>, Self::Error>
    where
        J: Borrow<K>,
    {
        let values = self.multi_get_pinned(keys)?;
        Ok(values.into_iter().map(|v| v.is_some()).collect())
    }

    #[instrument(level = "trace", skip_all, err)]
    fn get(&self, key: &K) -> Result<Option<V>, TypedStoreError> {
        let _timer = self
            .db_metrics
            .op_metrics
            .rocksdb_get_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer();
        let perf_ctx = if self.get_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        let key_buf = be_fix_int_ser(key);
        let res = self
            .db
            .get(&self.column_family, &key_buf, &self.opts.readopts())?;
        self.db_metrics
            .op_metrics
            .rocksdb_get_bytes
            .with_label_values(&[&self.cf])
            .observe(res.as_ref().map_or(0.0, |v| v.len() as f64));
        if perf_ctx.is_some() {
            self.db_metrics
                .read_perf_ctx_metrics
                .report_metrics(&self.cf);
        }
        match res {
            Some(data) => {
                let value = bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err);
                if value.is_err() {
                    let key_hash = default_hash(&key_buf);
                    let value_hash = default_hash(&data);
                    debug_fatal!(
                        "Failed to deserialize value from DB table {:?}, key_hash: {:?}, value_hash: {:?}, error: {:?}",
                        self.cf_name(),
                        key_hash,
                        value_hash,
                        value.as_ref().err().unwrap()
                    );
                }
                Ok(Some(value?))
            }
            None => Ok(None),
        }
    }

    #[instrument(level = "trace", skip_all, err)]
    fn insert(&self, key: &K, value: &V) -> Result<(), TypedStoreError> {
        let timer = self
            .db_metrics
            .op_metrics
            .rocksdb_put_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer();
        let perf_ctx = if self.write_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        let key_buf = be_fix_int_ser(key);
        let value_buf = bcs::to_bytes(value).map_err(typed_store_err_from_bcs_err)?;
        self.db_metrics
            .op_metrics
            .rocksdb_put_bytes
            .with_label_values(&[&self.cf])
            .observe((key_buf.len() + value_buf.len()) as f64);
        if perf_ctx.is_some() {
            self.db_metrics
                .write_perf_ctx_metrics
                .report_metrics(&self.cf);
        }
        self.db.put_cf(&self.column_family, key_buf, value_buf)?;

        let elapsed = timer.stop_and_record();
        if elapsed > 1.0 {
            warn!(?elapsed, cf = ?self.cf, "very slow insert");
            self.db_metrics
                .op_metrics
                .rocksdb_very_slow_puts_count
                .with_label_values(&[&self.cf])
                .inc();
            self.db_metrics
                .op_metrics
                .rocksdb_very_slow_puts_duration_ms
                .with_label_values(&[&self.cf])
                .inc_by((elapsed * 1000.0) as u64);
        }

        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    fn remove(&self, key: &K) -> Result<(), TypedStoreError> {
        let _timer = self
            .db_metrics
            .op_metrics
            .rocksdb_delete_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer();
        let perf_ctx = if self.write_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        let key_buf = be_fix_int_ser(key);
        self.db.delete_cf(&self.column_family, key_buf)?;
        self.db_metrics
            .op_metrics
            .rocksdb_deletes
            .with_label_values(&[&self.cf])
            .inc();
        if perf_ctx.is_some() {
            self.db_metrics
                .write_perf_ctx_metrics
                .report_metrics(&self.cf);
        }
        Ok(())
    }

    /// Writes a range delete tombstone to delete all entries in the db map
    /// If the DBMap is configured with ignore_range_deletions set to false,
    /// the effect of this write will be visible immediately i.e. you won't
    /// see old values when you do a lookup or scan. But if it is configured
    /// with ignore_range_deletions set to true, the old value are visible until
    /// compaction actually deletes them which will happen sometime after. By
    /// default ignore_range_deletions is set to true on a DBMap (unless it is
    /// overridden in the config), so please use this function with caution
    #[instrument(level = "trace", skip_all, err)]
    fn schedule_delete_all(&self) -> Result<(), TypedStoreError> {
        let first_key = self.safe_iter().next().transpose()?.map(|(k, _v)| k);
        let last_key = self
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|(k, _v)| k);
        if let Some((first_key, last_key)) = first_key.zip(last_key) {
            let mut batch = self.batch();
            batch.schedule_delete_range(self, &first_key, &last_key)?;
            batch.write()?;
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.safe_iter().next().is_none()
    }

    fn safe_iter(&'a self) -> DbIterator<'a, (K, V)> {
        match &self.db.storage {
            Storage::Rocks(db) => {
                let db_iter = db
                    .underlying
                    .raw_iterator_cf_opt(&rocks_cf(db, &self.cf), self.opts.readopts());
                let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
                Box::new(SafeIter::new(
                    self.cf.clone(),
                    db_iter,
                    _timer,
                    _perf_ctx,
                    bytes_scanned,
                    keys_scanned,
                    Some(self.db_metrics.clone()),
                ))
            }
            Storage::InMemory(db) => db.iterator(&self.cf, None, None, false),
            #[cfg(tidehunter)]
            Storage::TideHunter(db) => match &self.column_family {
                ColumnFamily::TideHunter((ks, prefix)) => Box::new(transform_th_iterator(
                    db.iterator(*ks),
                    prefix,
                    self.start_iter_timer(),
                )),
                _ => unreachable!("storage backend invariant violation"),
            },
        }
    }

    fn safe_iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> DbIterator<'a, (K, V)> {
        let (lower_bound, upper_bound) = iterator_bounds(lower_bound, upper_bound);
        match &self.db.storage {
            Storage::Rocks(db) => {
                let readopts =
                    rocks_util::apply_range_bounds(self.opts.readopts(), lower_bound, upper_bound);
                let db_iter = db
                    .underlying
                    .raw_iterator_cf_opt(&rocks_cf(db, &self.cf), readopts);
                let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
                Box::new(SafeIter::new(
                    self.cf.clone(),
                    db_iter,
                    _timer,
                    _perf_ctx,
                    bytes_scanned,
                    keys_scanned,
                    Some(self.db_metrics.clone()),
                ))
            }
            Storage::InMemory(db) => db.iterator(&self.cf, lower_bound, upper_bound, false),
            #[cfg(tidehunter)]
            Storage::TideHunter(db) => match &self.column_family {
                ColumnFamily::TideHunter((ks, prefix)) => {
                    let mut iter = db.iterator(*ks);
                    apply_range_bounds(&mut iter, lower_bound, upper_bound);
                    Box::new(transform_th_iterator(iter, prefix, self.start_iter_timer()))
                }
                _ => unreachable!("storage backend invariant violation"),
            },
        }
    }

    fn safe_range_iter(&'a self, range: impl RangeBounds<K>) -> DbIterator<'a, (K, V)> {
        let (lower_bound, upper_bound) = iterator_bounds_with_range(range);
        match &self.db.storage {
            Storage::Rocks(db) => {
                let readopts =
                    rocks_util::apply_range_bounds(self.opts.readopts(), lower_bound, upper_bound);
                let db_iter = db
                    .underlying
                    .raw_iterator_cf_opt(&rocks_cf(db, &self.cf), readopts);
                let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
                Box::new(SafeIter::new(
                    self.cf.clone(),
                    db_iter,
                    _timer,
                    _perf_ctx,
                    bytes_scanned,
                    keys_scanned,
                    Some(self.db_metrics.clone()),
                ))
            }
            Storage::InMemory(db) => db.iterator(&self.cf, lower_bound, upper_bound, false),
            #[cfg(tidehunter)]
            Storage::TideHunter(db) => match &self.column_family {
                ColumnFamily::TideHunter((ks, prefix)) => {
                    let mut iter = db.iterator(*ks);
                    apply_range_bounds(&mut iter, lower_bound, upper_bound);
                    Box::new(transform_th_iterator(iter, prefix, self.start_iter_timer()))
                }
                _ => unreachable!("storage backend invariant violation"),
            },
        }
    }

    /// Returns a vector of values corresponding to the keys provided.
    #[instrument(level = "trace", skip_all, err)]
    fn multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<V>>, TypedStoreError>
    where
        J: Borrow<K>,
    {
        let results = self.multi_get_pinned(keys)?;
        let values_parsed: Result<Vec<_>, TypedStoreError> = results
            .into_iter()
            .map(|value_byte| match value_byte {
                Some(data) => Ok(Some(
                    bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err)?,
                )),
                None => Ok(None),
            })
            .collect();

        values_parsed
    }

    /// Convenience method for batch insertion
    #[instrument(level = "trace", skip_all, err)]
    fn multi_insert<J, U>(
        &self,
        key_val_pairs: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
        U: Borrow<V>,
    {
        let mut batch = self.batch();
        batch.insert_batch(self, key_val_pairs)?;
        batch.write()
    }

    /// Convenience method for batch removal
    #[instrument(level = "trace", skip_all, err)]
    fn multi_remove<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
    {
        let mut batch = self.batch();
        batch.delete_batch(self, keys)?;
        batch.write()
    }

    /// Try to catch up with primary when running as secondary
    #[instrument(level = "trace", skip_all, err)]
    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error> {
        if let Storage::Rocks(rocks) = &self.db.storage {
            rocks
                .underlying
                .try_catch_up_with_primary()
                .map_err(typed_store_err_from_rocks_err)?;
        }
        Ok(())
    }
}

/// Opens a database with options, and a number of column families with individual options that are created if they do not exist.
#[instrument(level="debug", skip_all, fields(path = ?path.as_ref()), err)]
pub fn open_cf_opts<P: AsRef<Path>>(
    path: P,
    db_options: Option<rocksdb::Options>,
    metric_conf: MetricConf,
    opt_cfs: &[(&str, rocksdb::Options)],
) -> Result<Arc<Database>, TypedStoreError> {
    let path = path.as_ref();
    // In the simulator, we intercept the wall clock in the test thread only. This causes problems
    // because rocksdb uses the simulated clock when creating its background threads, but then
    // those threads see the real wall clock (because they are not the test thread), which causes
    // rocksdb to panic. The `nondeterministic` macro evaluates expressions in new threads, which
    // resolves the issue.
    //
    // This is a no-op in non-simulator builds.

    let cfs = populate_missing_cfs(opt_cfs, path).map_err(typed_store_err_from_rocks_err)?;
    nondeterministic!({
        let mut options = db_options.unwrap_or_else(|| default_db_options().options);
        options.create_if_missing(true);
        options.create_missing_column_families(true);
        let rocksdb = {
            rocksdb::DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
                &options,
                path,
                cfs.into_iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts)),
            )
            .map_err(typed_store_err_from_rocks_err)?
        };
        Ok(Arc::new(Database::new(
            Storage::Rocks(RocksDB {
                underlying: rocksdb,
            }),
            metric_conf,
        )))
    })
}

/// Opens a database with options, and a number of column families with individual options that are created if they do not exist.
pub fn open_cf_opts_secondary<P: AsRef<Path>>(
    primary_path: P,
    secondary_path: Option<P>,
    db_options: Option<rocksdb::Options>,
    metric_conf: MetricConf,
    opt_cfs: &[(&str, rocksdb::Options)],
) -> Result<Arc<Database>, TypedStoreError> {
    let primary_path = primary_path.as_ref();
    let secondary_path = secondary_path.as_ref().map(|p| p.as_ref());
    // See comment above for explanation of why nondeterministic is necessary here.
    nondeterministic!({
        // Customize database options
        let mut options = db_options.unwrap_or_else(|| default_db_options().options);

        fdlimit::raise_fd_limit();
        // This is a requirement by RocksDB when opening as secondary
        options.set_max_open_files(-1);

        let mut opt_cfs: std::collections::HashMap<_, _> = opt_cfs.iter().cloned().collect();
        let cfs = rocksdb::DBWithThreadMode::<MultiThreaded>::list_cf(&options, primary_path)
            .ok()
            .unwrap_or_default();

        let default_db_options = default_db_options();
        // Add CFs not explicitly listed
        for cf_key in cfs.iter() {
            if !opt_cfs.contains_key(&cf_key[..]) {
                opt_cfs.insert(cf_key, default_db_options.options.clone());
            }
        }

        let primary_path = primary_path.to_path_buf();
        let secondary_path = secondary_path.map(|q| q.to_path_buf()).unwrap_or_else(|| {
            let mut s = primary_path.clone();
            s.pop();
            s.push("SECONDARY");
            s.as_path().to_path_buf()
        });

        let rocksdb = {
            options.create_if_missing(true);
            options.create_missing_column_families(true);
            let db = rocksdb::DBWithThreadMode::<MultiThreaded>::open_cf_descriptors_as_secondary(
                &options,
                &primary_path,
                &secondary_path,
                opt_cfs
                    .iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(*name, (*opts).clone())),
            )
            .map_err(typed_store_err_from_rocks_err)?;
            db.try_catch_up_with_primary()
                .map_err(typed_store_err_from_rocks_err)?;
            db
        };
        Ok(Arc::new(Database::new(
            Storage::Rocks(RocksDB {
                underlying: rocksdb,
            }),
            metric_conf,
        )))
    })
}

// Drops a database if there is no other handle to it, with retries and timeout.
#[cfg(not(tidehunter))]
pub async fn safe_drop_db(path: PathBuf, timeout: Duration) -> Result<(), rocksdb::Error> {
    let mut backoff = backoff::ExponentialBackoff {
        max_elapsed_time: Some(timeout),
        ..Default::default()
    };
    loop {
        match rocksdb::DB::destroy(&rocksdb::Options::default(), path.clone()) {
            Ok(()) => return Ok(()),
            Err(err) => match backoff.next_backoff() {
                Some(duration) => tokio::time::sleep(duration).await,
                None => return Err(err),
            },
        }
    }
}

#[cfg(tidehunter)]
pub async fn safe_drop_db(path: PathBuf, _: Duration) -> Result<(), std::io::Error> {
    std::fs::remove_dir_all(path)
}

fn populate_missing_cfs(
    input_cfs: &[(&str, rocksdb::Options)],
    path: &Path,
) -> Result<Vec<(String, rocksdb::Options)>, rocksdb::Error> {
    let mut cfs = vec![];
    let input_cf_index: HashSet<_> = input_cfs.iter().map(|(name, _)| *name).collect();
    let existing_cfs =
        rocksdb::DBWithThreadMode::<MultiThreaded>::list_cf(&rocksdb::Options::default(), path)
            .ok()
            .unwrap_or_default();

    for cf_name in existing_cfs {
        if !input_cf_index.contains(&cf_name[..]) {
            cfs.push((cf_name, rocksdb::Options::default()));
        }
    }
    cfs.extend(
        input_cfs
            .iter()
            .map(|(name, opts)| (name.to_string(), (*opts).clone())),
    );
    Ok(cfs)
}

fn default_hash(value: &[u8]) -> Digest<32> {
    let mut hasher = fastcrypto::hash::Blake2b256::default();
    hasher.update(value);
    hasher.finalize()
}
