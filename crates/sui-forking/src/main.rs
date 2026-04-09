// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tracing::info;

use forking_data_store::Node;
use forking_data_store::stores::GraphQLStore;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // For this PR, we hardcode the node and checkpoint, but these will eventually be CLI args
    let node: Node = Node::Mainnet;
    let node_str = node.chain().as_str();

    // For now, we fetch the latest checkpoint sequence number from the GraphQL store, but
    // eventually this will be a CLI arg.
    let checkpoint =
        GraphQLStore::new(node.clone(), VERSION)?.get_latest_checkpoint_sequence_number()?;

    let context = sui_forking::startup::initialize(node, checkpoint, VERSION).await?;
    println!(
        "Starting forked network from {} at checkpoint {}",
        node_str, checkpoint,
    );

    info!(
        "Starting forked network from {} at checkpoint {}",
        node_str, checkpoint,
    );

    let handle = tokio::spawn(sui_forking::startup::run(context));
    handle.await??;

    Ok(())
}
