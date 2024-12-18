// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{create_dir_all, Metadata},
    path::Path,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::LOCAL_CACHE_DIR;

/// A generic cache for values that take time to compute.
pub struct CacheResult<T> {
    pub value: T,
    pub metadata: Metadata,
}

impl<T> CacheResult<T> {
    pub fn new(value: T, metadata: Metadata) -> Self {
        Self { value, metadata }
    }

    pub fn is_expired(&self) -> bool {
        self.metadata
            .modified()
            .unwrap()
            .elapsed()
            .unwrap()
            .as_secs()
            > 86400
    }
}

pub fn cache<T: Serialize + for<'a> Deserialize<'a>>(
    key: &str,
    value: T,
    cache_dir: &Path,
) -> Result<T> {
    let cache_file = cache_dir.join(key);
    std::fs::write(cache_file, serde_json::to_string(&value)?)?;
    debug!("Cached value for key: {}", key);
    Ok(value)
}

pub fn cache_local<T: Serialize + for<'a> Deserialize<'a>>(key: &str, value: T) -> Result<T> {
    create_dir_all(Path::new(LOCAL_CACHE_DIR))?;
    cache(key, value, Path::new(LOCAL_CACHE_DIR))
}

pub fn get_cached<T: for<'a> Deserialize<'a>>(
    key: &str,
    cache_dir: &Path,
) -> Result<CacheResult<T>> {
    let cache_file = cache_dir.join(key);
    let value = std::fs::read_to_string(&cache_file)?;
    debug!("Retrieved cached value for key: {}", key);
    Ok(CacheResult::new(
        serde_json::from_str(&value)?,
        std::fs::metadata(&cache_file)?,
    ))
}
pub fn get_cached_local<T: for<'a> Deserialize<'a>>(key: &str) -> Result<CacheResult<T>> {
    get_cached(key, Path::new(LOCAL_CACHE_DIR))
}
