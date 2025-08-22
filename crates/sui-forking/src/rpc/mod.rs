// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod write;

use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context;
use prometheus::Registry;
use tokio_util::sync::CancellationToken;

use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::RpcService;
use tokio::task::JoinHandle;

pub(crate) async fn start_rpc(
    simulacrum: Arc<RwLock<Simulacrum>>,
) -> anyhow::Result<JoinHandle<()>> {
    let cancel = CancellationToken::new();

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")?;

    let mut rpc = RpcService::new(RpcArgs::default(), &registry, cancel.clone()).unwrap();
    rpc.add_module(write::Write(simulacrum))?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
    }))
}
