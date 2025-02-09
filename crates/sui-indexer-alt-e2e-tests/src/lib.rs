// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::Context;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt::{config::IndexerConfig, setup_indexer};
use sui_indexer_alt_framework::{ingestion::ClientArgs, schema::watermarks, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs, start_rpc, RpcArgs,
};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    Db, DbArgs,
};
use tokio::{task::JoinHandle, time::error::Elapsed};
use tokio_util::sync::CancellationToken;
use url::Url;

/// A collection of the off-chain services (an indexer, a database and a JSON-RPC server that reads
/// from that database), grouped together to simplify set-up and tear-down for tests.
///
/// The database is temporary, and will be cleaned up when the cluster is dropped, and the RPC is
/// set-up to listen on a random, available port, to avoid conflicts when multiple instances are
/// running concurrently in the same process.
pub struct OffchainCluster {
    /// The address the JSON-RPC server is listening on.
    rpc_listen_address: SocketAddr,

    /// Read access to the temporary database.
    db: Db,

    /// The pipelines that the indexer is populating.
    pipelines: Vec<&'static str>,

    /// A handle to the indexer task -- it will stop when the `cancel` token is triggered (or
    /// earlier of its own accord).
    indexer: JoinHandle<()>,

    /// A handle to the JSON-RPC server task -- it will stop when the `cancel` token is triggered
    /// (or earlier of its own accord).
    jsonrpc: JoinHandle<()>,

    /// Hold on to the database so it doesn't get dropped until the cluster is stopped.
    #[allow(unused)]
    database: TempDb,

    /// This token controls the clean up of the cluster.
    cancel: CancellationToken,
}

impl OffchainCluster {
    /// Construct a new off-chain cluster and spin up its constituent services.
    ///
    /// - `indexer_args`, `client_args`, and `indexer_config` control the indexer. In particular
    ///   `client_args` is used to configure the client that the indexer uses to fetch checkpoints.
    /// - `system_package_task_args`, and `rpc_config` control the JSON-RPC server.
    /// - `registry` is used to register metrics for the indexer and JSON-RPC server.
    pub async fn new(
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        system_package_task_args: SystemPackageTaskArgs,
        indexer_config: IndexerConfig,
        rpc_config: RpcConfig,
        registry: &prometheus::Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let rpc_port = get_available_port();
        let rpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), rpc_port);

        let database = TempDb::new().context("Failed to create database")?;

        let db_args = DbArgs {
            database_url: database.database().url().clone(),
            ..Default::default()
        };

        let rpc_args = RpcArgs {
            rpc_listen_address,
            ..Default::default()
        };

        let db = Db::for_read(db_args.clone())
            .await
            .context("Failed to connect to database")?;

        let with_genesis = true;
        let indexer = setup_indexer(
            db_args.clone(),
            indexer_args,
            client_args,
            indexer_config,
            with_genesis,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to setup indexer")?;

        let pipelines = indexer.pipelines().collect();
        let indexer = indexer.run().await.context("Failed to start indexer")?;

        let jsonrpc = start_rpc(
            db_args,
            rpc_args,
            system_package_task_args,
            rpc_config,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start JSON-RPC server")?;

        Ok(Self {
            rpc_listen_address,
            db,
            pipelines,
            indexer,
            jsonrpc,
            database,
            cancel,
        })
    }

    /// The URL to talk to the database on.
    pub fn db_url(&self) -> Url {
        self.database.database().url().clone()
    }

    /// The URL to send JSON-RPC requests to.
    pub fn rpc_url(&self) -> Url {
        Url::parse(&format!("http://{}/", self.rpc_listen_address))
            .expect("Failed to parse RPC URL")
    }

    /// Returns the latest checkpoint that we have all data for in the database, according to the
    /// watermarks table. Returns `None` if any of the expected pipelines are missing data.
    pub async fn latest_checkpoint(&self) -> anyhow::Result<Option<u64>> {
        use watermarks::dsl as w;

        let mut conn = self
            .db
            .connect()
            .await
            .context("Failed to connect to database")?;

        let latest: HashMap<String, i64> = w::watermarks
            .select((w::pipeline, w::checkpoint_hi_inclusive))
            .filter(w::pipeline.eq_any(&self.pipelines))
            .load(&mut conn)
            .await?
            .into_iter()
            .collect();

        for pipeline in &self.pipelines {
            if !latest.contains_key(*pipeline) {
                return Ok(None);
            }
        }

        Ok(latest.into_values().min().map(|l| l as u64))
    }

    /// Waits until the indexer has caught up to the given checkpoint, or the timeout is reached.
    pub async fn wait_for_checkpoint(
        &self,
        checkpoint: u64,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        tokio::time::timeout(timeout, async move {
            loop {
                if matches!(self.latest_checkpoint().await, Ok(Some(l)) if l >= checkpoint) {
                    break;
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        })
        .await
    }

    /// Triggers cancellation of all downstream services, waits for them to stop, and cleans up the
    /// temporary database.
    pub async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.indexer.await;
        let _ = self.jsonrpc.await;
    }
}
