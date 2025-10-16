// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Package store implementation that fetches packages from a Sui fullnode via RPC.
//!
//! This provides a convenient way to resolve package types
//! by fetching package data directly from the network.

use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use std::sync::Arc;
use sui_package_resolver::{error::Error, Package, PackageStore, PackageStoreWithLruCache};
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;

/// A PackageStore implementation that fetches packages from a Sui fullnode via RPC.
#[derive(Clone)]
pub struct RpcPackageStore {
    client: Client,
}

impl RpcPackageStore {
    /// Create a new RpcPackageStore with the given RPC URL.
    ///
    /// # Example
    /// ```ignore
    /// let store = RpcPackageStore::new("https://fullnode.testnet.sui.io:443");
    /// ```
    pub fn new(rpc_url: &str) -> Self {
        let client = Client::new(rpc_url).expect("Failed to create RPC client - invalid URL");
        Self { client }
    }

    /// Add an LRU cache layer to this package store for improved performance.
    ///
    /// This is particularly useful when processing many events or objects from
    /// the same packages, as it avoids redundant package fetches over the network.
    ///
    /// # Example
    /// ```ignore
    /// use sui_rpc_resolver::package_store::RpcPackageStore;
    /// use sui_package_resolver::Resolver;
    ///
    /// let store = RpcPackageStore::new("https://fullnode.testnet.sui.io:443");
    /// let cached_store = store.with_cache();
    /// let resolver = Resolver::new(cached_store);
    /// ```
    pub fn with_cache(self) -> PackageStoreWithLruCache<Self> {
        PackageStoreWithLruCache::new(self)
    }
}

#[async_trait]
impl PackageStore for RpcPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>, Error> {
        // Fetch the object from the RPC client (client needs to be cloned because get_object takes ownership)
        let object = self
            .client
            .clone()
            .get_object(ObjectID::from(id))
            .await
            .map_err(|_| Error::PackageNotFound(id))?;

        // Convert the object to a Package
        Ok(Arc::new(Package::read_from_object(&object)?))
    }
}
