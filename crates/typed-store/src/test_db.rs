// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::await_holding_lock)]

use std::{
    borrow::Borrow,
    collections::{btree_map::Iter, BTreeMap, HashMap, VecDeque},
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use crate::{
    rocks::{be_fix_int_ser, TypedStoreError},
    Map,
};
use bincode::Options;
use collectable::TryExtend;
use ouroboros::self_referencing;
use rand::distributions::{Alphanumeric, DistString};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{RwLockReadGuard, RwLockWriteGuard};

/// An interface to a btree map backed sally database. This is mainly intended
/// for tests and performing benchmark comparisons
#[derive(Clone, Debug)]
pub struct TestDB<K, V> {
    pub rows: Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    pub name: String,
    _phantom: PhantomData<fn(K) -> V>,
}

impl<K, V> TestDB<K, V> {
    pub fn open() -> Self {
        TestDB {
            rows: Arc::new(RwLock::new(BTreeMap::new())),
            name: Alphanumeric.sample_string(&mut rand::thread_rng(), 16),
            _phantom: PhantomData,
        }
    }
    pub fn batch(&self) -> TestDBWriteBatch {
        TestDBWriteBatch::default()
    }
}

#[self_referencing]
pub struct TestDBIter<'a, K, V> {
    rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<(K, V)>,
}

#[self_referencing]
pub struct TestDBKeys<'a, K> {
    rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<K>,
}

#[self_referencing]
pub struct TestDBValues<'a, V> {
    rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<V>,
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iterator for TestDBIter<'a, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        self.with_mut(|fields| {
            if let Some((raw_key, raw_value)) = fields.iter.next() {
                let key: K = config.deserialize(raw_key).ok().unwrap();
                let value: V = bincode::deserialize(raw_value).ok().unwrap();
                out = Some((key, value));
            }
        });
        out
    }
}

impl<'a, K: DeserializeOwned> Iterator for TestDBKeys<'a, K> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        self.with_mut(|fields| {
            let config = bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding();
            if let Some((raw_key, _)) = fields.iter.next() {
                let key: K = config.deserialize(raw_key).ok().unwrap();
                out = Some(key);
            }
        });
        out
    }
}

impl<'a, V: DeserializeOwned> Iterator for TestDBValues<'a, V> {
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        self.with_mut(|fields| {
            if let Some((_, raw_value)) = fields.iter.next() {
                let value: V = bincode::deserialize(raw_value).ok().unwrap();
                out = Some(value);
            }
        });
        out
    }
}

impl<'a, K, V> Map<'a, K, V> for TestDB<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = TypedStoreError;
    type Iterator = TestDBIter<'a, K, V>;
    type Keys = TestDBKeys<'a, K>;
    type Values = TestDBValues<'a, V>;

    fn contains_key(&self, key: &K) -> Result<bool, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        Ok(locked.contains_key(&raw_key))
    }

    fn get(&self, key: &K) -> Result<Option<V>, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        let res = locked.get(&raw_key);
        Ok(res.map(|raw_value| bincode::deserialize(raw_value).ok().unwrap()))
    }

    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        let res = locked.get(&raw_key);
        Ok(res.cloned())
    }

    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let raw_value = bincode::serialize(value)?;
        let mut locked = self.rows.write().unwrap();
        locked.insert(raw_key, raw_value);
        Ok(())
    }

    fn remove(&self, key: &K) -> Result<(), Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let mut locked = self.rows.write().unwrap();
        locked.remove(&raw_key);
        Ok(())
    }

    fn clear(&self) -> Result<(), Self::Error> {
        let mut locked = self.rows.write().unwrap();
        locked.clear();
        Ok(())
    }

    fn is_empty(&self) -> bool {
        let locked = self.rows.read().unwrap();
        locked.is_empty()
    }

    fn iter(&'a self) -> Self::Iterator {
        TestDBIterBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
        }
        .build()
    }

    fn keys(&'a self) -> Self::Keys {
        TestDBKeysBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
        }
        .build()
    }

    fn values(&'a self) -> Self::Values {
        TestDBValuesBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
        }
        .build()
    }

    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<J, K, U, V> TryExtend<(J, U)> for TestDB<K, V>
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
        let mut wb = self.batch();
        wb.insert_batch(self, iter)?;
        wb.write()
    }

    fn try_extend_from_slice(&mut self, slice: &[(J, U)]) -> Result<(), Self::Error> {
        let slice_of_refs = slice.iter().map(|(k, v)| (k.borrow(), v.borrow()));
        let mut wb = self.batch();
        wb.insert_batch(self, slice_of_refs)?;
        wb.write()
    }
}

pub type DeleteBatchPayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    Vec<Vec<u8>>,
);
pub type DeleteRangePayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    (Vec<u8>, Vec<u8>),
);
pub type InsertBatchPayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    Vec<(Vec<u8>, Vec<u8>)>,
);
type DBAndName = (Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>, String);

pub enum WriteBatchOp {
    DeleteBatch(DeleteBatchPayload),
    DeleteRange(DeleteRangePayload),
    InsertBatch(InsertBatchPayload),
}

#[derive(Default)]
pub struct TestDBWriteBatch {
    pub ops: VecDeque<WriteBatchOp>,
}

#[self_referencing]
pub struct DBLocked {
    db: Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    #[borrows(db)]
    #[covariant]
    db_guard: RwLockWriteGuard<'this, BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl TestDBWriteBatch {
    pub fn write(self) -> Result<(), TypedStoreError> {
        let mut dbs: Vec<DBAndName> = self
            .ops
            .iter()
            .map(|op| match op {
                WriteBatchOp::DeleteBatch((db, name, _)) => (db.clone(), name.clone()),
                WriteBatchOp::DeleteRange((db, name, _)) => (db.clone(), name.clone()),
                WriteBatchOp::InsertBatch((db, name, _)) => (db.clone(), name.clone()),
            })
            .collect();
        dbs.sort_by_key(|(_k, v)| v.clone());
        dbs.dedup_by_key(|(_k, v)| v.clone());
        // lock all databases
        let mut db_locks = HashMap::new();
        dbs.iter().for_each(|(db, name)| {
            if !db_locks.contains_key(name) {
                db_locks.insert(
                    name.clone(),
                    DBLockedBuilder {
                        db: db.clone(),
                        db_guard_builder: |db: &Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>| {
                            db.write().unwrap()
                        },
                    }
                    .build(),
                );
            }
        });
        self.ops.iter().for_each(|op| match op {
            WriteBatchOp::DeleteBatch((_, id, keys)) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    keys.iter().for_each(|key| {
                        db.remove(key);
                    });
                });
            }
            WriteBatchOp::DeleteRange((_, id, (from, to))) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    db.retain(|k, _| k < from || k >= to);
                });
            }
            WriteBatchOp::InsertBatch((_, id, key_values)) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    key_values.iter().for_each(|(k, v)| {
                        db.insert(k.clone(), v.clone());
                    });
                });
            }
        });
        // unlock in the reverse order
        dbs.iter().rev().for_each(|(_db, id)| {
            if db_locks.contains_key(id) {
                db_locks.remove(id);
            }
        });
        Ok(())
    }
    /// Deletes a set of keys given as an iterator
    pub fn delete_batch<J: Borrow<K>, K: Serialize, V>(
        &mut self,
        db: &TestDB<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<(), TypedStoreError> {
        self.ops.push_back(WriteBatchOp::DeleteBatch((
            db.rows.clone(),
            db.name.clone(),
            purged_vals
                .into_iter()
                .map(|key| be_fix_int_ser(&key.borrow()).unwrap())
                .collect(),
        )));
        Ok(())
    }
    /// Deletes a range of keys between `from` (inclusive) and `to` (non-inclusive)
    pub fn delete_range<'a, K: Serialize, V>(
        &mut self,
        db: &'a TestDB<K, V>,
        from: &K,
        to: &K,
    ) -> Result<(), TypedStoreError> {
        let raw_from = be_fix_int_ser(from.borrow()).unwrap();
        let raw_to = be_fix_int_ser(to.borrow()).unwrap();
        self.ops.push_back(WriteBatchOp::DeleteRange((
            db.rows.clone(),
            db.name.clone(),
            (raw_from, raw_to),
        )));
        Ok(())
    }
    /// inserts a range of (key, value) pairs given as an iterator
    pub fn insert_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &TestDB<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), TypedStoreError> {
        self.ops.push_back(WriteBatchOp::InsertBatch((
            db.rows.clone(),
            db.name.clone(),
            new_vals
                .into_iter()
                .map(|(key, value)| {
                    (
                        be_fix_int_ser(&key.borrow()).unwrap(),
                        bincode::serialize(&value.borrow()).unwrap(),
                    )
                })
                .collect(),
        )));
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{test_db::TestDB, Map};

    #[test]
    fn test_contains_key() {
        let db = TestDB::open();
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");
        assert!(db
            .contains_key(&123456789)
            .expect("Failed to call contains key"));
        assert!(!db
            .contains_key(&000000000)
            .expect("Failed to call contains key"));
    }

    #[test]
    fn test_get() {
        let db = TestDB::open();
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");
        assert_eq!(
            Some("123456789".to_string()),
            db.get(&123456789).expect("Failed to get")
        );
        assert_eq!(None, db.get(&000000000).expect("Failed to get"));
    }

    #[test]
    fn test_get_raw() {
        let db = TestDB::open();
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");

        let val_bytes = db
            .get_raw_bytes(&123456789)
            .expect("Failed to get_raw_bytes")
            .unwrap();

        assert_eq!(
            bincode::serialize(&"123456789".to_string()).unwrap(),
            val_bytes
        );
        assert_eq!(
            None,
            db.get_raw_bytes(&000000000)
                .expect("Failed to get_raw_bytes")
        );
    }

    #[test]
    fn test_multi_get() {
        let db = TestDB::open();
        db.insert(&123, &"123".to_string())
            .expect("Failed to insert");
        db.insert(&456, &"456".to_string())
            .expect("Failed to insert");

        let result = db.multi_get([123, 456, 789]).expect("Failed to multi get");

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Some("123".to_string()));
        assert_eq!(result[1], Some("456".to_string()));
        assert_eq!(result[2], None);
    }

    #[test]
    fn test_remove() {
        let db = TestDB::open();
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");
        assert!(db.get(&123456789).expect("Failed to get").is_some());

        db.remove(&123456789).expect("Failed to remove");
        assert!(db.get(&123456789).expect("Failed to get").is_none());
    }

    #[test]
    fn test_iter() {
        let db = TestDB::open();
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");

        let mut iter = db.iter();
        assert_eq!(Some((123456789, "123456789".to_string())), iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_iter_reverse() {
        let db = TestDB::open();
        db.insert(&1, &"1".to_string()).expect("Failed to insert");
        db.insert(&2, &"2".to_string()).expect("Failed to insert");
        db.insert(&3, &"3".to_string()).expect("Failed to insert");
        let mut iter = db.iter();

        assert_eq!(Some((1, "1".to_string())), iter.next());
        assert_eq!(Some((2, "2".to_string())), iter.next());
        assert_eq!(Some((3, "3".to_string())), iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_keys() {
        let db = TestDB::open();

        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");

        let mut keys = db.keys();
        assert_eq!(Some(123456789), keys.next());
        assert_eq!(None, keys.next());
    }

    #[test]
    fn test_values() {
        let db = TestDB::open();

        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");

        let mut values = db.values();
        assert_eq!(Some("123456789".to_string()), values.next());
        assert_eq!(None, values.next());
    }

    #[test]
    fn test_insert_batch() {
        let db = TestDB::open();
        let keys_vals = (1..100).map(|i| (i, i.to_string()));
        let mut wb = db.batch();
        wb.insert_batch(&db, keys_vals.clone())
            .expect("Failed to batch insert");
        wb.write().expect("Failed to execute batch");
        for (k, v) in keys_vals {
            let val = db.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }
    }

    #[test]
    fn test_insert_batch_across_cf() {
        let db_cf_1 = TestDB::open();
        let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));

        let db_cf_2 = TestDB::open();
        let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));

        let mut wb = db_cf_1.batch();
        wb.insert_batch(&db_cf_1, keys_vals_1.clone())
            .expect("Failed to batch insert");
        wb.insert_batch(&db_cf_2, keys_vals_2.clone())
            .expect("Failed to batch insert");
        wb.write().expect("Failed to execute batch");
        for (k, v) in keys_vals_1 {
            let val = db_cf_1.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }

        for (k, v) in keys_vals_2 {
            let val = db_cf_2.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }
    }

    #[test]
    fn test_delete_batch() {
        let db: TestDB<i32, String> = TestDB::open();

        let keys_vals = (1..100).map(|i| (i, i.to_string()));
        let mut wb = db.batch();
        wb.insert_batch(&db, keys_vals)
            .expect("Failed to batch insert");

        // delete the odd-index keys
        let deletion_keys = (1..100).step_by(2);
        wb.delete_batch(&db, deletion_keys)
            .expect("Failed to batch delete");

        wb.write().expect("Failed to execute batch");

        for k in db.keys() {
            assert_eq!(k % 2, 0);
        }
    }

    #[test]
    fn test_delete_range() {
        let db: TestDB<i32, String> = TestDB::open();

        // Note that the last element is (100, "100".to_owned()) here
        let keys_vals = (0..101).map(|i| (i, i.to_string()));
        let mut wb = db.batch();
        wb.insert_batch(&db, keys_vals)
            .expect("Failed to batch insert");

        wb.delete_range(&db, &50, &100)
            .expect("Failed to delete range");

        wb.write().expect("Failed to execute batch");

        for k in 0..50 {
            assert!(db.contains_key(&k).expect("Failed to query legal key"),);
        }
        for k in 50..100 {
            assert!(!db.contains_key(&k).expect("Failed to query legal key"));
        }

        // range operator is not inclusive of to
        assert!(db.contains_key(&100).expect("Failed to query legel key"));
    }

    #[test]
    fn test_clear() {
        let db: TestDB<i32, String> = TestDB::open();

        // Test clear of empty map
        let _ = db.clear();

        let keys_vals = (0..101).map(|i| (i, i.to_string()));
        let mut wb = db.batch();
        wb.insert_batch(&db, keys_vals)
            .expect("Failed to batch insert");

        wb.write().expect("Failed to execute batch");

        // Check we have multiple entries
        assert!(db.iter().count() > 1);
        let _ = db.clear();
        assert_eq!(db.iter().count(), 0);
        // Clear again to ensure safety when clearing empty map
        let _ = db.clear();
        assert_eq!(db.iter().count(), 0);
        // Clear with one item
        let _ = db.insert(&1, &"e".to_string());
        assert_eq!(db.iter().count(), 1);
        let _ = db.clear();
        assert_eq!(db.iter().count(), 0);
    }

    #[test]
    fn test_is_empty() {
        let db: TestDB<i32, String> = TestDB::open();

        // Test empty map is truly empty
        assert!(db.is_empty());
        let _ = db.clear();
        assert!(db.is_empty());

        let keys_vals = (0..101).map(|i| (i, i.to_string()));
        let mut wb = db.batch();
        wb.insert_batch(&db, keys_vals)
            .expect("Failed to batch insert");

        wb.write().expect("Failed to execute batch");

        // Check we have multiple entries and not empty
        assert!(db.iter().count() > 1);
        assert!(!db.is_empty());

        // Clear again to ensure empty works after clearing
        let _ = db.clear();
        assert_eq!(db.iter().count(), 0);
        assert!(db.is_empty());
    }

    #[test]
    fn test_multi_insert() {
        // Init a DB
        let db: TestDB<i32, String> = TestDB::open();

        // Create kv pairs
        let keys_vals = (0..101).map(|i| (i, i.to_string()));

        db.multi_insert(keys_vals.clone())
            .expect("Failed to multi-insert");

        for (k, v) in keys_vals {
            let val = db.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }
    }

    #[test]
    fn test_multi_remove() {
        // Init a DB
        let db: TestDB<i32, String> = TestDB::open();

        // Create kv pairs
        let keys_vals = (0..101).map(|i| (i, i.to_string()));

        db.multi_insert(keys_vals.clone())
            .expect("Failed to multi-insert");

        // Check insertion
        for (k, v) in keys_vals.clone() {
            let val = db.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }

        // Remove 50 items
        db.multi_remove(keys_vals.clone().map(|kv| kv.0).take(50))
            .expect("Failed to multi-remove");
        assert_eq!(db.iter().count(), 101 - 50);

        // Check that the remaining are present
        for (k, v) in keys_vals.skip(50) {
            let val = db.get(&k).expect("Failed to get inserted key");
            assert_eq!(Some(v), val);
        }
    }
}
