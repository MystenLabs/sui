// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod objects;
pub mod read;
pub mod write;

use anyhow::Context as _;

use sui_indexer_alt_jsonrpc::RpcService;
use sui_indexer_alt_jsonrpc::api::coin::Coins;
use sui_indexer_alt_jsonrpc::api::governance::Governance;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_reader::system_package_task::SystemPackageTask;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;

use crate::context::Context;
use sui_indexer_alt_jsonrpc::api::transactions::QueryTransactions;
use sui_indexer_alt_jsonrpc::api::transactions::Transactions;

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
    // indexer-alt-jsonrpc defined modules
    rpc.add_module(Coins(context.pg_context.clone()))?;
    rpc.add_module(Governance(context.pg_context.clone()))?;
    rpc.add_module(Transactions(context.pg_context.clone()))?;
    rpc.add_module(QueryTransactions(context.pg_context.clone()))?;

    // Local RPC defined modules
    rpc.add_module(objects::Objects(context.clone()))?;
    rpc.add_module(objects::QueryObjects(context.clone()))?;
    rpc.add_module(read::Read(context.clone()))?;
    rpc.add_module(write::Write(context.clone()))?;

    let s_metrics = metrics.run().await?;
    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    match h_rpc
        .attach(s_metrics)
        // .attach(s_system_package_task)
        .main()
        .await
    {
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
