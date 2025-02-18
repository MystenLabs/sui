// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod errors;
pub(crate) mod iter;
pub(crate) mod keys;
pub(crate) mod safe_iter;
pub(crate) mod values;

use self::{iter::Iter, keys::Keys, values::Values};
use crate::rocks::errors::typed_store_err_from_bcs_err;
use crate::rocks::errors::typed_store_err_from_bincode_err;
use crate::rocks::errors::typed_store_err_from_rocks_err;
use crate::rocks::safe_iter::SafeIter;
use crate::TypedStoreError;
use crate::{
    metrics::{DBMetrics, RocksDBPerfContext, SamplingInterval},
    traits::{Map, TableSummary},
};
use bincode::Options;
use collectable::TryExtend;
use itertools::Itertools;
use prometheus::{Histogram, HistogramTimer};
use rocksdb::properties::num_files_at_level;
use rocksdb::{
    checkpoint::Checkpoint, BlockBasedOptions, BottommostLevelCompaction, Cache, CompactOptions,
    DBPinnableSlice, LiveFile, OptimisticTransactionDB, SnapshotWithThreadMode,
};
use rocksdb::{
    properties, AsColumnFamilyRef, CStrLike, ColumnFamilyDescriptor, DBWithThreadMode, Error,
    ErrorKind, IteratorMode, MultiThreaded, OptimisticTransactionOptions, ReadOptions, Transaction,
    WriteBatch, WriteBatchWithTransaction, WriteOptions,
};
use serde::{de::DeserializeOwned, Serialize};
use std::ops::Bound;
use std::{
    borrow::Borrow,
    collections::BTreeMap,
    env,
    marker::PhantomData,
    ops::RangeBounds,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use std::{collections::HashSet, ffi::CStr};
use sui_macros::{fail_point, nondeterministic};
use tap::TapFallible;
use tokio::sync::oneshot;
use tracing::{debug, error, info, instrument, warn};

// Write buffer size per RocksDB instance can be set via the env var below.
// If the env var is not set, use the default value in MiB.
const ENV_VAR_DB_WRITE_BUFFER_SIZE: &str = "DB_WRITE_BUFFER_SIZE_MB";
const DEFAULT_DB_WRITE_BUFFER_SIZE: usize = 1024;

// Write ahead log size per RocksDB instance can be set via the env var below.
// If the env var is not set, use the default value in MiB.
const ENV_VAR_DB_WAL_SIZE: &str = "DB_WAL_SIZE_MB";
const DEFAULT_DB_WAL_SIZE: usize = 1024;

// Environment variable to control behavior of write throughput optimized tables.
const ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER: &str = "L0_NUM_FILES_COMPACTION_TRIGGER";
const DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER: usize = 4;
const DEFAULT_UNIVERSAL_COMPACTION_L0_NUM_FILES_COMPACTION_TRIGGER: usize = 80;
const ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB: &str = "MAX_WRITE_BUFFER_SIZE_MB";
const DEFAULT_MAX_WRITE_BUFFER_SIZE_MB: usize = 256;
const ENV_VAR_MAX_WRITE_BUFFER_NUMBER: &str = "MAX_WRITE_BUFFER_NUMBER";
const DEFAULT_MAX_WRITE_BUFFER_NUMBER: usize = 6;
const ENV_VAR_TARGET_FILE_SIZE_BASE_MB: &str = "TARGET_FILE_SIZE_BASE_MB";
const DEFAULT_TARGET_FILE_SIZE_BASE_MB: usize = 128;

// Set to 1 to disable blob storage for transactions and effects.
const ENV_VAR_DISABLE_BLOB_STORAGE: &str = "DISABLE_BLOB_STORAGE";

const ENV_VAR_DB_PARALLELISM: &str = "DB_PARALLELISM";

// TODO: remove this after Rust rocksdb has the TOTAL_BLOB_FILES_SIZE property built-in.
// From https://github.com/facebook/rocksdb/blob/bd80433c73691031ba7baa65c16c63a83aef201a/include/rocksdb/db.h#L1169
const ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked("rocksdb.total-blob-file-size\0".as_bytes()) };

#[cfg(test)]
mod tests;

/// A helper macro to reopen multiple column families. The macro returns
/// a tuple of DBMap structs in the same order that the column families
/// are defined.
///
/// # Arguments
///
/// * `db` - a reference to a rocks DB object
/// * `cf;<ty,ty>` - a comma separated list of column families to open. For each column family a
///     concatenation of column family name (cf) and Key-Value <ty, ty> should be provided.
///
/// # Examples
///
/// We successfully open two different column families.
/// ```
/// use typed_store::reopen;
/// use typed_store::rocks::*;
/// use tempfile::tempdir;
/// use prometheus::Registry;
/// use std::sync::Arc;
/// use typed_store::metrics::DBMetrics;
/// use core::fmt::Error;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
/// const FIRST_CF: &str = "First_CF";
/// const SECOND_CF: &str = "Second_CF";
///
///
/// /// Create the rocks database reference for the desired column families
/// let rocks = open_cf(tempdir().unwrap(), None, MetricConf::default(), &[FIRST_CF, SECOND_CF]).unwrap();
///
/// /// Now simply open all the column families for their expected Key-Value types
/// let (db_map_1, db_map_2) = reopen!(&rocks, FIRST_CF;<i32, String>, SECOND_CF;<i32, String>);
/// Ok(())
/// }
/// ```
///
#[macro_export]
macro_rules! reopen {
    ( $db:expr, $($cf:expr;<$K:ty, $V:ty>),*) => {
        (
            $(
                DBMap::<$K, $V>::reopen($db, Some($cf), &ReadWriteOptions::default(), false).expect(&format!("Cannot open {} CF.", $cf)[..])
            ),*
        )
    };
}

/// Repeatedly attempt an Optimistic Transaction until it succeeds.
/// Since many callsites (e.g. the consensus handler) cannot proceed in the case of failed writes,
/// this will loop forever until the transaction succeeds.
#[macro_export]
macro_rules! retry_transaction {
    ($transaction:expr) => {
        retry_transaction!($transaction, Some(20))
    };

    (
        $transaction:expr,
        $max_retries:expr // should be an Option<int type>, None for unlimited
        $(,)?

    ) => {{
        use rand::{
            distributions::{Distribution, Uniform},
            rngs::ThreadRng,
        };
        use std::time::Duration;
        use tracing::{error, info};

        let mut retries = 0;
        let max_retries = $max_retries;
        loop {
            let status = $transaction;
            match status {
                Err(TypedStoreError::RetryableTransactionError) => {
                    retries += 1;
                    // Randomized delay to help racing transactions get out of each other's way.
                    let delay = {
                        let mut rng = ThreadRng::default();
                        Duration::from_millis(Uniform::new(0, 50).sample(&mut rng))
                    };
                    if let Some(max_retries) = max_retries {
                        if retries > max_retries {
                            error!(?max_retries, "max retries exceeded");
                            break status;
                        }
                    }
                    if retries > 10 {
                        // TODO: monitoring needed?
                        error!(?delay, ?retries, "excessive transaction retries...");
                    } else {
                        info!(
                            ?delay,
                            ?retries,
                            "transaction write conflict detected, sleeping"
                        );
                    }
                    std::thread::sleep(delay);
                }
                _ => break status,
            }
        }
    }};
}

#[macro_export]
macro_rules! retry_transaction_forever {
    ($transaction:expr) => {
        $crate::retry_transaction!($transaction, None)
    };
}

#[derive(Debug)]
pub struct DBWithThreadModeWrapper {
    pub underlying: rocksdb::DBWithThreadMode<MultiThreaded>,
    pub metric_conf: MetricConf,
    pub db_path: PathBuf,
}

impl DBWithThreadModeWrapper {
    fn new(
        underlying: rocksdb::DBWithThreadMode<MultiThreaded>,
        metric_conf: MetricConf,
        db_path: PathBuf,
    ) -> Self {
        DBMetrics::get().increment_num_active_dbs(&metric_conf.db_name);
        Self {
            underlying,
            metric_conf,
            db_path,
        }
    }
}

impl Drop for DBWithThreadModeWrapper {
    fn drop(&mut self) {
        DBMetrics::get().decrement_num_active_dbs(&self.metric_conf.db_name);
    }
}

#[derive(Debug)]
pub struct OptimisticTransactionDBWrapper {
    pub underlying: rocksdb::OptimisticTransactionDB<MultiThreaded>,
    pub metric_conf: MetricConf,
    pub db_path: PathBuf,
}

impl OptimisticTransactionDBWrapper {
    fn new(
        underlying: rocksdb::OptimisticTransactionDB<MultiThreaded>,
        metric_conf: MetricConf,
        db_path: PathBuf,
    ) -> Self {
        DBMetrics::get().increment_num_active_dbs(&metric_conf.db_name);
        Self {
            underlying,
            metric_conf,
            db_path,
        }
    }
}

impl Drop for OptimisticTransactionDBWrapper {
    fn drop(&mut self) {
        DBMetrics::get().decrement_num_active_dbs(&self.metric_conf.db_name);
    }
}

/// Thin wrapper to unify interface across different db types
#[derive(Debug)]
pub enum RocksDB {
    DBWithThreadMode(DBWithThreadModeWrapper),
    OptimisticTransactionDB(OptimisticTransactionDBWrapper),
}

macro_rules! delegate_call {
    ($self:ident.$method:ident($($args:ident),*)) => {
        match $self {
            Self::DBWithThreadMode(d) => d.underlying.$method($($args),*),
            Self::OptimisticTransactionDB(d) => d.underlying.$method($($args),*),
        }
    }
}

impl Drop for RocksDB {
    fn drop(&mut self) {
        delegate_call!(self.cancel_all_background_work(/* wait */ true))
    }
}

impl RocksDB {
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        delegate_call!(self.get(key))
    }

    pub fn multi_get_cf<'a, 'b: 'a, K, I, W>(
        &'a self,
        keys: I,
        readopts: &ReadOptions,
    ) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = (&'b W, K)>,
        W: 'b + AsColumnFamilyRef,
    {
        delegate_call!(self.multi_get_cf_opt(keys, readopts))
    }

    pub fn batched_multi_get_cf_opt<I, K>(
        &self,
        cf: &impl AsColumnFamilyRef,
        keys: I,
        sorted_input: bool,
        readopts: &ReadOptions,
    ) -> Vec<Result<Option<DBPinnableSlice<'_>>, Error>>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<[u8]>,
    {
        delegate_call!(self.batched_multi_get_cf_opt(cf, keys, sorted_input, readopts))
    }

    pub fn property_int_value_cf(
        &self,
        cf: &impl AsColumnFamilyRef,
        name: impl CStrLike,
    ) -> Result<Option<u64>, rocksdb::Error> {
        delegate_call!(self.property_int_value_cf(cf, name))
    }

    pub fn get_pinned_cf_opt<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        readopts: &ReadOptions,
    ) -> Result<Option<DBPinnableSlice<'_>>, rocksdb::Error> {
        delegate_call!(self.get_pinned_cf_opt(cf, key, readopts))
    }

    pub fn cf_handle(&self, name: &str) -> Option<Arc<rocksdb::BoundColumnFamily<'_>>> {
        delegate_call!(self.cf_handle(name))
    }

    pub fn create_cf<N: AsRef<str>>(
        &self,
        name: N,
        opts: &rocksdb::Options,
    ) -> Result<(), rocksdb::Error> {
        delegate_call!(self.create_cf(name, opts))
    }

    pub fn drop_cf(&self, name: &str) -> Result<(), rocksdb::Error> {
        delegate_call!(self.drop_cf(name))
    }

    pub fn delete_file_in_range<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        from: K,
        to: K,
    ) -> Result<(), rocksdb::Error> {
        delegate_call!(self.delete_file_in_range_cf(cf, from, to))
    }

    pub fn delete_cf<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        writeopts: &WriteOptions,
    ) -> Result<(), rocksdb::Error> {
        fail_point!("delete-cf-before");
        let ret = delegate_call!(self.delete_cf_opt(cf, key, writeopts));
        fail_point!("delete-cf-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    pub fn path(&self) -> &Path {
        delegate_call!(self.path())
    }

    pub fn put_cf<K, V>(
        &self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        value: V,
        writeopts: &WriteOptions,
    ) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        fail_point!("put-cf-before");
        let ret = delegate_call!(self.put_cf_opt(cf, key, value, writeopts));
        fail_point!("put-cf-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    pub fn key_may_exist_cf<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        readopts: &ReadOptions,
    ) -> bool {
        delegate_call!(self.key_may_exist_cf_opt(cf, key, readopts))
    }

    pub fn try_catch_up_with_primary(&self) -> Result<(), rocksdb::Error> {
        delegate_call!(self.try_catch_up_with_primary())
    }

    pub fn write(
        &self,
        batch: RocksDBBatch,
        writeopts: &WriteOptions,
    ) -> Result<(), TypedStoreError> {
        fail_point!("batch-write-before");
        let ret = match (self, batch) {
            (RocksDB::DBWithThreadMode(db), RocksDBBatch::Regular(batch)) => {
                db.underlying
                    .write_opt(batch, writeopts)
                    .map_err(typed_store_err_from_rocks_err)?;
                Ok(())
            }
            (RocksDB::OptimisticTransactionDB(db), RocksDBBatch::Transactional(batch)) => {
                db.underlying
                    .write_opt(batch, writeopts)
                    .map_err(typed_store_err_from_rocks_err)?;
                Ok(())
            }
            _ => Err(TypedStoreError::RocksDBError(
                "using invalid batch type for the database".to_string(),
            )),
        };
        fail_point!("batch-write-after");
        #[allow(clippy::let_and_return)]
        ret
    }

    pub fn transaction_without_snapshot(
        &self,
    ) -> Result<Transaction<'_, rocksdb::OptimisticTransactionDB>, TypedStoreError> {
        match self {
            Self::OptimisticTransactionDB(db) => Ok(db.underlying.transaction()),
            Self::DBWithThreadMode(_) => panic!(),
        }
    }

    pub fn transaction(
        &self,
    ) -> Result<Transaction<'_, rocksdb::OptimisticTransactionDB>, TypedStoreError> {
        match self {
            Self::OptimisticTransactionDB(db) => {
                let mut tx_opts = OptimisticTransactionOptions::new();
                tx_opts.set_snapshot(true);

                Ok(db
                    .underlying
                    .transaction_opt(&WriteOptions::default(), &tx_opts))
            }
            Self::DBWithThreadMode(_) => panic!(),
        }
    }

    pub fn raw_iterator_cf<'a: 'b, 'b>(
        &'a self,
        cf_handle: &impl AsColumnFamilyRef,
        readopts: ReadOptions,
    ) -> RocksDBRawIter<'b> {
        match self {
            Self::DBWithThreadMode(db) => {
                RocksDBRawIter::DB(db.underlying.raw_iterator_cf_opt(cf_handle, readopts))
            }
            Self::OptimisticTransactionDB(db) => RocksDBRawIter::OptimisticTransactionDB(
                db.underlying.raw_iterator_cf_opt(cf_handle, readopts),
            ),
        }
    }

    pub fn iterator_cf<'a: 'b, 'b>(
        &'a self,
        cf_handle: &impl AsColumnFamilyRef,
        readopts: ReadOptions,
        mode: IteratorMode<'_>,
    ) -> RocksDBIter<'b> {
        match self {
            Self::DBWithThreadMode(db) => {
                RocksDBIter::DB(db.underlying.iterator_cf_opt(cf_handle, readopts, mode))
            }
            Self::OptimisticTransactionDB(db) => RocksDBIter::OptimisticTransactionDB(
                db.underlying.iterator_cf_opt(cf_handle, readopts, mode),
            ),
        }
    }

    pub fn compact_range_cf<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        start: Option<K>,
        end: Option<K>,
    ) {
        delegate_call!(self.compact_range_cf(cf, start, end))
    }

    pub fn compact_range_to_bottom<K: AsRef<[u8]>>(
        &self,
        cf: &impl AsColumnFamilyRef,
        start: Option<K>,
        end: Option<K>,
    ) {
        let opt = &mut CompactOptions::default();
        opt.set_bottommost_level_compaction(BottommostLevelCompaction::ForceOptimized);
        delegate_call!(self.compact_range_cf_opt(cf, start, end, opt))
    }

    pub fn flush(&self) -> Result<(), TypedStoreError> {
        delegate_call!(self.flush()).map_err(|e| TypedStoreError::RocksDBError(e.into_string()))
    }

    pub fn snapshot(&self) -> RocksDBSnapshot<'_> {
        match self {
            Self::DBWithThreadMode(d) => RocksDBSnapshot::DBWithThreadMode(d.underlying.snapshot()),
            Self::OptimisticTransactionDB(d) => {
                RocksDBSnapshot::OptimisticTransactionDB(d.underlying.snapshot())
            }
        }
    }

    pub fn checkpoint(&self, path: &Path) -> Result<(), TypedStoreError> {
        let checkpoint = match self {
            Self::DBWithThreadMode(d) => {
                Checkpoint::new(&d.underlying).map_err(typed_store_err_from_rocks_err)?
            }
            Self::OptimisticTransactionDB(d) => {
                Checkpoint::new(&d.underlying).map_err(typed_store_err_from_rocks_err)?
            }
        };
        checkpoint
            .create_checkpoint(path)
            .map_err(|e| TypedStoreError::RocksDBError(e.to_string()))?;
        Ok(())
    }

    pub fn flush_cf(&self, cf: &impl AsColumnFamilyRef) -> Result<(), rocksdb::Error> {
        delegate_call!(self.flush_cf(cf))
    }

    pub fn set_options_cf(
        &self,
        cf: &impl AsColumnFamilyRef,
        opts: &[(&str, &str)],
    ) -> Result<(), rocksdb::Error> {
        delegate_call!(self.set_options_cf(cf, opts))
    }

    pub fn get_sampling_interval(&self) -> SamplingInterval {
        match self {
            Self::DBWithThreadMode(d) => d.metric_conf.read_sample_interval.new_from_self(),
            Self::OptimisticTransactionDB(d) => d.metric_conf.read_sample_interval.new_from_self(),
        }
    }

    pub fn multiget_sampling_interval(&self) -> SamplingInterval {
        match self {
            Self::DBWithThreadMode(d) => d.metric_conf.read_sample_interval.new_from_self(),
            Self::OptimisticTransactionDB(d) => d.metric_conf.read_sample_interval.new_from_self(),
        }
    }

    pub fn write_sampling_interval(&self) -> SamplingInterval {
        match self {
            Self::DBWithThreadMode(d) => d.metric_conf.write_sample_interval.new_from_self(),
            Self::OptimisticTransactionDB(d) => d.metric_conf.write_sample_interval.new_from_self(),
        }
    }

    pub fn iter_sampling_interval(&self) -> SamplingInterval {
        match self {
            Self::DBWithThreadMode(d) => d.metric_conf.iter_sample_interval.new_from_self(),
            Self::OptimisticTransactionDB(d) => d.metric_conf.iter_sample_interval.new_from_self(),
        }
    }

    pub fn db_name(&self) -> String {
        let name = match self {
            Self::DBWithThreadMode(d) => &d.metric_conf.db_name,
            Self::OptimisticTransactionDB(d) => &d.metric_conf.db_name,
        };
        if name.is_empty() {
            self.default_db_name()
        } else {
            name.clone()
        }
    }

    fn default_db_name(&self) -> String {
        self.path()
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn live_files(&self) -> Result<Vec<LiveFile>, Error> {
        delegate_call!(self.live_files())
    }
}

pub enum RocksDBSnapshot<'a> {
    DBWithThreadMode(rocksdb::Snapshot<'a>),
    OptimisticTransactionDB(SnapshotWithThreadMode<'a, OptimisticTransactionDB>),
}

impl<'a> RocksDBSnapshot<'a> {
    pub fn multi_get_cf_opt<'b: 'a, K, I, W>(
        &'a self,
        keys: I,
        readopts: ReadOptions,
    ) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = (&'b W, K)>,
        W: 'b + AsColumnFamilyRef,
    {
        match self {
            Self::DBWithThreadMode(s) => s.multi_get_cf_opt(keys, readopts),
            Self::OptimisticTransactionDB(s) => s.multi_get_cf_opt(keys, readopts),
        }
    }
    pub fn multi_get_cf<'b: 'a, K, I, W>(
        &'a self,
        keys: I,
    ) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = (&'b W, K)>,
        W: 'b + AsColumnFamilyRef,
    {
        match self {
            Self::DBWithThreadMode(s) => s.multi_get_cf(keys),
            Self::OptimisticTransactionDB(s) => s.multi_get_cf(keys),
        }
    }
}

pub enum RocksDBBatch {
    Regular(rocksdb::WriteBatch),
    Transactional(rocksdb::WriteBatchWithTransaction<true>),
}

macro_rules! delegate_batch_call {
    ($self:ident.$method:ident($($args:ident),*)) => {
        match $self {
            Self::Regular(b) => b.$method($($args),*),
            Self::Transactional(b) => b.$method($($args),*),
        }
    }
}

impl RocksDBBatch {
    fn size_in_bytes(&self) -> usize {
        delegate_batch_call!(self.size_in_bytes())
    }

    pub fn delete_cf<K: AsRef<[u8]>>(&mut self, cf: &impl AsColumnFamilyRef, key: K) {
        delegate_batch_call!(self.delete_cf(cf, key))
    }

    pub fn put_cf<K, V>(&mut self, cf: &impl AsColumnFamilyRef, key: K, value: V)
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        delegate_batch_call!(self.put_cf(cf, key, value))
    }

    pub fn merge_cf<K, V>(&mut self, cf: &impl AsColumnFamilyRef, key: K, value: V)
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        delegate_batch_call!(self.merge_cf(cf, key, value))
    }

    pub fn delete_range_cf<K: AsRef<[u8]>>(
        &mut self,
        cf: &impl AsColumnFamilyRef,
        from: K,
        to: K,
    ) -> Result<(), TypedStoreError> {
        match self {
            Self::Regular(batch) => {
                batch.delete_range_cf(cf, from, to);
                Ok(())
            }
            Self::Transactional(_) => panic!(),
        }
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
    pub rocksdb: Arc<RocksDB>,
    _phantom: PhantomData<fn(K) -> V>,
    // the rocksDB ColumnFamily under which the map is stored
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
        db: Arc<RocksDB>,
        opts: &ReadWriteOptions,
        opt_cf: &str,
        is_deprecated: bool,
    ) -> Self {
        let db_cloned = db.clone();
        let db_metrics = DBMetrics::get();
        let db_metrics_cloned = db_metrics.clone();
        let cf = opt_cf.to_string();
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        if !is_deprecated {
            tokio::task::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_secs(CF_METRICS_REPORT_PERIOD_SECS));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            let db = db_cloned.clone();
                            let cf = cf.clone();
                            let db_metrics = db_metrics.clone();
                            if let Err(e) = tokio::task::spawn_blocking(move || {
                                Self::report_metrics(&db, &cf, &db_metrics);
                            }).await {
                                error!("Failed to log metrics with error: {}", e);
                            }
                        }
                        _ = &mut recv => break,
                    }
                }
                debug!("Returning the cf metric logging task for DBMap: {}", &cf);
            });
        }
        DBMap {
            rocksdb: db.clone(),
            opts: opts.clone(),
            _phantom: PhantomData,
            cf: opt_cf.to_string(),
            db_metrics: db_metrics_cloned,
            _metrics_task_cancel_handle: Arc::new(sender),
            get_sample_interval: db.get_sampling_interval(),
            multiget_sample_interval: db.multiget_sampling_interval(),
            write_sample_interval: db.write_sampling_interval(),
            iter_sample_interval: db.iter_sampling_interval(),
        }
    }

    /// Opens a database from a path, with specific options and an optional column family.
    ///
    /// This database is used to perform operations on single column family, and parametrizes
    /// all operations in `DBBatch` when writing across column families.
    #[instrument(level="debug", skip_all, fields(path = ?path.as_ref(), cf = ?opt_cf), err)]
    pub fn open<P: AsRef<Path>>(
        path: P,
        metric_conf: MetricConf,
        db_options: Option<rocksdb::Options>,
        opt_cf: Option<&str>,
        rw_options: &ReadWriteOptions,
    ) -> Result<Self, TypedStoreError> {
        let cf_key = opt_cf.unwrap_or(rocksdb::DEFAULT_COLUMN_FAMILY_NAME);
        let cfs = vec![cf_key];
        let rocksdb = open_cf(path, db_options, metric_conf, &cfs)?;
        Ok(DBMap::new(rocksdb, rw_options, cf_key, false))
    }

    /// Reopens an open database as a typed map operating under a specific column family.
    /// if no column family is passed, the default column family is used.
    ///
    /// ```
    ///    use typed_store::rocks::*;
    ///    use typed_store::metrics::DBMetrics;
    ///    use tempfile::tempdir;
    ///    use prometheus::Registry;
    ///    use std::sync::Arc;
    ///    use core::fmt::Error;
    ///    #[tokio::main]
    ///    async fn main() -> Result<(), Error> {
    ///    /// Open the DB with all needed column families first.
    ///    let rocks = open_cf(tempdir().unwrap(), None, MetricConf::default(), &["First_CF", "Second_CF"]).unwrap();
    ///    /// Attach the column families to specific maps.
    ///    let db_cf_1 = DBMap::<u32,u32>::reopen(&rocks, Some("First_CF"), &ReadWriteOptions::default(), false).expect("Failed to open storage");
    ///    let db_cf_2 = DBMap::<u32,u32>::reopen(&rocks, Some("Second_CF"), &ReadWriteOptions::default(), false).expect("Failed to open storage");
    ///    Ok(())
    ///    }
    /// ```
    #[instrument(level = "debug", skip(db), err)]
    pub fn reopen(
        db: &Arc<RocksDB>,
        opt_cf: Option<&str>,
        rw_options: &ReadWriteOptions,
        is_deprecated: bool,
    ) -> Result<Self, TypedStoreError> {
        let cf_key = opt_cf
            .unwrap_or(rocksdb::DEFAULT_COLUMN_FAMILY_NAME)
            .to_owned();

        db.cf_handle(&cf_key)
            .ok_or_else(|| TypedStoreError::UnregisteredColumn(cf_key.clone()))?;

        Ok(DBMap::new(db.clone(), rw_options, &cf_key, is_deprecated))
    }

    pub fn cf_name(&self) -> &str {
        &self.cf
    }

    pub fn batch(&self) -> DBBatch {
        let batch = match *self.rocksdb {
            RocksDB::DBWithThreadMode(_) => RocksDBBatch::Regular(WriteBatch::default()),
            RocksDB::OptimisticTransactionDB(_) => {
                RocksDBBatch::Transactional(WriteBatchWithTransaction::<true>::default())
            }
        };
        DBBatch::new(
            &self.rocksdb,
            batch,
            self.opts.writeopts(),
            &self.db_metrics,
            &self.write_sample_interval,
        )
    }

    pub fn compact_range<J: Serialize>(&self, start: &J, end: &J) -> Result<(), TypedStoreError> {
        let from_buf = be_fix_int_ser(start)?;
        let to_buf = be_fix_int_ser(end)?;
        self.rocksdb
            .compact_range_cf(&self.cf(), Some(from_buf), Some(to_buf));
        Ok(())
    }

    pub fn compact_range_raw(
        &self,
        cf_name: &str,
        start: Vec<u8>,
        end: Vec<u8>,
    ) -> Result<(), TypedStoreError> {
        let cf = self
            .rocksdb
            .cf_handle(cf_name)
            .expect("compact range: column family does not exist");
        self.rocksdb.compact_range_cf(&cf, Some(start), Some(end));
        Ok(())
    }

    pub fn compact_range_to_bottom<J: Serialize>(
        &self,
        start: &J,
        end: &J,
    ) -> Result<(), TypedStoreError> {
        let from_buf = be_fix_int_ser(start)?;
        let to_buf = be_fix_int_ser(end)?;
        self.rocksdb
            .compact_range_to_bottom(&self.cf(), Some(from_buf), Some(to_buf));
        Ok(())
    }

    pub fn cf(&self) -> Arc<rocksdb::BoundColumnFamily<'_>> {
        self.rocksdb
            .cf_handle(&self.cf)
            .expect("Map-keying column family should have been checked at DB creation")
    }

    pub fn iterator_cf(&self) -> RocksDBIter<'_> {
        self.rocksdb
            .iterator_cf(&self.cf(), self.opts.readopts(), IteratorMode::Start)
    }

    pub fn flush(&self) -> Result<(), TypedStoreError> {
        self.rocksdb
            .flush_cf(&self.cf())
            .map_err(|e| TypedStoreError::RocksDBError(e.into_string()))
    }

    pub fn set_options(&self, opts: &[(&str, &str)]) -> Result<(), rocksdb::Error> {
        self.rocksdb.set_options_cf(&self.cf(), opts)
    }

    fn get_int_property(
        rocksdb: &RocksDB,
        cf: &impl AsColumnFamilyRef,
        property_name: &std::ffi::CStr,
    ) -> Result<i64, TypedStoreError> {
        match rocksdb.property_int_value_cf(cf, property_name) {
            Ok(Some(value)) => Ok(value.min(i64::MAX as u64).try_into().unwrap_or_default()),
            Ok(None) => Ok(0),
            Err(e) => Err(TypedStoreError::RocksDBError(e.into_string())),
        }
    }

    /// Returns a vector of raw values corresponding to the keys provided.
    fn multi_get_pinned<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<DBPinnableSlice<'_>>>, TypedStoreError>
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
        let keys_bytes: Result<Vec<_>, TypedStoreError> = keys
            .into_iter()
            .map(|k| be_fix_int_ser(k.borrow()))
            .collect();
        let results: Result<Vec<_>, TypedStoreError> = self
            .rocksdb
            .batched_multi_get_cf_opt(
                &self.cf(),
                keys_bytes?,
                /*sorted_keys=*/ false,
                &self.opts.readopts(),
            )
            .into_iter()
            .map(|r| r.map_err(|e| TypedStoreError::RocksDBError(e.into_string())))
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

    fn report_metrics(rocksdb: &Arc<RocksDB>, cf_name: &str, db_metrics: &Arc<DBMetrics>) {
        let Some(cf) = rocksdb.cf_handle(cf_name) else {
            tracing::warn!(
                "unable to report metrics for cf {cf_name:?} in db {:?}",
                rocksdb.db_name()
            );
            return;
        };

        db_metrics
            .cf_metrics
            .rocksdb_total_sst_files_size
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::TOTAL_SST_FILES_SIZE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_total_blob_files_size
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE)
                    .unwrap_or(METRICS_ERROR),
            );
        // 7 is the default number of levels in RocksDB. If we ever change the number of levels using `set_num_levels`,
        // we need to update here as well. Note that there isn't an API to query the DB to get the number of levels (yet).
        let total_num_files: i64 = (0..=6)
            .map(|level| {
                Self::get_int_property(rocksdb, &cf, &num_files_at_level(level))
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
                Self::get_int_property(rocksdb, &cf, &num_files_at_level(0))
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_current_size_active_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::CUR_SIZE_ACTIVE_MEM_TABLE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_size_all_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::SIZE_ALL_MEM_TABLES)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_snapshots
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::NUM_SNAPSHOTS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_oldest_snapshot_time
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::OLDEST_SNAPSHOT_TIME)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_actual_delayed_write_rate
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::ACTUAL_DELAYED_WRITE_RATE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_is_write_stopped
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::IS_WRITE_STOPPED)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_capacity
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::BLOCK_CACHE_CAPACITY)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_usage
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::BLOCK_CACHE_USAGE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_block_cache_pinned_usage
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::BLOCK_CACHE_PINNED_USAGE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_table_readers_mem
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::ESTIMATE_TABLE_READERS_MEM)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimated_num_keys
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::ESTIMATE_NUM_KEYS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_immutable_mem_tables
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::NUM_IMMUTABLE_MEM_TABLE)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_mem_table_flush_pending
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::MEM_TABLE_FLUSH_PENDING)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_compaction_pending
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::COMPACTION_PENDING)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_pending_compaction_bytes
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::ESTIMATE_PENDING_COMPACTION_BYTES)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_running_compactions
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::NUM_RUNNING_COMPACTIONS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_num_running_flushes
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::NUM_RUNNING_FLUSHES)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_estimate_oldest_key_time
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::ESTIMATE_OLDEST_KEY_TIME)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_background_errors
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::BACKGROUND_ERRORS)
                    .unwrap_or(METRICS_ERROR),
            );
        db_metrics
            .cf_metrics
            .rocksdb_base_level
            .with_label_values(&[cf_name])
            .set(
                Self::get_int_property(rocksdb, &cf, properties::BASE_LEVEL)
                    .unwrap_or(METRICS_ERROR),
            );
    }

    pub fn transaction(&self) -> Result<DBTransaction<'_>, TypedStoreError> {
        DBTransaction::new(&self.rocksdb)
    }

    pub fn transaction_without_snapshot(&self) -> Result<DBTransaction<'_>, TypedStoreError> {
        DBTransaction::new_without_snapshot(&self.rocksdb)
    }

    pub fn checkpoint_db(&self, path: &Path) -> Result<(), TypedStoreError> {
        self.rocksdb.checkpoint(path)
    }

    pub fn snapshot(&self) -> Result<RocksDBSnapshot<'_>, TypedStoreError> {
        Ok(self.rocksdb.snapshot())
    }

    pub fn table_summary(&self) -> eyre::Result<TableSummary> {
        let mut num_keys = 0;
        let mut key_bytes_total = 0;
        let mut value_bytes_total = 0;
        let mut key_hist = hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
        let mut value_hist = hdrhistogram::Histogram::<u64>::new_with_max(100000, 2).unwrap();
        let iter = self.iterator_cf().map(Result::unwrap);
        for (key, value) in iter {
            num_keys += 1;
            key_bytes_total += key.len();
            value_bytes_total += value.len();
            key_hist.record(key.len() as u64)?;
            value_hist.record(value.len() as u64)?;
        }
        Ok(TableSummary {
            num_keys,
            key_bytes_total,
            value_bytes_total,
            key_hist,
            value_hist,
        })
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
        let timer = self
            .db_metrics
            .op_metrics
            .rocksdb_iter_latency_seconds
            .with_label_values(&[&self.cf])
            .start_timer();
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

    // Creates a RocksDB read option with specified lower and upper bounds.
    /// Lower bound is inclusive, and upper bound is exclusive.
    fn create_read_options_with_bounds(
        &self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> ReadOptions
    where
        K: Serialize,
    {
        let mut readopts = self.opts.readopts();
        if let Some(lower_bound) = lower_bound {
            let key_buf = be_fix_int_ser(&lower_bound).unwrap();
            readopts.set_iterate_lower_bound(key_buf);
        }
        if let Some(upper_bound) = upper_bound {
            let key_buf = be_fix_int_ser(&upper_bound).unwrap();
            readopts.set_iterate_upper_bound(key_buf);
        }
        readopts
    }

    // Creates a RocksDB read option with lower and upper bounds set corresponding to `range`.
    fn create_read_options_with_range(&self, range: impl RangeBounds<K>) -> ReadOptions
    where
        K: Serialize,
    {
        let mut readopts = self.opts.readopts();

        let lower_bound = range.start_bound();
        let upper_bound = range.end_bound();

        match lower_bound {
            Bound::Included(lower_bound) => {
                // Rocksdb lower bound is inclusive by default so nothing to do
                let key_buf = be_fix_int_ser(&lower_bound).expect("Serialization must not fail");
                readopts.set_iterate_lower_bound(key_buf);
            }
            Bound::Excluded(lower_bound) => {
                let mut key_buf =
                    be_fix_int_ser(&lower_bound).expect("Serialization must not fail");

                // Since we want exclusive, we need to increment the key to exclude the previous
                big_endian_saturating_add_one(&mut key_buf);
                readopts.set_iterate_lower_bound(key_buf);
            }
            Bound::Unbounded => (),
        };

        match upper_bound {
            Bound::Included(upper_bound) => {
                let mut key_buf =
                    be_fix_int_ser(&upper_bound).expect("Serialization must not fail");

                // If the key is already at the limit, there's nowhere else to go, so no upper bound
                if !is_max(&key_buf) {
                    // Since we want exclusive, we need to increment the key to get the upper bound
                    big_endian_saturating_add_one(&mut key_buf);
                    readopts.set_iterate_upper_bound(key_buf);
                }
            }
            Bound::Excluded(upper_bound) => {
                // Rocksdb upper bound is inclusive by default so nothing to do
                let key_buf = be_fix_int_ser(&upper_bound).expect("Serialization must not fail");
                readopts.set_iterate_upper_bound(key_buf);
            }
            Bound::Unbounded => (),
        };

        readopts
    }
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
/// let rocks = open_cf(tempfile::tempdir().unwrap(), None, MetricConf::default(), &["First_CF", "Second_CF"]).unwrap();
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
    rocksdb: Arc<RocksDB>,
    batch: RocksDBBatch,
    opts: WriteOptions,
    db_metrics: Arc<DBMetrics>,
    write_sample_interval: SamplingInterval,
}

impl DBBatch {
    /// Create a new batch associated with a DB reference.
    ///
    /// Use `open_cf` to get the DB reference or an existing open database.
    pub fn new(
        dbref: &Arc<RocksDB>,
        batch: RocksDBBatch,
        opts: WriteOptions,
        db_metrics: &Arc<DBMetrics>,
        write_sample_interval: &SamplingInterval,
    ) -> Self {
        DBBatch {
            rocksdb: dbref.clone(),
            batch,
            opts,
            db_metrics: db_metrics.clone(),
            write_sample_interval: write_sample_interval.clone(),
        }
    }

    /// Consume the batch and write its operations to the database
    #[instrument(level = "trace", skip_all, err)]
    pub fn write(self) -> Result<(), TypedStoreError> {
        let db_name = self.rocksdb.db_name();
        let timer = self
            .db_metrics
            .op_metrics
            .rocksdb_batch_commit_latency_seconds
            .with_label_values(&[&db_name])
            .start_timer();
        let batch_size = self.batch.size_in_bytes();

        let perf_ctx = if self.write_sample_interval.sample() {
            Some(RocksDBPerfContext)
        } else {
            None
        };
        self.rocksdb.write(self.batch, &self.opts)?;
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
        self.batch.size_in_bytes()
    }
}

impl DBBatch {
    pub fn delete_batch<J: Borrow<K>, K: Serialize, V>(
        &mut self,
        db: &DBMap<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<(), TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }

        purged_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|k| {
                let k_buf = be_fix_int_ser(k.borrow())?;
                self.batch.delete_cf(&db.cf(), k_buf);

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
    /// overriden in the config), so please use this function with caution
    pub fn schedule_delete_range<K: Serialize, V>(
        &mut self,
        db: &DBMap<K, V>,
        from: &K,
        to: &K,
    ) -> Result<(), TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }

        let from_buf = be_fix_int_ser(from)?;
        let to_buf = be_fix_int_ser(to)?;

        self.batch.delete_range_cf(&db.cf(), from_buf, to_buf)?;
        Ok(())
    }

    /// inserts a range of (key, value) pairs given as an iterator
    pub fn insert_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &DBMap<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<&mut Self, TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }
        let mut total = 0usize;
        new_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|(k, v)| {
                let k_buf = be_fix_int_ser(k.borrow())?;
                let v_buf = bcs::to_bytes(v.borrow()).map_err(typed_store_err_from_bcs_err)?;
                total += k_buf.len() + v_buf.len();
                self.batch.put_cf(&db.cf(), k_buf, v_buf);
                Ok(())
            })?;
        self.db_metrics
            .op_metrics
            .rocksdb_batch_put_bytes
            .with_label_values(&[&db.cf])
            .observe(total as f64);
        Ok(self)
    }
}

pub struct DBTransaction<'a> {
    rocksdb: Arc<RocksDB>,
    transaction: Transaction<'a, rocksdb::OptimisticTransactionDB>,
}

impl<'a> DBTransaction<'a> {
    pub fn new(db: &'a Arc<RocksDB>) -> Result<Self, TypedStoreError> {
        Ok(Self {
            rocksdb: db.clone(),
            transaction: db.transaction()?,
        })
    }

    pub fn new_without_snapshot(db: &'a Arc<RocksDB>) -> Result<Self, TypedStoreError> {
        Ok(Self {
            rocksdb: db.clone(),
            transaction: db.transaction_without_snapshot()?,
        })
    }

    pub fn insert_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &DBMap<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<&mut Self, TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }

        new_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|(k, v)| {
                let k_buf = be_fix_int_ser(k.borrow())?;
                let v_buf = bcs::to_bytes(v.borrow()).map_err(typed_store_err_from_bcs_err)?;
                self.transaction
                    .put_cf(&db.cf(), k_buf, v_buf)
                    .map_err(typed_store_err_from_rocks_err)?;
                Ok(())
            })?;
        Ok(self)
    }

    /// Deletes a set of keys given as an iterator
    pub fn delete_batch<J: Borrow<K>, K: Serialize, V>(
        &mut self,
        db: &DBMap<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<&mut Self, TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }
        purged_vals
            .into_iter()
            .try_for_each::<_, Result<_, TypedStoreError>>(|k| {
                let k_buf = be_fix_int_ser(k.borrow())?;
                self.transaction
                    .delete_cf(&db.cf(), k_buf)
                    .map_err(typed_store_err_from_rocks_err)?;
                Ok(())
            })?;
        Ok(self)
    }

    pub fn snapshot(
        &self,
    ) -> rocksdb::SnapshotWithThreadMode<'_, Transaction<'a, rocksdb::OptimisticTransactionDB>>
    {
        self.transaction.snapshot()
    }

    pub fn get_for_update<K: Serialize, V: DeserializeOwned>(
        &self,
        db: &DBMap<K, V>,
        key: &K,
    ) -> Result<Option<V>, TypedStoreError> {
        if !Arc::ptr_eq(&db.rocksdb, &self.rocksdb) {
            return Err(TypedStoreError::CrossDBBatch);
        }
        let k_buf = be_fix_int_ser(key)?;
        match self
            .transaction
            .get_for_update_cf_opt(&db.cf(), k_buf, true, &db.opts.readopts())
            .map_err(typed_store_err_from_rocks_err)?
        {
            Some(data) => Ok(Some(
                bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err)?,
            )),
            None => Ok(None),
        }
    }

    pub fn get<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned>(
        &self,
        db: &DBMap<K, V>,
        key: &K,
    ) -> Result<Option<V>, TypedStoreError> {
        let key_buf = be_fix_int_ser(key)?;
        self.transaction
            .get_cf_opt(&db.cf(), key_buf, &db.opts.readopts())
            .map_err(|e| TypedStoreError::RocksDBError(e.to_string()))
            .map(|res| res.and_then(|bytes| bcs::from_bytes::<V>(&bytes).ok()))
    }

    pub fn multi_get<J: Borrow<K>, K: Serialize + DeserializeOwned, V: DeserializeOwned>(
        &self,
        db: &DBMap<K, V>,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<V>>, TypedStoreError> {
        let cf = db.cf();
        let keys_bytes: Result<Vec<_>, TypedStoreError> = keys
            .into_iter()
            .map(|k| Ok((&cf, be_fix_int_ser(k.borrow())?)))
            .collect();

        let results = self
            .transaction
            .multi_get_cf_opt(keys_bytes?, &db.opts.readopts());

        let values_parsed: Result<Vec<_>, TypedStoreError> = results
            .into_iter()
            .map(
                |value_byte| match value_byte.map_err(typed_store_err_from_rocks_err)? {
                    Some(data) => Ok(Some(
                        bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err)?,
                    )),
                    None => Ok(None),
                },
            )
            .collect();

        values_parsed
    }

    pub fn iter<K: DeserializeOwned, V: DeserializeOwned>(
        &'a self,
        db: &DBMap<K, V>,
    ) -> Iter<'a, K, V> {
        let db_iter = self
            .transaction
            .raw_iterator_cf_opt(&db.cf(), db.opts.readopts());
        Iter::new(
            db.cf.clone(),
            RocksDBRawIter::OptimisticTransaction(db_iter),
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub fn keys<K: DeserializeOwned, V: DeserializeOwned>(
        &'a self,
        db: &DBMap<K, V>,
    ) -> Keys<'a, K> {
        let mut db_iter = RocksDBRawIter::OptimisticTransaction(
            self.transaction
                .raw_iterator_cf_opt(&db.cf(), db.opts.readopts()),
        );
        db_iter.seek_to_first();

        Keys::new(db_iter)
    }

    pub fn values<K: DeserializeOwned, V: DeserializeOwned>(
        &'a self,
        db: &DBMap<K, V>,
    ) -> Values<'a, V> {
        let mut db_iter = RocksDBRawIter::OptimisticTransaction(
            self.transaction
                .raw_iterator_cf_opt(&db.cf(), db.opts.readopts()),
        );
        db_iter.seek_to_first();

        Values::new(db_iter)
    }

    pub fn commit(self) -> Result<(), TypedStoreError> {
        fail_point!("transaction-commit");
        self.transaction.commit().map_err(|e| match e.kind() {
            // empirically, this is what you get when there is a write conflict. it is not
            // documented whether this is the only time you can get this error.
            ErrorKind::Busy | ErrorKind::TryAgain => TypedStoreError::RetryableTransactionError,
            _ => typed_store_err_from_rocks_err(e),
        })?;
        Ok(())
    }
}

macro_rules! delegate_iter_call {
    ($self:ident.$method:ident($($args:ident),*)) => {
        match $self {
            Self::DB(db) => db.$method($($args),*),
            Self::OptimisticTransactionDB(db) => db.$method($($args),*),
            Self::OptimisticTransaction(db) => db.$method($($args),*),
        }
    }
}

pub enum RocksDBRawIter<'a> {
    DB(rocksdb::DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>),
    OptimisticTransactionDB(
        rocksdb::DBRawIteratorWithThreadMode<'a, rocksdb::OptimisticTransactionDB<MultiThreaded>>,
    ),
    OptimisticTransaction(
        rocksdb::DBRawIteratorWithThreadMode<
            'a,
            Transaction<'a, rocksdb::OptimisticTransactionDB<MultiThreaded>>,
        >,
    ),
}

impl<'a> RocksDBRawIter<'a> {
    pub fn valid(&self) -> bool {
        delegate_iter_call!(self.valid())
    }
    pub fn key(&self) -> Option<&[u8]> {
        delegate_iter_call!(self.key())
    }
    pub fn value(&self) -> Option<&[u8]> {
        delegate_iter_call!(self.value())
    }
    pub fn next(&mut self) {
        delegate_iter_call!(self.next())
    }
    pub fn prev(&mut self) {
        delegate_iter_call!(self.prev())
    }
    pub fn seek<K: AsRef<[u8]>>(&mut self, key: K) {
        delegate_iter_call!(self.seek(key))
    }
    pub fn seek_to_last(&mut self) {
        delegate_iter_call!(self.seek_to_last())
    }
    pub fn seek_to_first(&mut self) {
        delegate_iter_call!(self.seek_to_first())
    }
    pub fn seek_for_prev<K: AsRef<[u8]>>(&mut self, key: K) {
        delegate_iter_call!(self.seek_for_prev(key))
    }
    pub fn status(&self) -> Result<(), rocksdb::Error> {
        delegate_iter_call!(self.status())
    }
}

pub enum RocksDBIter<'a> {
    DB(rocksdb::DBIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>),
    OptimisticTransactionDB(
        rocksdb::DBIteratorWithThreadMode<'a, rocksdb::OptimisticTransactionDB<MultiThreaded>>,
    ),
}

impl<'a> Iterator for RocksDBIter<'a> {
    type Item = Result<(Box<[u8]>, Box<[u8]>), Error>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::DB(db) => db.next(),
            Self::OptimisticTransactionDB(db) => db.next(),
        }
    }
}

impl<'a, K, V> Map<'a, K, V> for DBMap<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = TypedStoreError;
    type Iterator = Iter<'a, K, V>;
    type SafeIterator = SafeIter<'a, K, V>;
    type Keys = Keys<'a, K>;
    type Values = Values<'a, V>;

    #[instrument(level = "trace", skip_all, err)]
    fn contains_key(&self, key: &K) -> Result<bool, TypedStoreError> {
        let key_buf = be_fix_int_ser(key)?;
        // [`rocksdb::DBWithThreadMode::key_may_exist_cf`] can have false positives,
        // but no false negatives. We use it to short-circuit the absent case
        let readopts = self.opts.readopts();
        Ok(self
            .rocksdb
            .key_may_exist_cf(&self.cf(), &key_buf, &readopts)
            && self
                .rocksdb
                .get_pinned_cf_opt(&self.cf(), &key_buf, &readopts)
                .map_err(typed_store_err_from_rocks_err)?
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
        let key_buf = be_fix_int_ser(key)?;
        let res = self
            .rocksdb
            .get_pinned_cf_opt(&self.cf(), &key_buf, &self.opts.readopts())
            .map_err(typed_store_err_from_rocks_err)?;
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
            Some(data) => Ok(Some(
                bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err)?,
            )),
            None => Ok(None),
        }
    }

    #[instrument(level = "trace", skip_all, err)]
    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, TypedStoreError> {
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
        let key_buf = be_fix_int_ser(key)?;
        let res = self
            .rocksdb
            .get_pinned_cf_opt(&self.cf(), &key_buf, &self.opts.readopts())
            .map_err(typed_store_err_from_rocks_err)?;
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
            Some(data) => Ok(Some(data.to_vec())),
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
        let key_buf = be_fix_int_ser(key)?;
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
        self.rocksdb
            .put_cf(&self.cf(), &key_buf, &value_buf, &self.opts.writeopts())
            .map_err(typed_store_err_from_rocks_err)?;

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
        let key_buf = be_fix_int_ser(key)?;
        self.rocksdb
            .delete_cf(&self.cf(), key_buf, &self.opts.writeopts())
            .map_err(typed_store_err_from_rocks_err)?;
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

    /// Deletes a range of keys between `from` (inclusive) and `to` (non-inclusive)
    /// by immediately deleting any sst files whose key range overlaps with the range.
    /// Files whose range only partially overlaps with the range are not deleted.
    /// This can be useful for quickly removing a large amount of data without having
    /// to delete individual keys. Only files at level 1 or higher are considered (
    /// Level 0 files are skipped). It doesn't guarantee that all keys in the range are
    /// deleted, as there might be keys in files that weren't entirely within the range.
    #[instrument(level = "trace", skip_all, err)]
    fn delete_file_in_range(&self, from: &K, to: &K) -> Result<(), TypedStoreError> {
        let from_buf = be_fix_int_ser(from.borrow())?;
        let to_buf = be_fix_int_ser(to.borrow())?;
        self.rocksdb
            .delete_file_in_range(&self.cf(), from_buf, to_buf)
            .map_err(typed_store_err_from_rocks_err)?;
        Ok(())
    }

    /// This method first drops the existing column family and then creates a new one
    /// with the same name. The two operations are not atomic and hence it is possible
    /// to get into a race condition where the column family has been dropped but new
    /// one is not created yet
    #[instrument(level = "trace", skip_all, err)]
    fn unsafe_clear(&self) -> Result<(), TypedStoreError> {
        let _ = self.rocksdb.drop_cf(&self.cf);
        self.rocksdb
            .create_cf(self.cf.clone(), &default_db_options().options)
            .map_err(typed_store_err_from_rocks_err)?;
        Ok(())
    }

    /// Writes a range delete tombstone to delete all entries in the db map
    /// If the DBMap is configured with ignore_range_deletions set to false,
    /// the effect of this write will be visible immediately i.e. you won't
    /// see old values when you do a lookup or scan. But if it is configured
    /// with ignore_range_deletions set to true, the old value are visible until
    /// compaction actually deletes them which will happen sometime after. By
    /// default ignore_range_deletions is set to true on a DBMap (unless it is
    /// overriden in the config), so please use this function with caution
    #[instrument(level = "trace", skip_all, err)]
    fn schedule_delete_all(&self) -> Result<(), TypedStoreError> {
        let mut iter = self.unbounded_iter().seek_to_first();
        let first_key = iter.next().map(|(k, _v)| k);
        let last_key = iter.skip_to_last().next().map(|(k, _v)| k);
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

    /// Returns an unbounded iterator visiting each key-value pair in the map.
    /// This is potentially unsafe as it can perform a full table scan
    fn unbounded_iter(&'a self) -> Self::Iterator {
        let db_iter = self
            .rocksdb
            .raw_iterator_cf(&self.cf(), self.opts.readopts());
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        Iter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    /// Returns an iterator visiting each key-value pair in the map. By proving bounds of the
    /// scan range, RocksDB scan avoid unnecessary scans.
    /// Lower bound is inclusive, while upper bound is exclusive.
    fn iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::Iterator {
        let readopts = self.create_read_options_with_bounds(lower_bound, upper_bound);
        let db_iter = self.rocksdb.raw_iterator_cf(&self.cf(), readopts);
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        Iter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    /// Similar to `iter_with_bounds` but allows specifying inclusivity/exclusivity of ranges explicitly.
    /// TODO: find better name
    fn range_iter(&'a self, range: impl RangeBounds<K>) -> Self::Iterator {
        let readopts = self.create_read_options_with_range(range);
        let db_iter = self.rocksdb.raw_iterator_cf(&self.cf(), readopts);
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        Iter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    fn safe_iter(&'a self) -> Self::SafeIterator {
        let db_iter = self
            .rocksdb
            .raw_iterator_cf(&self.cf(), self.opts.readopts());
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        SafeIter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    fn safe_iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::SafeIterator {
        let readopts = self.create_read_options_with_bounds(lower_bound, upper_bound);
        let db_iter = self.rocksdb.raw_iterator_cf(&self.cf(), readopts);
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        SafeIter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    fn safe_range_iter(&'a self, range: impl RangeBounds<K>) -> Self::SafeIterator {
        let readopts = self.create_read_options_with_range(range);
        let db_iter = self.rocksdb.raw_iterator_cf(&self.cf(), readopts);
        let (_timer, bytes_scanned, keys_scanned, _perf_ctx) = self.create_iter_context();
        SafeIter::new(
            self.cf.clone(),
            db_iter,
            _timer,
            _perf_ctx,
            bytes_scanned,
            keys_scanned,
            Some(self.db_metrics.clone()),
        )
    }

    fn keys(&'a self) -> Self::Keys {
        let mut db_iter = self
            .rocksdb
            .raw_iterator_cf(&self.cf(), self.opts.readopts());
        db_iter.seek_to_first();

        Keys::new(db_iter)
    }

    fn values(&'a self) -> Self::Values {
        let mut db_iter = self
            .rocksdb
            .raw_iterator_cf(&self.cf(), self.opts.readopts());
        db_iter.seek_to_first();

        Values::new(db_iter)
    }

    /// Returns a vector of raw values corresponding to the keys provided.
    #[instrument(level = "trace", skip_all, err)]
    fn multi_get_raw_bytes<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<Vec<u8>>>, TypedStoreError>
    where
        J: Borrow<K>,
    {
        let results = self
            .multi_get_pinned(keys)?
            .into_iter()
            .map(|val| val.map(|v| v.to_vec()))
            .collect();
        Ok(results)
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

    /// Returns a vector of values corresponding to the keys provided.
    #[instrument(level = "trace", skip_all, err)]
    fn chunked_multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
        chunk_size: usize,
    ) -> Result<Vec<Option<V>>, TypedStoreError>
    where
        J: Borrow<K>,
    {
        let cf = self.cf();
        let keys_bytes = keys
            .into_iter()
            .map(|k| (&cf, be_fix_int_ser(k.borrow()).unwrap()));
        let chunked_keys = keys_bytes.into_iter().chunks(chunk_size);
        let snapshot = self.snapshot()?;
        let mut results = vec![];
        for chunk in chunked_keys.into_iter() {
            let chunk_result = snapshot.multi_get_cf(chunk);
            let values_parsed: Result<Vec<_>, TypedStoreError> = chunk_result
                .into_iter()
                .map(|value_byte| {
                    let value_byte = value_byte.map_err(typed_store_err_from_rocks_err)?;
                    match value_byte {
                        Some(data) => Ok(Some(
                            bcs::from_bytes(&data).map_err(typed_store_err_from_bcs_err)?,
                        )),
                        None => Ok(None),
                    }
                })
                .collect();
            results.extend(values_parsed?);
        }
        Ok(results)
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
        self.rocksdb
            .try_catch_up_with_primary()
            .map_err(typed_store_err_from_rocks_err)
    }
}

impl<J, K, U, V> TryExtend<(J, U)> for DBMap<K, V>
where
    J: Borrow<K>,
    U: Borrow<V>,
    K: Serialize,
    V: Serialize,
{
    type Error = TypedStoreError;

    fn try_extend<T>(&mut self, iter: &mut T) -> Result<(), Self::Error>
    where
        T: Iterator<Item = (J, U)>,
    {
        let mut batch = self.batch();
        batch.insert_batch(self, iter)?;
        batch.write()
    }

    fn try_extend_from_slice(&mut self, slice: &[(J, U)]) -> Result<(), Self::Error> {
        let slice_of_refs = slice.iter().map(|(k, v)| (k.borrow(), v.borrow()));
        let mut batch = self.batch();
        batch.insert_batch(self, slice_of_refs)?;
        batch.write()
    }
}

pub fn read_size_from_env(var_name: &str) -> Option<usize> {
    env::var(var_name)
        .ok()?
        .parse::<usize>()
        .tap_err(|e| {
            warn!(
                "Env var {} does not contain valid usize integer: {}",
                var_name, e
            )
        })
        .ok()
}

#[derive(Clone, Debug)]
pub struct ReadWriteOptions {
    pub ignore_range_deletions: bool,
    // Whether to sync to disk on every write.
    sync_to_disk: bool,
}

impl ReadWriteOptions {
    pub fn readopts(&self) -> ReadOptions {
        let mut readopts = ReadOptions::default();
        readopts.set_ignore_range_deletions(self.ignore_range_deletions);
        readopts
    }

    pub fn writeopts(&self) -> WriteOptions {
        let mut opts = WriteOptions::default();
        opts.set_sync(self.sync_to_disk);
        opts
    }

    pub fn set_ignore_range_deletions(mut self, ignore: bool) -> Self {
        self.ignore_range_deletions = ignore;
        self
    }
}

impl Default for ReadWriteOptions {
    fn default() -> Self {
        Self {
            ignore_range_deletions: true,
            sync_to_disk: std::env::var("SUI_DB_SYNC_TO_DISK").map_or(false, |v| v != "0"),
        }
    }
}
// TODO: refactor this into a builder pattern, where rocksdb::Options are
// generated after a call to build().
#[derive(Default, Clone)]
pub struct DBOptions {
    pub options: rocksdb::Options,
    pub rw_options: ReadWriteOptions,
}

impl DBOptions {
    // Optimize lookup perf for tables where no scans are performed.
    // If non-trivial number of values can be > 512B in size, it is beneficial to also
    // specify optimize_for_large_values_no_scan().
    pub fn optimize_for_point_lookup(mut self, block_cache_size_mb: usize) -> DBOptions {
        // NOTE: this overwrites the block options.
        self.options
            .optimize_for_point_lookup(block_cache_size_mb as u64);
        self
    }

    // Optimize write and lookup perf for tables which are rarely scanned, and have large values.
    // https://rocksdb.org/blog/2021/05/26/integrated-blob-db.html
    pub fn optimize_for_large_values_no_scan(mut self, min_blob_size: u64) -> DBOptions {
        if env::var(ENV_VAR_DISABLE_BLOB_STORAGE).is_ok() {
            info!("Large value blob storage optimization is disabled via env var.");
            return self;
        }

        // Blob settings.
        self.options.set_enable_blob_files(true);
        self.options
            .set_blob_compression_type(rocksdb::DBCompressionType::Lz4);
        self.options.set_enable_blob_gc(true);
        // Since each blob can have non-trivial size overhead, and compression does not work across blobs,
        // set a min blob size in bytes to so small transactions and effects are kept in sst files.
        self.options.set_min_blob_size(min_blob_size);

        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Since large blobs are not in sst files, reduce the target file size and base level
        // target size.
        let target_file_size_base = 64 << 20;
        self.options
            .set_target_file_size_base(target_file_size_base);
        // Level 1 default to 64MiB * 4 ~ 256MiB.
        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options
            .set_max_bytes_for_level_base(target_file_size_base * max_level_zero_file_num as u64);

        self
    }

    // Optimize tables with a mix of lookup and scan workloads.
    pub fn optimize_for_read(mut self, block_cache_size_mb: usize) -> DBOptions {
        self.options
            .set_block_based_table_factory(&get_block_options(block_cache_size_mb, 16 << 10));
        self
    }

    // Optimize DB receiving significant insertions.
    pub fn optimize_db_for_write_throughput(mut self, db_max_write_buffer_gb: u64) -> DBOptions {
        self.options
            .set_db_write_buffer_size(db_max_write_buffer_gb as usize * 1024 * 1024 * 1024);
        self.options
            .set_max_total_wal_size(db_max_write_buffer_gb * 1024 * 1024 * 1024);
        self
    }

    // Optimize tables receiving significant insertions.
    pub fn optimize_for_write_throughput(mut self) -> DBOptions {
        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Increase write buffers to keep to 6 before slowing down writes.
        let max_write_buffer_number = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_NUMBER)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_NUMBER);
        self.options
            .set_max_write_buffer_number(max_write_buffer_number.try_into().unwrap());
        // Keep 1 write buffer so recent writes can be read from memory.
        self.options
            .set_max_write_buffer_size_to_maintain((write_buffer_size).try_into().unwrap());

        // Increase compaction trigger for level 0 to 6.
        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options.set_level_zero_file_num_compaction_trigger(
            max_level_zero_file_num.try_into().unwrap(),
        );
        self.options.set_level_zero_slowdown_writes_trigger(
            (max_level_zero_file_num * 12).try_into().unwrap(),
        );
        self.options
            .set_level_zero_stop_writes_trigger((max_level_zero_file_num * 16).try_into().unwrap());

        // Increase sst file size to 128MiB.
        self.options.set_target_file_size_base(
            read_size_from_env(ENV_VAR_TARGET_FILE_SIZE_BASE_MB)
                .unwrap_or(DEFAULT_TARGET_FILE_SIZE_BASE_MB) as u64
                * 1024
                * 1024,
        );

        // Increase level 1 target size to 256MiB * 6 ~ 1.5GiB.
        self.options
            .set_max_bytes_for_level_base((write_buffer_size * max_level_zero_file_num) as u64);

        self
    }

    // Optimize tables receiving significant insertions, without any deletions.
    // TODO: merge this function with optimize_for_write_throughput(), and use a flag to
    // indicate if deletion is received.
    pub fn optimize_for_write_throughput_no_deletion(mut self) -> DBOptions {
        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Increase write buffers to keep to 6 before slowing down writes.
        let max_write_buffer_number = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_NUMBER)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_NUMBER);
        self.options
            .set_max_write_buffer_number(max_write_buffer_number.try_into().unwrap());
        // Keep 1 write buffer so recent writes can be read from memory.
        self.options
            .set_max_write_buffer_size_to_maintain((write_buffer_size).try_into().unwrap());

        // Switch to universal compactions.
        self.options
            .set_compaction_style(rocksdb::DBCompactionStyle::Universal);
        let mut compaction_options = rocksdb::UniversalCompactOptions::default();
        compaction_options.set_max_size_amplification_percent(10000);
        compaction_options.set_stop_style(rocksdb::UniversalCompactionStopStyle::Similar);
        self.options
            .set_universal_compaction_options(&compaction_options);

        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_UNIVERSAL_COMPACTION_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options.set_level_zero_file_num_compaction_trigger(
            max_level_zero_file_num.try_into().unwrap(),
        );
        self.options.set_level_zero_slowdown_writes_trigger(
            (max_level_zero_file_num * 12).try_into().unwrap(),
        );
        self.options
            .set_level_zero_stop_writes_trigger((max_level_zero_file_num * 16).try_into().unwrap());

        // Increase sst file size to 128MiB.
        self.options.set_target_file_size_base(
            read_size_from_env(ENV_VAR_TARGET_FILE_SIZE_BASE_MB)
                .unwrap_or(DEFAULT_TARGET_FILE_SIZE_BASE_MB) as u64
                * 1024
                * 1024,
        );

        // This should be a no-op for universal compaction but increasing it to be safe.
        self.options
            .set_max_bytes_for_level_base((write_buffer_size * max_level_zero_file_num) as u64);

        self
    }

    // Overrides the block options with different block cache size and block size.
    pub fn set_block_options(
        mut self,
        block_cache_size_mb: usize,
        block_size_bytes: usize,
    ) -> DBOptions {
        self.options
            .set_block_based_table_factory(&get_block_options(
                block_cache_size_mb,
                block_size_bytes,
            ));
        self
    }

    // Disables write stalling and stopping based on pending compaction bytes.
    pub fn disable_write_throttling(mut self) -> DBOptions {
        self.options.set_soft_pending_compaction_bytes_limit(0);
        self.options.set_hard_pending_compaction_bytes_limit(0);
        self
    }
}

/// Creates a default RocksDB option, to be used when RocksDB option is unspecified.
pub fn default_db_options() -> DBOptions {
    let mut opt = rocksdb::Options::default();

    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    if let Some(limit) = fdlimit::raise_fd_limit() {
        // on windows raise_fd_limit return None
        opt.set_max_open_files((limit / 8) as i32);
    }

    // The table cache is locked for updates and this determines the number
    // of shards, ie 2^10. Increase in case of lock contentions.
    opt.set_table_cache_num_shard_bits(10);

    // LSM compression settings
    opt.set_compression_type(rocksdb::DBCompressionType::Lz4);
    opt.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
    opt.set_bottommost_zstd_max_train_bytes(1024 * 1024, true);

    // Sui uses multiple RocksDB in a node, so total sizes of write buffers and WAL can be higher
    // than the limits below.
    //
    // RocksDB also exposes the option to configure total write buffer size across multiple instances
    // via `write_buffer_manager`. But the write buffer flush policy (flushing the buffer receiving
    // the next write) may not work well. So sticking to per-db write buffer size limit for now.
    //
    // The environment variables are only meant to be emergency overrides. They may go away in future.
    // It is preferable to update the default value, or override the option in code.
    opt.set_db_write_buffer_size(
        read_size_from_env(ENV_VAR_DB_WRITE_BUFFER_SIZE).unwrap_or(DEFAULT_DB_WRITE_BUFFER_SIZE)
            * 1024
            * 1024,
    );
    opt.set_max_total_wal_size(
        read_size_from_env(ENV_VAR_DB_WAL_SIZE).unwrap_or(DEFAULT_DB_WAL_SIZE) as u64 * 1024 * 1024,
    );

    // Num threads for compactions and memtable flushes.
    opt.increase_parallelism(read_size_from_env(ENV_VAR_DB_PARALLELISM).unwrap_or(8) as i32);

    opt.set_enable_pipelined_write(true);

    // Increase block size to 16KiB.
    // https://github.com/EighteenZi/rocksdb_wiki/blob/master/Memory-usage-in-RocksDB.md#indexes-and-filter-blocks
    opt.set_block_based_table_factory(&get_block_options(128, 16 << 10));

    // Set memtable bloomfilter.
    opt.set_memtable_prefix_bloom_ratio(0.02);

    DBOptions {
        options: opt,
        rw_options: ReadWriteOptions::default(),
    }
}

fn get_block_options(block_cache_size_mb: usize, block_size_bytes: usize) -> BlockBasedOptions {
    // Set options mostly similar to those used in optimize_for_point_lookup(),
    // except non-default binary and hash index, to hopefully reduce lookup latencies
    // without causing any regression for scanning, with slightly more memory usages.
    // https://github.com/facebook/rocksdb/blob/11cb6af6e5009c51794641905ca40ce5beec7fee/options/options.cc#L611-L621
    let mut block_options = BlockBasedOptions::default();
    // Overrides block size.
    block_options.set_block_size(block_size_bytes);
    // Configure a block cache.
    block_options.set_block_cache(&Cache::new_lru_cache(block_cache_size_mb << 20));
    // Set a bloomfilter with 1% false positive rate.
    block_options.set_bloom_filter(10.0, false);
    // From https://github.com/EighteenZi/rocksdb_wiki/blob/master/Block-Cache.md#caching-index-and-filter-blocks
    block_options.set_pin_l0_filter_and_index_blocks_in_cache(true);
    block_options
}

/// Opens a database with options, and a number of column families that are created if they do not exist.
#[instrument(level="debug", skip_all, fields(path = ?path.as_ref(), cf = ?opt_cfs), err)]
pub fn open_cf<P: AsRef<Path>>(
    path: P,
    db_options: Option<rocksdb::Options>,
    metric_conf: MetricConf,
    opt_cfs: &[&str],
) -> Result<Arc<RocksDB>, TypedStoreError> {
    let options = db_options.unwrap_or_else(|| default_db_options().options);
    let column_descriptors: Vec<_> = opt_cfs
        .iter()
        .map(|name| (*name, options.clone()))
        .collect();
    open_cf_opts(
        path,
        Some(options.clone()),
        metric_conf,
        &column_descriptors[..],
    )
}

fn prepare_db_options(db_options: Option<rocksdb::Options>) -> rocksdb::Options {
    // Customize database options
    let mut options = db_options.unwrap_or_else(|| default_db_options().options);
    options.create_if_missing(true);
    options.create_missing_column_families(true);
    options
}

/// Opens a database with options, and a number of column families with individual options that are created if they do not exist.
#[instrument(level="debug", skip_all, fields(path = ?path.as_ref()), err)]
pub fn open_cf_opts<P: AsRef<Path>>(
    path: P,
    db_options: Option<rocksdb::Options>,
    metric_conf: MetricConf,
    opt_cfs: &[(&str, rocksdb::Options)],
) -> Result<Arc<RocksDB>, TypedStoreError> {
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
        let options = prepare_db_options(db_options);
        let rocksdb = {
            rocksdb::DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
                &options,
                path,
                cfs.into_iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts)),
            )
            .map_err(typed_store_err_from_rocks_err)?
        };
        Ok(Arc::new(RocksDB::DBWithThreadMode(
            DBWithThreadModeWrapper::new(rocksdb, metric_conf, PathBuf::from(path)),
        )))
    })
}

/// Opens a database with options, and a number of column families with individual options that are created if they do not exist.
#[instrument(level="debug", skip_all, fields(path = ?path.as_ref()), err)]
pub fn open_cf_opts_transactional<P: AsRef<Path>>(
    path: P,
    db_options: Option<rocksdb::Options>,
    metric_conf: MetricConf,
    opt_cfs: &[(&str, rocksdb::Options)],
) -> Result<Arc<RocksDB>, TypedStoreError> {
    let path = path.as_ref();
    let cfs = populate_missing_cfs(opt_cfs, path).map_err(typed_store_err_from_rocks_err)?;
    // See comment above for explanation of why nondeterministic is necessary here.
    nondeterministic!({
        let options = prepare_db_options(db_options);
        let rocksdb = rocksdb::OptimisticTransactionDB::<MultiThreaded>::open_cf_descriptors(
            &options,
            path,
            cfs.into_iter()
                .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts)),
        )
        .map_err(typed_store_err_from_rocks_err)?;
        Ok(Arc::new(RocksDB::OptimisticTransactionDB(
            OptimisticTransactionDBWrapper::new(rocksdb, metric_conf, PathBuf::from(path)),
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
) -> Result<Arc<RocksDB>, TypedStoreError> {
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
        Ok(Arc::new(RocksDB::DBWithThreadMode(
            DBWithThreadModeWrapper::new(rocksdb, metric_conf, secondary_path),
        )))
    })
}

pub fn list_tables(path: std::path::PathBuf) -> eyre::Result<Vec<String>> {
    const DB_DEFAULT_CF_NAME: &str = "default";

    let opts = rocksdb::Options::default();
    rocksdb::DBWithThreadMode::<rocksdb::MultiThreaded>::list_cf(&opts, path)
        .map_err(|e| e.into())
        .map(|q| {
            q.iter()
                .filter_map(|s| {
                    // The `default` table is not used
                    if s != DB_DEFAULT_CF_NAME {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
}

/// TODO: Good description of why we're doing this : RocksDB stores keys in BE and has a seek operator on iterators, see `https://github.com/facebook/rocksdb/wiki/Iterator#introduction`
#[inline]
pub fn be_fix_int_ser<S>(t: &S) -> Result<Vec<u8>, TypedStoreError>
where
    S: ?Sized + serde::Serialize,
{
    bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding()
        .serialize(t)
        .map_err(typed_store_err_from_bincode_err)
}

#[derive(Clone)]
pub struct DBMapTableConfigMap(BTreeMap<String, DBOptions>);
impl DBMapTableConfigMap {
    pub fn new(map: BTreeMap<String, DBOptions>) -> Self {
        Self(map)
    }

    pub fn to_map(&self) -> BTreeMap<String, DBOptions> {
        self.0.clone()
    }
}

pub enum RocksDBAccessType {
    Primary,
    Secondary(Option<PathBuf>),
}

pub fn safe_drop_db(path: PathBuf) -> Result<(), rocksdb::Error> {
    rocksdb::DB::destroy(&rocksdb::Options::default(), path)
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

/// Given a vec<u8>, find the value which is one more than the vector
/// if the vector was a big endian number.
/// If the vector is already minimum, don't change it.
fn big_endian_saturating_add_one(v: &mut [u8]) {
    if is_max(v) {
        return;
    }
    for i in (0..v.len()).rev() {
        if v[i] == u8::MAX {
            v[i] = 0;
        } else {
            v[i] += 1;
            break;
        }
    }
}

/// Check if all the bytes in the vector are 0xFF
fn is_max(v: &[u8]) -> bool {
    v.iter().all(|&x| x == u8::MAX)
}

#[allow(clippy::assign_op_pattern)]
#[test]
fn test_helpers() {
    let v = vec![];
    assert!(is_max(&v));

    fn check_add(v: Vec<u8>) {
        let mut v = v;
        let num = Num32::from_big_endian(&v);
        big_endian_saturating_add_one(&mut v);
        assert!(num + 1 == Num32::from_big_endian(&v));
    }

    uint::construct_uint! {
        // 32 byte number
        struct Num32(4);
    }

    let mut v = vec![255; 32];
    big_endian_saturating_add_one(&mut v);
    assert!(Num32::MAX == Num32::from_big_endian(&v));

    check_add(vec![1; 32]);
    check_add(vec![6; 32]);
    check_add(vec![254; 32]);

    // TBD: More tests coming with randomized arrays
}
