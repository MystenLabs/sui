// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod objects;
pub mod read;
pub mod write;

use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context as _;
use prometheus::Registry;
use rand::rngs::OsRng;
use reqwest::Url;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::NodeArgs;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::RpcService;
use sui_indexer_alt_jsonrpc::config::RpcConfig;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_types::supported_protocol_versions::Chain;

use crate::context;
use crate::context::Context;
use crate::store::ForkingStore;
use sui_indexer_alt_jsonrpc::context::Context as PgContext;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_reader::system_package_task::SystemPackageTask;

pub(crate) async fn start_rpc(
    context: Context,
    mut rpc: RpcService,
    metrics: MetricsService,
) -> anyhow::Result<()> {
    let pg_context = context.clone().pg_context;
    let system_package_task = SystemPackageTask::new(
        SystemPackageTaskArgs::default(),
        pg_context.pg_reader().clone(),
        pg_context.package_resolver().package_store().clone(),
    );
    rpc.add_module(objects::Objects(context.clone()))?;
    rpc.add_module(objects::QueryObjects(context.clone()))?;
    rpc.add_module(read::Read {
        simulacrum: context.clone().simulacrum,
        protocol_version: context.protocol_version,
        chain: context.chain,
    })?;
    rpc.add_module(write::Write(context.clone().simulacrum))?;
    rpc.add_module(sui_indexer_alt_jsonrpc::api::checkpoints::Checkpoints(
        context.clone(),
    ))?;
    // rpc.add_module(Coins(context.clone()))?;

    let s_metrics = metrics.run().await?;
    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    match h_rpc.attach(s_metrics).main().await {
        Ok(()) | Err(sui_futures::service::Error::Terminated) => {}

        Err(sui_futures::service::Error::Aborted) => {
            std::process::exit(1);
        }

        Err(sui_futures::service::Error::Task(_)) => {
            std::process::exit(2);
        }
    }
    Ok(())
}
