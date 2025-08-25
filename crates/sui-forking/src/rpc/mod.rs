// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod write;

use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context;
use prometheus::Registry;
use tokio_util::sync::CancellationToken;

use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::NodeArgs;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::RpcService;
use sui_indexer_alt_jsonrpc::config::RpcConfig;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use tokio::task::JoinHandle;

pub(crate) async fn start_rpc(
    simulacrum: Arc<RwLock<Simulacrum>>,
) -> anyhow::Result<JoinHandle<()>> {
    let cancel = CancellationToken::new();

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")?;

    let mut rpc = sui_indexer_alt_jsonrpc::basic_rpc(
        None,
        None,
        DbArgs::default(),
        BigtableArgs::default(),
        RpcArgs::default(),
        NodeArgs {
            fullnode_rpc_url: None,
        },
        SystemPackageTaskArgs::default(),
        RpcConfig::default(),
        &registry,
        cancel.child_token(),
    )
    .await?;
    // let mut rpc = RpcService::new(RpcArgs::default(), &registry, cancel.clone()).unwrap();
    rpc.add_module(write::Write(simulacrum))?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
    }))
}
