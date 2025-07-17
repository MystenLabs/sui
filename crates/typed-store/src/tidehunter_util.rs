// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::DBMetrics;
use bincode::Options;
use prometheus::Registry;
use serde::de::DeserializeOwned;
use std::path::Path;
use std::sync::Arc;
use tidehunter::config::Config;
use tidehunter::db::Db;
use tidehunter::iterators::db_iterator::DbIterator;
use tidehunter::key_shape::{KeyShape, KeySpace};
use tidehunter::metrics::Metrics;
pub use tidehunter::{
    key_shape::{KeyIndexing, KeyShapeBuilder, KeySpaceConfig, KeyType},
    minibytes::Bytes,
    IndexWalPosition, WalPosition,
};
use typed_store_error::TypedStoreError;

#[derive(Clone)]
pub struct ThConfig {
    key_indexing: KeyIndexing,
    key_type: KeyType,
    mutexes: usize,
    config: KeySpaceConfig,
    pub prefix: Option<Vec<u8>>,
}

pub fn open(path: &Path, key_shape: KeyShape, db_name: String) -> Arc<Db> {
    std::fs::create_dir_all(path).expect("failed to open tidehunter db");
    let registry_service = &DBMetrics::get().registry_serivce;
    let registry = new_db_registry(db_name);
    registry_service.add(registry.clone());
    let metrics = Metrics::new_in(&registry);
    let db = Db::open(path, key_shape, Arc::new(thdb_config()), metrics)
        .expect("failed to open tidehunter db");
    db.start_periodic_snapshot();
    db
}

fn new_db_registry(name: String) -> Registry {
    let labels = [("db".to_string(), name)].into_iter().collect();
    Registry::new_custom(None, Some(labels)).expect("failed to create registry")
}

pub fn add_key_space(builder: &mut KeyShapeBuilder, name: &str, config: &ThConfig) -> KeySpace {
    builder.add_key_space_config_indexing(
        name,
        config.key_indexing.clone(),
        config.mutexes,
        config.key_type,
        config.config.clone(),
    )
}
fn thdb_config() -> Config {
    Config {
        frag_size: 1024 * 1024 * 1024,
        // run snapshot every 64 Gb written to wal
        snapshot_written_bytes: 64 * 1024 * 1024 * 1024,
        // force unloading dirty index entries if behind 128 Gb of wal
        snapshot_unload_threshold: 128 * 1024 * 1024 * 1024,
        unload_jitter_pct: 30,
        max_dirty_keys: 1024,
        max_maps: 8, // 8Gb of mapped space
        ..Config::default()
    }
}

pub(crate) fn apply_range_bounds(
    iterator: &mut DbIterator,
    lower_bound: Option<Vec<u8>>,
    upper_bound: Option<Vec<u8>>,
) {
    if let Some(lower_bound) = lower_bound {
        iterator.set_lower_bound(lower_bound);
    }
    if let Some(upper_bound) = upper_bound {
        iterator.set_upper_bound(upper_bound);
    }
}

pub(crate) fn transform_th_iterator<'a, K, V>(
    iterator: impl Iterator<
            Item = Result<
                (tidehunter::minibytes::Bytes, tidehunter::minibytes::Bytes),
                tidehunter::db::DbError,
            >,
        > + 'a,
    prefix: &'a Option<Vec<u8>>,
) -> impl Iterator<Item = Result<(K, V), TypedStoreError>> + 'a
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    let config = bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding();
    iterator.map(move |item| {
        item.map_err(|e| TypedStoreError::RocksDBError(format!("tidehunter error {:?}", e)))
            .and_then(|(raw_key, raw_value)| {
                let key = match prefix {
                    Some(prefix) => {
                        let mut buffer = Vec::with_capacity(raw_key.len() + prefix.len());
                        buffer.extend_from_slice(prefix);
                        buffer.extend_from_slice(&raw_key);
                        config.deserialize(&buffer)
                    }
                    None => config.deserialize(&raw_key),
                };
                let value = bcs::from_bytes(&raw_value);
                match (key, value) {
                    (Ok(k), Ok(v)) => Ok((k, v)),
                    (Err(e), _) => Err(TypedStoreError::SerializationError(e.to_string())),
                    (_, Err(e)) => Err(TypedStoreError::SerializationError(e.to_string())),
                }
            })
    })
}

pub(crate) fn transform_th_key(key: &[u8], prefix: &Option<Vec<u8>>) -> Vec<u8> {
    match prefix {
        Some(prefix) => key[prefix.len()..].to_vec(),
        None => key.to_vec(),
    }
}

pub(crate) fn typed_store_error_from_th_error(err: tidehunter::db::DbError) -> TypedStoreError {
    TypedStoreError::RocksDBError(format!("tidehunter error: {:?}", err))
}

impl ThConfig {
    pub fn new(key_size: usize, mutexes: usize, key_type: KeyType) -> Self {
        Self {
            key_indexing: KeyIndexing::fixed(key_size),
            mutexes,
            key_type,
            config: KeySpaceConfig::default(),
            prefix: None,
        }
    }
    pub fn new_with_indexing(key_indexing: KeyIndexing, mutexes: usize, key_type: KeyType) -> Self {
        Self {
            key_indexing,
            mutexes,
            key_type,
            config: KeySpaceConfig::default(),
            prefix: None,
        }
    }

    pub fn new_with_config_indexing(
        key_indexing: KeyIndexing,
        mutexes: usize,
        key_type: KeyType,
        config: KeySpaceConfig,
    ) -> Self {
        Self {
            key_indexing,
            mutexes,
            key_type,
            config,
            prefix: None,
        }
    }

    pub fn new_with_config(
        key_size: usize,
        mutexes: usize,
        key_type: KeyType,
        config: KeySpaceConfig,
    ) -> Self {
        Self::new_with_config_indexing(KeyIndexing::fixed(key_size), mutexes, key_type, config)
    }

    pub fn new_with_rm_prefix(
        key_size: usize,
        mutexes: usize,
        key_type: KeyType,
        config: KeySpaceConfig,
        prefix: Vec<u8>,
    ) -> Self {
        Self::new_with_rm_prefix_indexing(
            KeyIndexing::fixed(key_size),
            mutexes,
            key_type,
            config,
            prefix,
        )
    }

    pub fn new_with_rm_prefix_indexing(
        key_indexing: KeyIndexing,
        mutexes: usize,
        key_type: KeyType,
        config: KeySpaceConfig,
        prefix: Vec<u8>,
    ) -> Self {
        Self {
            key_indexing,
            mutexes,
            key_type,
            config,
            prefix: Some(prefix),
        }
    }

    pub fn with_config(mut self, config: KeySpaceConfig) -> Self {
        self.config = config;
        self
    }
}

pub fn default_cells_per_mutex() -> usize {
    2
}
