// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_package_resolver::Resolver;
use sui_pg_db::DbArgs;

use crate::{
    data::{
        package_resolver::{DbPackageStore, PackageCache, PackageResolver},
        reader::{ReadError, Reader},
    },
    metrics::RpcMetrics,
};

/// A bundle of different interfaces to data, for use by JSON-RPC method implementations.
#[derive(Clone)]
pub(crate) struct Context {
    /// Direct access to the database, for running SQL queries.
    reader: Reader,

    /// Access to the database for performing point look-ups. Access is through the same connection
    /// pool as `reader`, but this interface groups multiple point lookups into a single database
    /// query.
    loader: Arc<DataLoader<Reader>>,

    /// Access to the database for accessing information about types from their packages (again
    /// through the same connection pool as `reader`).
    package_resolver: PackageResolver,
}

impl Context {
    /// Set-up access to the database through all the interfaces available in the context.
    pub(crate) async fn new(
        db_args: DbArgs,
        metrics: Arc<RpcMetrics>,
        registry: &Registry,
    ) -> Result<Self, ReadError> {
        let reader = Reader::new(db_args, metrics, registry).await?;
        let loader = Arc::new(reader.as_data_loader());

        let store = PackageCache::new(DbPackageStore::new(loader.clone()));
        let package_resolver = Arc::new(Resolver::new(store));

        Ok(Self {
            reader,
            loader,
            package_resolver,
        })
    }

    /// For performing arbitrary SQL queries.
    pub(crate) fn reader(&self) -> &Reader {
        &self.reader
    }

    /// For performing point look-ups.
    pub(crate) fn loader(&self) -> &Arc<DataLoader<Reader>> {
        &self.loader
    }

    /// For querying type and function signature information.
    pub(crate) fn package_resolver(&self) -> &PackageResolver {
        &self.package_resolver
    }
}
