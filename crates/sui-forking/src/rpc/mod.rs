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

use crate::context::Context;
use sui_indexer_alt_jsonrpc::api::transactions::QueryTransactions;
use sui_indexer_alt_jsonrpc::api::transactions::Transactions;

pub(crate) async fn start_rpc(
    context: Context,
    mut rpc: RpcService,
    metrics: MetricsService,
) -> anyhow::Result<()> {
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

    // run services
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
