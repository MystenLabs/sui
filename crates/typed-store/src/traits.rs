// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::TypedStoreError;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::ops::RangeBounds;
use std::{borrow::Borrow, collections::BTreeMap, error::Error};

pub trait Map<'a, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error: Error;
    type Iterator: Iterator<Item = (K, V)>;
    type SafeIterator: Iterator<Item = Result<(K, V), TypedStoreError>>;
    type Keys: Iterator<Item = Result<K, TypedStoreError>>;
    type Values: Iterator<Item = Result<V, TypedStoreError>>;

    /// Returns true if the map contains a value for the specified key.
    fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns true if the map contains a value for the specified key.
    fn multi_contains_keys<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<bool>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter()
            .map(|key| self.contains_key(key.borrow()))
            .collect()
    }

    /// Returns the value for the given key from the map, if it exists.
    fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the raw value (serialized bytes) for the given key from the map, if it exists.
    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Inserts the given key-value pair into the map.
    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error>;

    /// Removes the entry for the given key from the map.
    fn remove(&self, key: &K) -> Result<(), Self::Error>;

    /// Removes every key-value pair from the map.
    fn unsafe_clear(&self) -> Result<(), Self::Error>;

    /// Removes every key-value pair from the map by deleting the underlying file.
    fn delete_file_in_range(&self, from: &K, to: &K) -> Result<(), TypedStoreError>;

    /// Uses delete range on the entire key range
    fn schedule_delete_all(&self) -> Result<(), TypedStoreError>;

    /// Returns true if the map is empty, otherwise false.
    fn is_empty(&self) -> bool;

    /// Returns an unbounded iterator visiting each key-value pair in the map.
    /// This is potentially unsafe as it can perform a full table scan
    fn unbounded_iter(&'a self) -> Self::Iterator;

    /// Returns an iterator visiting each key-value pair within the specified bounds in the map.
    fn iter_with_bounds(&'a self, lower_bound: Option<K>, upper_bound: Option<K>)
        -> Self::Iterator;

    /// Similar to `iter_with_bounds` but allows specifying inclusivity/exclusivity of ranges explicitly.
    /// TODO: find better name
    fn range_iter(&'a self, range: impl RangeBounds<K>) -> Self::Iterator;

    /// Same as `iter` but performs status check.
    fn safe_iter(&'a self) -> Self::SafeIterator;

    // Same as `iter_with_bounds` but performs status check.
    fn safe_iter_with_bounds(
        &'a self,
        lower_bound: Option<K>,
        upper_bound: Option<K>,
    ) -> Self::SafeIterator;

    // Same as `range_iter` but performs status check.
    fn safe_range_iter(&'a self, range: impl RangeBounds<K>) -> Self::SafeIterator;

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

    /// Returns a vector of raw values corresponding to the keys provided, non-atomically.
    fn multi_get_raw_bytes<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<Vec<u8>>>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter()
            .map(|key| self.get_raw_bytes(key.borrow()))
            .collect()
    }

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    fn chunked_multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
        _chunk_size: usize,
    ) -> Result<Vec<Option<V>>, Self::Error>
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

#[async_trait]
pub trait AsyncMap<'a, K, V>
where
    K: Serialize + DeserializeOwned + std::marker::Sync,
    V: Serialize + DeserializeOwned + std::marker::Sync + std::marker::Send,
{
    type Error: Error;
    type Iterator: Iterator<Item = Result<(K, V), TypedStoreError>>;
    type Keys: Iterator<Item = Result<K, TypedStoreError>>;
    type Values: Iterator<Item = Result<V, TypedStoreError>>;

    /// Returns true if the map contains a value for the specified key.
    async fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns the value for the given key from the map, if it exists.
    async fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the raw value (serialized bytes) for the given key from the map, if it exists.
    async fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Returns true if the map is empty, otherwise false.
    async fn is_empty(&self) -> bool;

    /// Returns an iterator visiting each key-value pair in the map.
    async fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator over each key in the map.
    async fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    async fn values(&'a self) -> Self::Values;

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    async fn multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J> + std::marker::Send,
    ) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>;

    /// Try to catch up with primary when running as secondary
    async fn try_catch_up_with_primary(&self) -> Result<(), Self::Error>;
}

pub struct TableSummary {
    pub num_keys: u64,
    pub key_bytes_total: usize,
    pub value_bytes_total: usize,
    pub key_hist: hdrhistogram::Histogram<u64>,
    pub value_hist: hdrhistogram::Histogram<u64>,
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

    /// Return table summary of the input table
    fn table_summary(&self, table_name: String) -> eyre::Result<TableSummary>;
}
