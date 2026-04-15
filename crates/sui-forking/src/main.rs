// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use anyhow::Result;
use tracing::info;

use sui_forking::GraphQLClient;
use sui_forking::Node;
use sui_forking::startup;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

/// Default bind address for the embedded `sui-rpc-api` server.
const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // For this PR, we hardcode the node and checkpoint, but these will eventually be CLI args
    let node = Node::Mainnet;
    let node_str = node.chain().as_str();

    // For now, we fetch the latest checkpoint sequence number from the GraphQL store, but
    // eventually this will be a CLI arg.
    let checkpoint = GraphQLClient::new(node.clone(), VERSION)?
        .get_latest_checkpoint_sequence_number()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no checkpoints found for node {}", node_str))?;

    // Parsed up front so we fail fast on a bad address rather than after the
    // (slow) fork bootstrap. Accepts `--rpc-addr <addr>` or `--rpc-addr=<addr>`;
    // falls back to `SUI_FORKING_RPC_ADDR` then `DEFAULT_RPC_ADDR`. Kept
    // deliberately tiny — `clap` will get pulled in once the rest of the CLI
    // surface (node selection, checkpoint, etc.) actually exists.
    let rpc_addr: SocketAddr = parse_rpc_addr_arg()?
        .or_else(|| std::env::var("SUI_FORKING_RPC_ADDR").ok())
        .unwrap_or_else(|| DEFAULT_RPC_ADDR.to_string())
        .parse()?;

    let context = startup::initialize(node, checkpoint, VERSION).await?;
    println!(
        "Starting forked network from {} at checkpoint {} (rpc on {})",
        node_str, checkpoint, rpc_addr,
    );

    info!(
        "Starting forked network from {} at checkpoint {} (rpc on {})",
        node_str, checkpoint, rpc_addr,
    );

    let handle = tokio::spawn(sui_forking::startup::run(context, rpc_addr, VERSION));
    handle.await??;

    Ok(())
}

/// Walk argv looking for `--rpc-addr <value>` or `--rpc-addr=<value>`.
fn parse_rpc_addr_arg() -> Result<Option<String>> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--rpc-addr=") {
            return Ok(Some(value.to_string()));
        }
        if arg == "--rpc-addr" {
            return args
                .next()
                .ok_or_else(|| anyhow::anyhow!("--rpc-addr requires a value"))
                .map(Some);
        }
    }
    Ok(None)
}
