use crate::rocks::be_fix_int_ser;
use crate::Map;
use bincode::Options;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::borrow::Borrow;
use std::fs;
use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};
use std::path::Path;
use std::sync::Arc;
use tidehunter::batch::WriteBatch;
use tidehunter::config::Config;
use tidehunter::db::{Db, DbError};
use tidehunter::metrics::Metrics;
use typed_store_error::TypedStoreError;

pub struct ThDbMap<K, V> {
    db: Arc<Db>,
    kf_spec: (u8, u8),
    _phantom: PhantomData<(K, V)>,
}

pub struct ThDbBatch {
    db: Arc<Db>,
    batch: WriteBatch,
}

impl<'a, K, V> ThDbMap<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn new(db: &Arc<Db>, kf_spec: (u8, u8)) -> Self {
        let db = db.clone();
        Self {
            db,
            kf_spec,
            _phantom: PhantomData,
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
            .last_in_range(&from_included, &to_included)
            .unwrap()?;
        Some((self.deserialize_key(&key), self.deserialize_value(&value)))
    }

    fn serialize_key(&self, k: impl Borrow<K>) -> Vec<u8> {
        let mut key = be_fix_int_ser(k.borrow()).unwrap();
        key.insert(0, self.kf_spec.0);
        key
    }

    fn serialize_value(&self, v: impl Borrow<V>) -> Vec<u8> {
        bincode::serialize(v.borrow()).unwrap()
    }

    fn deserialize_key(&self, k: &[u8]) -> K {
        deserialize_key(&k[1..])
    }

    fn checked_deserialize_key(&self, k: &[u8]) -> Option<K> {
        if k[0] == self.kf_spec.0 {
            Some(deserialize_key(&k[1..]))
        } else {
            None
        }
    }

    fn deserialize_value(&self, v: &[u8]) -> V {
        bincode::deserialize(v).unwrap()
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
            self.batch.write(db.serialize_key(k), db.serialize_value(v));
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
            self.batch.delete(db.serialize_key(key.borrow()));
        }
        Ok(self)
    }

    pub fn write(self) -> Result<(), TypedStoreError> {
        self.db
            .write_batch(self.batch)
            .map_err(typed_store_error_from_db_error)
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
            .exists(&key)
            .map_err(typed_store_error_from_db_error)
    }

    fn get(&self, key: &K) -> Result<Option<V>, Self::Error> {
        let key = self.serialize_key(key);
        let v = self.db.get(&key).map_err(typed_store_error_from_db_error)?;
        if let Some(v) = v {
            Ok(Some(self.deserialize_value(&v)))
        } else {
            Ok(None)
        }
    }

    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error> {
        let key = self.serialize_key(key);
        let v = self.db.get(&key).map_err(typed_store_error_from_db_error)?;
        Ok(v.map(|v| v.into_vec()))
    }

    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error> {
        let key = self.serialize_key(key);
        let value = self.serialize_value(value);
        self.db
            .insert(key, value)
            .map_err(typed_store_error_from_db_error)
    }

    fn remove(&self, key: &K) -> Result<(), Self::Error> {
        let key = self.serialize_key(key);
        self.db.remove(key).map_err(typed_store_error_from_db_error)
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
        // todo this is not super efficient
        Box::new(self.db.unordered_iterator().filter_map(|r| {
            let (k, v) = r.unwrap();
            let key = self.checked_deserialize_key(&k);
            key.map(|key| (key, self.deserialize_value(&v)))
        }))
    }

    fn iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::Iterator {
        todo!()
    }

    fn range_iter(&'a self, range: impl RangeBounds<K>) -> Self::Iterator {
        todo!()
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

        Box::new(self.db.range_ordered_iterator(start..end).map(|r| {
            let (k, v) = r.unwrap();
            let key = self
                .checked_deserialize_key(&k)
                .expect("Somehow got key from wrong key space");
            Ok((key, self.deserialize_value(&v)))
        }))
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
        .unwrap()
}

pub fn open_thdb(path: &Path) -> Arc<Db> {
    fs::create_dir_all(path).unwrap();
    let db = Db::open(path, Arc::new(Config::default()), Metrics::new()).unwrap();
    Arc::new(db)
}
