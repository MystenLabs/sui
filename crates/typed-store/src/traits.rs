// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{de::DeserializeOwned, Serialize};
use std::{borrow::Borrow, collections::BTreeMap, error::Error};

pub trait Map<'a, K, V>
where
    K: Serialize + DeserializeOwned + ?Sized,
    V: Serialize + DeserializeOwned,
{
    type Error: Error;
    type Iterator: Iterator<Item = (K, V)>;
    type Keys: Iterator<Item = K>;
    type Values: Iterator<Item = V>;

    /// Returns true if the map contains a value for the specified key.
    fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns the value for the given key from the map, if it exists.
    fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the raw value (bincode serialized bytes) for the given key from the map, if it exists.
    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Returns the value for the given key from the map, if it exists
    /// or the given default value if it does not.
    /// This method is not thread safe
    fn get_or_insert_unsafe<F: FnOnce() -> V>(
        &self,
        key: &K,
        default: F,
    ) -> Result<V, Self::Error> {
        self.get(key).and_then(|optv| match optv {
            Some(v) => Ok(v),
            None => {
                self.insert(key, &default())?;
                self.get(key).transpose().expect("default just inserted")
            }
        })
    }

    /// Inserts the given key-value pair into the map.
    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error>;

    /// Removes the entry for the given key from the map.
    fn remove(&self, key: &K) -> Result<(), Self::Error>;

    /// Removes every key-value pair from the map.
    fn clear(&self) -> Result<(), Self::Error>;

    /// Returns true if the map is empty, otherwise false.
    fn is_empty(&self) -> bool;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator over each key in the map.
    fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    fn values(&'a self) -> Self::Values;

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    fn multi_get<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter().map(|key| self.get(key.borrow())).collect()
    }

    /// Inserts key-value pairs, non-atomically.
    fn multi_insert<J, U>(
        &self,
        key_val_pairs: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
        U: Borrow<V>,
    {
        key_val_pairs
            .into_iter()
            .try_for_each(|(key, value)| self.insert(key.borrow(), value.borrow()))
    }

    /// Removes keys, non-atomically.
    fn multi_remove<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter()
            .try_for_each(|key| self.remove(key.borrow()))
    }

    /// Try to catch up with primary when running as secondary
    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error>;
}

pub trait TypedStoreDebug {
    /// Dump a DB table with pagination
    fn dump_table(
        &self,
        table_name: String,
        page_size: u16,
        page_number: usize,
    ) -> eyre::Result<BTreeMap<String, String>>;

    /// Get the name of the DB. This is simply the name of the struct
    fn primary_db_name(&self) -> String;

    /// Get a map of table names to key-value types
    fn describe_all_tables(&self) -> BTreeMap<String, (String, String)>;

    /// Count the entries in the table
    fn count_table_keys(&self, table_name: String) -> eyre::Result<usize>;
}
