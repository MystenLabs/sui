// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// pub mod objects;
pub mod read;
pub mod write;

use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context;
use prometheus::Registry;
use tokio_util::sync::CancellationToken;

use crate::forking_store::ForkingStore;
use rand::rngs::OsRng;
use reqwest::Url;
use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::NodeArgs;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::RpcService;
use sui_indexer_alt_jsonrpc::config::RpcConfig;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_types::supported_protocol_versions::Chain;
use tokio::task::JoinHandle;

pub(crate) async fn start_rpc(
    simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    protocol_version: u64,
    chain: Chain,
    database_url: Url,
) -> anyhow::Result<JoinHandle<()>> {
    let cancel = CancellationToken::new();

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")?;

    let mut rpc = sui_indexer_alt_jsonrpc::basic_rpc(
        Some(database_url),
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
    // rpc.add_module(objects::Objects {
    //     simulacrum: simulacrum.clone(),
    //     protocol_version,
    //     chain,
    // });
    rpc.add_module(read::Read {
        simulacrum: simulacrum.clone(),
        protocol_version,
        chain,
    })?;
    rpc.add_module(write::Write(simulacrum))?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
    }))
}
