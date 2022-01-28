use super::*;

use rocksdb::{DBWithThreadMode, MultiThreaded};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use typed_store::rocks::DBMap;
use typed_store::traits::Map;

/// Initialize
pub fn init_store(path: PathBuf, names: Vec<&str>) -> Arc<DBWithThreadMode<MultiThreaded>> {
    open_cf(&path, None, &names).expect("Cannot open DB.")
}

// Wrapper around DBMap for easy compat with other Rust maps
pub struct ClientStoreMap<K, V> {
    db_map: DBMap<K, V>,
}

impl<K, V> ClientStoreMap<K, V>
where
    K: Serialize + DeserializeOwned + std::cmp::Ord + std::clone::Clone,
    V: Serialize + DeserializeOwned + std::clone::Clone,
{
    // Reopen a rocks db cf
    pub fn new(db: &Arc<DBWithThreadMode<MultiThreaded>>, name: &str) -> ClientStoreMap<K, V> {
        Self {
            db_map: DBMap::reopen(db, Some(name)).expect(&format!("Cannot open {} CF.", name)[..]),
        }
    }
    /// Insert key,value pair
    pub fn insert(&self, k: K, v: V) {
        self.db_map.insert(&k, &v).unwrap();
    }
    /// Get value form key
    pub fn get(&self, k: &K) -> Option<V> {
        self.db_map.get(k).unwrap()
    }
    /// Check if map contains key
    pub fn contains_key(&self, k: &K) -> bool {
        self.db_map.contains_key(k).unwrap()
    }
    /// Remove key value par
    pub fn remove(&self, k: &K) {
        self.db_map.remove(k).unwrap();
    }
    /// Convenience fn to opulate from BTreeMap
    pub fn populate_from_btree_map(&self, b: BTreeMap<K, V>) {
        let _: Vec<_> = b
            .iter()
            .map(|(k, v)| self.insert(k.clone(), v.clone()))
            .collect();
    }
    /// Get a copy as a BTreeMap
    pub fn copy_as_btree_map(&self) -> BTreeMap<K, V> {
        let mut b = BTreeMap::new();
        b.extend(self.db_map.iter());
        b.clone()
    }
    /// Get a list of the keys
    pub fn key_list(&self) -> Vec<K> {
        self.db_map.keys().collect::<Vec<K>>()
    }
    /// Clear all elems in map
    /// Need to improve to use CF drop  for atomicity
    pub fn clear(&self) {
        // TODO: need to clear properly implement https://github.com/MystenLabs/mysten-infra/issues/7
        let batch = self
            .db_map
            .batch()
            .delete_batch(&self.db_map, self.key_list().into_iter())
            .unwrap();
        let _ = batch.write();
    }

    /// Get the length of the map
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.db_map.iter().count()
    }
    /// Check if map is empty
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.db_map.iter().count() == 0
    }
}
