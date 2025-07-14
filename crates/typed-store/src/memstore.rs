// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::Options;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, Bound, HashMap};
use std::sync::{Arc, RwLock};
use typed_store_error::TypedStoreError;

type InMemoryStoreInternal = Arc<RwLock<HashMap<String, BTreeMap<Vec<u8>, Vec<u8>>>>>;

#[derive(Clone, Debug)]
pub struct InMemoryDB {
    data: InMemoryStoreInternal,
}

#[derive(Clone, Debug)]
enum InMemoryChange {
    Delete((String, Vec<u8>)),
    Put((String, Vec<u8>, Vec<u8>)),
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryBatch {
    data: Vec<InMemoryChange>,
}

impl InMemoryBatch {
    pub fn delete_cf<K: AsRef<[u8]>>(&mut self, cf_name: &str, key: K) {
        self.data.push(InMemoryChange::Delete((
            cf_name.to_string(),
            key.as_ref().to_vec(),
        )));
    }

    pub fn put_cf<K, V>(&mut self, cf_name: &str, key: K, value: V)
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.data.push(InMemoryChange::Put((
            cf_name.to_string(),
            key.as_ref().to_vec(),
            value.as_ref().to_vec(),
        )));
    }
}

impl InMemoryDB {
    pub fn get<K: AsRef<[u8]>>(&self, cf_name: &str, key: K) -> Option<Vec<u8>> {
        let data = self.data.read().expect("can't read data");
        match data.get(cf_name) {
            Some(cf) => cf.get(key.as_ref()).cloned(),
            None => None,
        }
    }

    pub fn multi_get<I, K>(&self, cf_name: &str, keys: I) -> Vec<Option<Vec<u8>>>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<[u8]>,
    {
        let data = self.data.read().expect("can't read data");
        match data.get(cf_name) {
            Some(cf) => keys
                .into_iter()
                .map(|k| cf.get(k.as_ref()).cloned())
                .collect(),
            None => vec![],
        }
    }

    pub fn delete(&self, cf_name: &str, key: &[u8]) {
        let mut data = self.data.write().expect("can't write data");
        data.entry(cf_name.to_string()).or_default().remove(key);
    }

    pub fn put(&self, cf_name: &str, key: Vec<u8>, value: Vec<u8>) {
        let mut data = self.data.write().expect("can't write data");
        data.entry(cf_name.to_string())
            .or_default()
            .insert(key, value);
    }

    pub fn write(&self, batch: InMemoryBatch) {
        for change in batch.data {
            match change {
                InMemoryChange::Delete((cf_name, key)) => self.delete(&cf_name, &key),
                InMemoryChange::Put((cf_name, key, value)) => self.put(&cf_name, key, value),
            }
        }
    }

    pub fn drop_cf(&self, name: &str) {
        self.data.write().expect("can't write data").remove(name);
    }

    pub fn iterator<K, V>(
        &self,
        cf_name: &str,
        lower_bound: Option<Vec<u8>>,
        upper_bound: Option<Vec<u8>>,
        reverse: bool,
    ) -> Box<dyn Iterator<Item = Result<(K, V), TypedStoreError>> + '_>
    where
        K: DeserializeOwned,
        V: DeserializeOwned,
    {
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        let lower_bound = lower_bound.map(Bound::Included).unwrap_or(Bound::Unbounded);
        let upper_bound = upper_bound.map(Bound::Included).unwrap_or(Bound::Unbounded);

        let data = self.data.read().expect("can't read data");
        let mut section: Vec<_> = data
            .get(cf_name)
            .unwrap_or(&BTreeMap::new())
            .range((lower_bound, upper_bound))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if reverse {
            section.reverse();
        }
        Box::new(section.into_iter().map(move |(raw_key, raw_value)| {
            let key = config.deserialize(&raw_key).unwrap();
            let value = bcs::from_bytes(&raw_value).unwrap();
            Ok((key, value))
        }))
    }
}
