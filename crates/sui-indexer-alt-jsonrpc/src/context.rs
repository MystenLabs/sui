// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_package_resolver::Resolver;
use sui_pg_db::DbArgs;

use crate::{
    config::BigtableConfig,
    data::{
        bigtable_reader::BigtableReader,
        error::Error,
        kv_loader::KvLoader,
        package_resolver::{DbPackageStore, PackageCache, PackageResolver},
        pg_reader::PgReader,
    },
    metrics::RpcMetrics,
};

/// A bundle of different interfaces to data, for use by JSON-RPC method implementations.
#[derive(Clone)]
pub(crate) struct Context {
    /// Direct access to the database, for running SQL queries.
    pg_reader: PgReader,

    /// Access to the database for performing point look-ups. Access is through the same connection
    /// pool as `reader`, but this interface groups multiple point lookups into a single database
    /// query.
    pg_loader: Arc<DataLoader<PgReader>>,

    /// Access to the kv store for performing point look-ups. This may either be backed by Bigtable
    /// or Postgres db, depending on the configuration.
    kv_loader: KvLoader,

    /// Access to the database for accessing information about types from their packages (again
    /// through the same connection pool as `reader`).
    package_resolver: PackageResolver,
}

impl Context {
    /// Set-up access to the database through all the interfaces available in the context.
    pub(crate) async fn new(
        db_args: DbArgs,
        bigtable_config: Option<BigtableConfig>,
        metrics: Arc<RpcMetrics>,
        registry: &Registry,
    ) -> Result<Self, Error> {
        let pg_reader = PgReader::new(db_args, metrics, registry).await?;
        let pg_loader = Arc::new(pg_reader.as_data_loader());

        let kv_loader = if let Some(config) = bigtable_config {
            let bigtable_reader = BigtableReader::new(config.instance_id).await?;
            KvLoader::new_with_bigtable(Arc::new(bigtable_reader.as_data_loader()))
        } else {
            KvLoader::new_with_pg(pg_loader.clone())
        };

        let store = PackageCache::new(DbPackageStore::new(pg_loader.clone()));
        let package_resolver = Arc::new(Resolver::new(store));

        Ok(Self {
            pg_reader,
            pg_loader,
            kv_loader,
            package_resolver,
        })
    }

    /// For performing arbitrary SQL queries on the Postgres db.
    pub(crate) fn pg_reader(&self) -> &PgReader {
        &self.pg_reader
    }

    /// For performing point look-ups on the Postgres db only.
    pub(crate) fn pg_loader(&self) -> &Arc<DataLoader<PgReader>> {
        &self.pg_loader
    }

    /// For performing point look-ups on the kv store.
    /// Depends on the configuration of the indexer, the kv store may be backed by
    /// eitherBigtable or Postgres.
    pub(crate) fn kv_loader(&self) -> &KvLoader {
        &self.kv_loader
    }

    /// For querying type and function signature information.
    pub(crate) fn package_resolver(&self) -> &PackageResolver {
        &self.package_resolver
    }
}
