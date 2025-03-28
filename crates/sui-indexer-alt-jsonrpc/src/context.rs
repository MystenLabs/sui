// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_package_resolver::Resolver;
use sui_pg_db::DbArgs;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    config::RpcConfig,
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

    /// Access to the RPC's metrics.
    metrics: Arc<RpcMetrics>,

    /// Access to the RPC's configuration.
    config: Arc<RpcConfig>,
}

impl Context {
    /// Set-up access to the database through all the interfaces available in the context. If
    /// `database_url` is `None`, the interfaces will be set-up but will fail to accept any
    /// connections.
    pub(crate) async fn new(
        database_url: Option<Url>,
        db_args: DbArgs,
        config: RpcConfig,
        metrics: Arc<RpcMetrics>,
        registry: &Registry,
        cancel: CancellationToken,
        slow_request_threshold: Duration,
    ) -> Result<Self, Error> {
        let pg_reader = PgReader::new(
            database_url,
            db_args,
            metrics.clone(),
            registry,
            cancel,
            slow_request_threshold,
        )
        .await?;
        let pg_loader = Arc::new(pg_reader.as_data_loader());

        let kv_loader = if let Some(config) = config.bigtable.clone() {
            let bigtable_reader =
                BigtableReader::new(config.instance_id, registry, slow_request_threshold).await?;
            KvLoader::new_with_bigtable(Arc::new(bigtable_reader.as_data_loader()))
        } else {
            KvLoader::new_with_pg(pg_loader.clone())
        };

        let store = PackageCache::new(DbPackageStore::new(pg_loader.clone()));
        let package_resolver = Arc::new(Resolver::new_with_limits(
            store,
            config.package_resolver.clone(),
        ));

        Ok(Self {
            pg_reader,
            pg_loader,
            kv_loader,
            package_resolver,
            metrics,
            config: Arc::new(config),
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

    /// Access to the RPC metrics.
    pub(crate) fn metrics(&self) -> &RpcMetrics {
        self.metrics.as_ref()
    }

    /// Access to the RPC configuration.
    pub(crate) fn config(&self) -> &RpcConfig {
        self.config.as_ref()
    }
}
