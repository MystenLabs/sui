// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use forking_data_store::{
    CheckpointStoreWriter, EpochStoreWriter, Node, SetupStore,
    stores::{FileSystemStore, ForkingStore, GraphQLStore, ReadThroughStore},
};

use sui_forking::{
    Network, ServiceStore, StartupContext, resolve_resume_fork_checkpoint, start_server,
};

type BootstrapStore = ForkingStore<
    FileSystemStore,
    ReadThroughStore<FileSystemStore, ReadThroughStore<FileSystemStore, GraphQLStore>>,
    FileSystemStore,
    ReadThroughStore<FileSystemStore, ReadThroughStore<FileSystemStore, GraphQLStore>>,
>;

type HistoricalStore = ForkingStore<
    FileSystemStore,
    FileSystemStore,
    ReadThroughStore<FileSystemStore, GraphQLStore>,
    ReadThroughStore<FileSystemStore, GraphQLStore>,
>;

type LocalStore = ForkingStore<FileSystemStore, FileSystemStore, FileSystemStore, FileSystemStore>;

fn network_node(network: &Network) -> Node {
    match network {
        Network::Mainnet => Node::Mainnet,
        Network::Testnet => Node::Testnet,
        Network::Devnet => Node::Devnet,
        Network::Custom(url) => Node::Custom(url.clone()),
    }
}

fn filesystem_store(node: &Node, fork_origin_checkpoint: Option<u64>) -> Result<FileSystemStore> {
    match fork_origin_checkpoint {
        Some(fork_origin_checkpoint) => {
            FileSystemStore::new_for_fork(node.clone(), fork_origin_checkpoint)
        }
        None => FileSystemStore::new(node.clone()),
    }
}

fn bootstrap_store(
    node: &Node,
    startup_checkpoint: Option<u64>,
    resume_fork_origin: Option<u64>,
) -> Result<BootstrapStore> {
    let shared_epochs = ReadThroughStore::new(
        filesystem_store(node, None)?,
        GraphQLStore::new(node.clone(), env!("CARGO_PKG_VERSION"))?,
    );
    let shared_checkpoints = ReadThroughStore::new(
        filesystem_store(node, None)?,
        GraphQLStore::new(node.clone(), env!("CARGO_PKG_VERSION"))?,
    );
    let bootstrap_fork_origin =
        startup_checkpoint.map(|sequence| resume_fork_origin.unwrap_or(sequence));

    Ok(ForkingStore::new(
        filesystem_store(node, None)?,
        ReadThroughStore::new(
            filesystem_store(node, bootstrap_fork_origin)?,
            shared_epochs,
        ),
        filesystem_store(node, None)?,
        ReadThroughStore::new(
            filesystem_store(node, bootstrap_fork_origin)?,
            shared_checkpoints,
        ),
    ))
}

fn historical_store(node: &Node) -> Result<HistoricalStore> {
    Ok(ForkingStore::new(
        filesystem_store(node, None)?,
        filesystem_store(node, None)?,
        ReadThroughStore::new(
            filesystem_store(node, None)?,
            GraphQLStore::new(node.clone(), env!("CARGO_PKG_VERSION"))?,
        ),
        ReadThroughStore::new(
            filesystem_store(node, None)?,
            GraphQLStore::new(node.clone(), env!("CARGO_PKG_VERSION"))?,
        ),
    ))
}

fn local_store(node: &Node, startup: &StartupContext) -> Result<LocalStore> {
    let transactions = filesystem_store(node, Some(startup.fork_origin_checkpoint))?;
    transactions.setup(Some(startup.chain_id.clone()))?;

    let epochs = filesystem_store(node, Some(startup.fork_origin_checkpoint))?;
    epochs.setup(Some(startup.chain_id.clone()))?;
    epochs.write_epoch_info(startup.epoch_data.epoch_id, startup.epoch_data.clone())?;

    let objects = filesystem_store(node, Some(startup.fork_origin_checkpoint))?;
    objects.setup(Some(startup.chain_id.clone()))?;

    let checkpoints = filesystem_store(node, Some(startup.fork_origin_checkpoint))?;
    checkpoints.setup(Some(startup.chain_id.clone()))?;
    checkpoints.write_checkpoint(&startup.checkpoint)?;

    Ok(ForkingStore::new(
        transactions,
        epochs,
        objects,
        checkpoints,
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let network = Network::Mainnet;
    let startup_checkpoint = None;
    let node = network_node(&network);
    let resume_fork_origin =
        resolve_resume_fork_checkpoint(&FileSystemStore::base_path()?, startup_checkpoint)?;
    let bootstrap_store = bootstrap_store(&node, startup_checkpoint, resume_fork_origin)?;
    let historical_store = historical_store(&node)?;

    start_server(
        &bootstrap_store,
        startup_checkpoint,
        resume_fork_origin,
        move |startup, _config| {
            let local_store = local_store(&node, startup)?;
            Ok(ServiceStore::new(
                startup.fork_origin_checkpoint,
                historical_store,
                local_store,
            ))
        },
        "127.0.0.1",
        9001,
        None,
        None,
    )
    .await
}
