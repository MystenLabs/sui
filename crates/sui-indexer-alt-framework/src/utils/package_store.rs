// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! RPC-based package store implementation for fetching packages from a Sui full node.

use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use std::sync::Arc;
use sui_package_resolver::{
    error::Error as ResolverError, Package, PackageStore, PackageStoreWithLruCache, Result,
};
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;

/// A simple RPC-based package store that fetches packages from a Sui full node.
#[derive(Clone)]
pub struct RpcPackageStore {
    client: Client,
}

impl RpcPackageStore {
    /// Create a new RPC package store connected to the given full node URL.
    pub fn new(rpc_url: &str) -> anyhow::Result<Self> {
        let client = Client::new(rpc_url)
            .map_err(|e| anyhow::anyhow!("Failed to create RPC client: {}", e))?;
        Ok(Self { client })
    }

    /// Wraps this store with an LRU cache for better performance.
    ///
    /// # Example
    /// ```ignore
    /// let store = RpcPackageStore::new("https://fullnode.mainnet.sui.io:443")?;
    /// let cached_store = store.with_cache();
    /// let resolver = Resolver::new(cached_store);
    /// ```
    pub fn with_cache(self) -> PackageStoreWithLruCache<Self> {
        PackageStoreWithLruCache::new(self)
    }
}

#[async_trait]
impl PackageStore for RpcPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let object = self
            .client
            .clone()
            .get_object(ObjectID::from(id))
            .await
            .map_err(|_| ResolverError::PackageNotFound(id))?;

        Ok(Arc::new(Package::read_from_object(&object)?))
    }
}
