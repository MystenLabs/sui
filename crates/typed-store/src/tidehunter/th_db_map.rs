use std::backtrace::Backtrace;
use crate::rocks::be_fix_int_ser;
use crate::{DBMetrics, Map};
use bincode::Options;
use eyre::format_err;
use prometheus::Registry;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::borrow::Borrow;
use std::fs;
use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use once_cell::sync::Lazy;
use tidehunter::batch::WriteBatch;
use tidehunter::config::Config;
use tidehunter::db::{Db, DbError};
use tidehunter::key_shape::{KeyShape, KeySpace};
use tidehunter::metrics::Metrics;
use typed_store_error::TypedStoreError;

pub struct ThDbMap<K, V> {
    db: Arc<Db>,
    ks: KeySpace,
    rm_prefix: Vec<u8>,
    _phantom: PhantomData<(K, V)>,
    metrics: Arc<DBMetrics>,
    pub log_get: bool,
    last_get_log: AtomicU64,
}

pub struct ThDbBatch {
    db: Arc<Db>,
    batch: WriteBatch,
}

static START: Lazy<Instant> = Lazy::new(||Instant::now());

impl<'a, K, V> ThDbMap<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn new(db: &Arc<Db>, ks: KeySpace) -> Self {
        Self::new_with_rm_prefix(db, ks, vec![])
    }

    pub fn new_with_rm_prefix(db: &Arc<Db>, ks: KeySpace, rm_prefix: Vec<u8>) -> Self {
        let metrics = DBMetrics::get().clone();
        let db = db.clone();
        
        Self {
            db,
            ks,
            rm_prefix,
            metrics,
            last_get_log: AtomicU64::new(0),
            log_get: false,
            _phantom: PhantomData,
        }
    }

    fn do_get(&self, key: &K, multi: bool) -> Result<Option<V>, <Self as Map<K, V>>::Error> {
        if self.log_get {
            let now = START.elapsed().as_secs();
            let prev = self.last_get_log.swap(now, Ordering::Relaxed);
            if prev != now {
                tracing::info!("reported_get_backtrace {multi} {:?} ", Backtrace::force_capture());
            }
        }
        let key = self.serialize_key(key);
        let v = self
            .db
            .get(self.ks, &key)
            .map_err(typed_store_error_from_db_error)?;
        if let Some(v) = v {
            Ok(Some(self.deserialize_value(&v)))
        } else {
            Ok(None)
        }
    }

    pub fn batch(&self) -> ThDbBatch {
        ThDbBatch {
            db: self.db.clone(),
            batch: WriteBatch::new(),
        }
    }

    pub fn last_in_range(&self, from_included: &K, to_included: &K) -> Option<(K, V)> {
        let from_included = self.serialize_key(from_included).into();
        let to_included = self.serialize_key(to_included).into();
        let (key, value) = self
            .db
            .last_in_range(self.ks, &from_included, &to_included)
            .unwrap()?;
        Some((self.deserialize_key(&key), self.deserialize_value(&value)))
    }

    fn serialize_key(&self, k: impl Borrow<K>) -> Vec<u8> {
        // todo use bytes slice instead
        let mut key = be_fix_int_ser(k.borrow()).unwrap();
        let result = key.split_off(self.rm_prefix.len());
        assert_eq!(key, self.rm_prefix, "Unexpected key prefix");
        result
    }

    fn serialize_value(&self, v: impl Borrow<V>) -> Vec<u8> {
        bincode::serialize(v.borrow()).unwrap()
    }

    fn deserialize_key(&self, k: &[u8]) -> K {
        if self.rm_prefix.is_empty() {
            deserialize_key(k)
        } else {
            let mut v = Vec::with_capacity(k.len() + self.rm_prefix.len());
            v.extend_from_slice(&self.rm_prefix);
            v.extend_from_slice(k);
            deserialize_key(&v)
        }
    }

    fn deserialize_value(&self, v: &[u8]) -> V {
        bincode::deserialize(v).unwrap()
    }

    fn ks_name(&self) -> &str {
        self.db.ks_name(self.ks)
    }
}

impl ThDbBatch {
    pub fn insert_batch<
        J: Borrow<K>,
        K: Serialize + DeserializeOwned,
        U: Borrow<V>,
        V: Serialize + DeserializeOwned,
    >(
        &mut self,
        db: &ThDbMap<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<&mut Self, TypedStoreError> {
        assert!(
            Arc::ptr_eq(&db.db, &self.db),
            "Cross db batch calls not allowed"
        );
        for (k, v) in new_vals {
            self.batch
                .write(db.ks, db.serialize_key(k), db.serialize_value(v));
        }
        Ok(self)
    }
    pub fn delete_batch<
        J: Borrow<K>,
        K: Serialize + DeserializeOwned,
        V: Serialize + DeserializeOwned,
    >(
        &mut self,
        db: &ThDbMap<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<&mut Self, TypedStoreError> {
        assert!(
            Arc::ptr_eq(&db.db, &self.db),
            "Cross db batch calls not allowed"
        );
        for key in purged_vals {
            self.batch.delete(db.ks, db.serialize_key(key.borrow()));
        }
        Ok(self)
    }

    pub fn write(self) -> Result<(), TypedStoreError> {
        self.db
            .write_batch(self.batch)
            .map_err(typed_store_error_from_db_error)
    }

    pub fn schedule_delete_range_inclusive<
        K: Serialize + DeserializeOwned,
        V: Serialize + DeserializeOwned,
    >(
        &mut self,
        db: &ThDbMap<K, V>,
        from: &K,
        to: &K,
    ) -> Result<(), TypedStoreError> {
        let delete: Vec<_> = db.range_iter(from..=to).map(|(k, _)| k).collect();
        self.delete_batch(&db, delete)?;
        Ok(())
    }
}

impl<'a, K, V> Map<'a, K, V> for ThDbMap<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = TypedStoreError;
    type Iterator = Box<dyn Iterator<Item = (K, V)> + 'a>;
    type SafeIterator = Box<dyn Iterator<Item = Result<(K, V), TypedStoreError>> + 'a>;
    type Keys = Box<dyn Iterator<Item = Result<K, TypedStoreError>>>;
    type Values = Box<dyn Iterator<Item = Result<V, TypedStoreError>>>;

    fn contains_key(&self, key: &K) -> Result<bool, Self::Error> {
        let key = self.serialize_key(key);
        self.db
            .exists(self.ks, &key)
            .map_err(typed_store_error_from_db_error)
    }

    fn get(&self, key: &K) -> Result<Option<V>, Self::Error> {
        let _timer = self
            .metrics
            .op_metrics
            .rocksdb_get_latency_seconds // todo different metric name?
            .with_label_values(&[&self.ks_name()])
            .start_timer();
        self.do_get(key, false)
    }

    fn multi_get<J>(&self, keys: impl IntoIterator<Item=J>) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>,
    {
        let _timer = self
            .metrics
            .op_metrics
            .rocksdb_multiget_latency_seconds // todo different metric name?
            .with_label_values(&[&self.ks_name()])
            .start_timer();
        // copy from Map::multi_get
        keys.into_iter().map(|key| self.do_get(key.borrow(), true)).collect()
    }

    fn multi_contains_keys<J>(&self, keys: impl IntoIterator<Item=J>) -> Result<Vec<bool>, Self::Error>
    where
        J: Borrow<K>,
    {
        let _timer = self
            .metrics
            .op_metrics
            .rocksdb_multiget_latency_seconds // todo different metric name?
            .with_label_values(&[&self.ks_name()])
            .start_timer();
        // copy from Map::multi_contains_key
        keys.into_iter()
            .map(|key| self.contains_key(key.borrow()))
            .collect()
    }

    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error> {
        let key = self.serialize_key(key);
        let v = self
            .db
            .get(self.ks, &key)
            .map_err(typed_store_error_from_db_error)?;
        Ok(v.map(|v| v.into_vec()))
    }

    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error> {
        let key = self.serialize_key(key);
        let value = self.serialize_value(value);
        self.db
            .insert(self.ks, key, value)
            .map_err(typed_store_error_from_db_error)
    }

    fn remove(&self, key: &K) -> Result<(), Self::Error> {
        let key = self.serialize_key(key);
        self.db
            .remove(self.ks, key)
            .map_err(typed_store_error_from_db_error)
    }

    fn unsafe_clear(&self) -> Result<(), Self::Error> {
        todo!()
    }

    fn delete_file_in_range(&self, from: &K, to: &K) -> Result<(), TypedStoreError> {
        todo!()
    }

    fn schedule_delete_all(&self) -> Result<(), TypedStoreError> {
        Ok(()) // todo implement
    }

    fn is_empty(&self) -> bool {
        self.db.is_empty()
    }

    fn unbounded_iter(&'a self) -> Self::Iterator {
        Box::new(self.db.unordered_iterator(self.ks).map(|r| {
            let (k, v) = r.unwrap();
            let key = self.deserialize_key(&k);
            let value = self.deserialize_value(&v);
            (key, value)
        }))
    }

    fn iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::Iterator {
        let lower_bound = lower_bound.expect("lower_bound required");
        let upper_bound = upper_bound.expect("upper_bound required");
        self.range_iter(lower_bound..=upper_bound)
    }

    fn range_iter(&'a self, range: impl RangeBounds<K>) -> Self::Iterator {
        Box::new(self.safe_range_iter(range).map(|r| r.unwrap()))
    }

    fn safe_iter(&'a self) -> Self::SafeIterator {
        todo!()
    }

    fn safe_iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::SafeIterator {
        let lower_bound = lower_bound.expect("lower_bound required");
        let upper_bound = upper_bound.expect("upper_bound required");
        self.safe_range_iter(lower_bound..=upper_bound)
    }

    fn safe_range_iter(&'a self, range: impl RangeBounds<K>) -> Self::SafeIterator {
        let start = range.start_bound();
        let end = range.end_bound();
        let Bound::Included(start) = start else {
            panic!("Only included bounds currently implemented");
        };
        let Bound::Included(end) = end else {
            panic!("Only included bounds currently implemented");
        };
        let start = self.serialize_key(start).into();
        let end = self.serialize_key(end).into();

        Box::new(
            self.db
                .range_ordered_iterator(self.ks, start..end)
                .map(|r| {
                    let (k, v) = r.unwrap();
                    let key = self.deserialize_key(&k);
                    let value = self.deserialize_value(&v);
                    Ok((key, value))
                }),
        )
    }

    fn keys(&'a self) -> Self::Keys {
        todo!()
    }

    fn values(&'a self) -> Self::Values {
        todo!()
    }

    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error> {
        todo!()
    }
}

fn typed_store_error_from_db_error(_err: DbError) -> TypedStoreError {
    TypedStoreError::RocksDBError("DbError".to_string())
}

fn deserialize_key<K: DeserializeOwned>(v: &[u8]) -> K {
    bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding()
        .deserialize(v)
        .map_err(|err| {
            format_err!(
                "Error {:?} while deserializing {:?}({} bytes)",
                err,
                v,
                v.len()
            )
        })
        .unwrap()
}

pub fn open_thdb(
    path: &Path,
    key_shape: KeyShape,
    registry: &Registry,
) -> Arc<Db> {
    fs::create_dir_all(path).unwrap();
    let metrics = Metrics::new_in(registry);
    let config = thdb_config();
    let config = Arc::new(config);
    let db = Db::open(path, key_shape, config, metrics).unwrap();
    let db = Arc::new(db);
    // db.start_periodic_snapshot();
    db
}

fn thdb_config() -> Config {
    let mut config = Config::default();
    modify_frag_size(&mut config);
    config.max_loaded_entries = 256;
    // run snapshot every 64 Gb written to wal
    config.snapshot_written_bytes = 64 * 1024 * 1024 * 1024;
    config.max_dirty_keys = 1024;
    config.max_maps = 32; // 32Gb of mapped space
    config
}

#[cfg(not(debug_assertions))]
pub fn default_cells_per_mutex() -> usize {
    8
}

#[cfg(debug_assertions)]
pub fn default_cells_per_mutex() -> usize {
    1
}

#[cfg(not(debug_assertions))]
fn modify_frag_size(config: &mut Config) {
    // Set frag size to 1Gb in prod
    config.frag_size = 1024 * 1024 * 1024;
}

#[cfg(debug_assertions)]
fn modify_frag_size(_config: &mut Config) {}
