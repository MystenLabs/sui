// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::dataloader::DataLoader;
use diesel::QueryDsl;
use prometheus::Registry;
use sui_indexer_alt_reader::consistent_reader::ConsistentReader;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeClient;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::package_resolver::DbPackageStore;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_schema::schema::kv_genesis;
use sui_package_resolver::Resolver;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use url::Url;

use crate::config::RpcConfig;
use crate::metrics::RpcMetrics;

/// A bundle of different interfaces to data, for use by JSON-RPC method implementations.
#[derive(Clone)]
pub(crate) struct Context {
    /// Access to the Consistent Store.
    consistent_reader: ConsistentReader,

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
    package_resolver: Arc<Resolver<Arc<PackageCache>>>,

    /// Access to the RPC's metrics.
    metrics: Arc<RpcMetrics>,

    /// Access to the RPC's configuration.
    config: Arc<RpcConfig>,

    /// The chain identifier, derived from the genesis checkpoint digest. This is `None` if no
    /// database is configured.
    chain_identifier: Option<ChainIdentifier>,

    /// Direct access to the fullnode client for executing transactions.
    fullnode_client: Option<FullnodeClient>,

    /// Access to the same `fullnode_client` through a `DataLoader` to batch requests.
    execution_loader: Option<Arc<DataLoader<FullnodeClient>>>,
}

impl Context {
    /// Set-up access to the stores through all the interfaces available in the context.
    ///
    /// KV lookups are routed based on `kv_args`: if a Bigtable instance is configured, lookups go
    /// directly to Bigtable; if a Ledger gRPC URL is configured, lookups go through kv-rpc;
    /// otherwise they fall back to Postgres.
    ///
    /// If `database_url` is `None`, the Postgres-backed interfaces will be set-up but will fail to
    /// accept any connections.
    pub(crate) async fn new(
        database_url: Option<Url>,
        db_args: DbArgs,
        kv_args: KvArgs,
        consistent_reader_args: ConsistentReaderArgs,
        fullnode_client: Option<FullnodeClient>,
        config: RpcConfig,
        metrics: Arc<RpcMetrics>,
        registry: &Registry,
    ) -> Result<Self, anyhow::Error> {
        let has_database = database_url.is_some();
        let pg_reader = PgReader::new(None, database_url, db_args, registry).await?;
        let pg_loader = Arc::new(pg_reader.as_data_loader());

        let kv_loader = KvLoader::from_kv_sources(
            kv_args
                .bigtable_reader("indexer-alt-jsonrpc".to_owned(), registry)
                .await?,
            kv_args
                .ledger_grpc_reader(Some("jsonrpc_ledger_grpc"), registry)
                .await?,
            pg_loader.clone(),
        );

        let store = Arc::new(PackageCache::new(DbPackageStore::new(pg_loader.clone())));
        let package_resolver = Arc::new(Resolver::new_with_limits(
            store,
            config.package_resolver.clone(),
        ));

        let consistent_reader =
            ConsistentReader::new(Some("jsonrpc_consistent"), consistent_reader_args, registry)
                .await?;

        let chain_identifier = if has_database {
            use kv_genesis::dsl as g;

            let mut conn = pg_reader
                .connect()
                .await
                .context("Failed to connect to the database")?;

            let genesis_digest_bytes: Vec<u8> = conn
                .first(g::kv_genesis.select(g::genesis_digest))
                .await
                .context("Failed to fetch genesis digest")?;

            let bytes: [u8; 32] = genesis_digest_bytes
                .try_into()
                .ok()
                .context("Invalid genesis digest length")?;

            Some(ChainIdentifier::from(CheckpointDigest::new(bytes)))
        } else {
            None
        };

        Ok(Self {
            consistent_reader,
            pg_reader,
            pg_loader,
            kv_loader,
            package_resolver,
            metrics,
            config: Arc::new(config),
            chain_identifier,
            fullnode_client: fullnode_client.clone(),
            execution_loader: fullnode_client.map(|client| Arc::new(client.as_data_loader())),
        })
    }

    /// For performing reads against the Consistent Store.
    pub(crate) fn consistent_reader(&self) -> &ConsistentReader {
        &self.consistent_reader
    }

    /// For performing arbitrary SQL queries on the Postgres db.
    pub(crate) fn pg_reader(&self) -> &PgReader {
        &self.pg_reader
    }

    /// For performing point look-ups on the Postgres db only.
    pub(crate) fn pg_loader(&self) -> &Arc<DataLoader<PgReader>> {
        &self.pg_loader
    }

    /// For performing point look-ups on the kv store. Depends on the configuration of the indexer,
    /// the kv store may be backed by either Bigtable or Postgres.
    pub(crate) fn kv_loader(&self) -> &KvLoader {
        &self.kv_loader
    }

    /// For querying type and function signature information.
    pub(crate) fn package_resolver(&self) -> &Resolver<Arc<PackageCache>> {
        self.package_resolver.as_ref()
    }

    /// Access to the RPC metrics.
    pub(crate) fn metrics(&self) -> &RpcMetrics {
        self.metrics.as_ref()
    }

    /// Access to the RPC configuration.
    pub(crate) fn config(&self) -> &RpcConfig {
        self.config.as_ref()
    }

    /// The chain identifier.
    pub(crate) fn chain_identifier(&self) -> Option<ChainIdentifier> {
        self.chain_identifier
    }

    pub(crate) fn fullnode_client(&self) -> anyhow::Result<&FullnodeClient> {
        self.fullnode_client
            .as_ref()
            .context("Fullnode gRPC client is not configured")
    }

    pub(crate) fn execution_loader(&self) -> anyhow::Result<&Arc<DataLoader<FullnodeClient>>> {
        self.execution_loader
            .as_ref()
            .context("Execution loader is not configured")
    }
}
