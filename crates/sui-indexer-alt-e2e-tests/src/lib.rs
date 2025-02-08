// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Context;
use sui_indexer_alt::{config::IndexerConfig, start_indexer};
use sui_indexer_alt_framework::{ingestion::ClientArgs, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs, start_rpc, RpcArgs,
};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    DbArgs,
};
use tokio::task::JoinHandle;
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

        let with_genesis = true;
        let indexer = start_indexer(
            db_args.clone(),
            indexer_args,
            client_args,
            indexer_config,
            with_genesis,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start indexer")?;

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

    /// Triggers cancellation of all downstream services, waits for them to stop, and cleans up the
    /// temporary database.
    pub async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.indexer.await;
        let _ = self.jsonrpc.await;
    }
}
