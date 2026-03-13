// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::Context as _;
use tracing::info;

use sui_data_store::{
    CheckpointStore as _, SetupStore as _,
    stores::{DataStore, FileSystemStore, ReadThroughStore},
};
use sui_types::supported_protocol_versions::Chain;

use crate::network::ForkNetwork;

/// Create the data stores for the forking server, including a file system store for transactions
/// and a read-through store for objects that combines the file system store and the GraphQL RPC
/// store.
pub(super) fn initialize_data_store(
    fork_network: &ForkNetwork,
    fullnode_endpoint: &str,
    at_checkpoint: u64,
    data_ingestion_path: &Path,
    version: &'static str,
) -> Result<
    (
        FileSystemStore,
        ReadThroughStore<FileSystemStore, DataStore>,
    ),
    anyhow::Error,
> {
    let forking_path = format!(
        "forking/{}/forked_at_checkpoint_{}",
        fork_network.cache_namespace(),
        at_checkpoint
    );

    let node = fork_network.node();

    let fs_base_path = data_ingestion_path.join(forking_path);
    let fs = FileSystemStore::new_with_path(node.clone(), fs_base_path.clone())
        .context("failed to initialize file-system primary cache store")?;
    let gql_rpc_store = DataStore::new_with_endpoints(
        node.clone(),
        fork_network.gql_endpoint(),
        fullnode_endpoint,
        version,
    )
    .context("failed to initialize GraphQL/fullnode data store")?;
    let fs_store = FileSystemStore::new_with_path(node, fs_base_path.clone())
        .context("failed to initialize file-system checkpoint store")?;
    let fs_gql_store = ReadThroughStore::new(fs, gql_rpc_store);

    info!("Fs base path {:?}", fs_base_path.display());
    match fork_network {
        ForkNetwork::Mainnet => {
            fs_store
                .setup(Some(Chain::Mainnet.as_str().to_string()))
                .context("failed to initialize local mainnet node mapping")?;
        }
        ForkNetwork::Testnet => {
            fs_store
                .setup(Some(Chain::Testnet.as_str().to_string()))
                .context("failed to initialize local testnet node mapping")?;
        }
        ForkNetwork::Devnet | ForkNetwork::Custom(_) => {
            let chain_id = fs_gql_store
                .setup(None)
                .context("failed to initialize dynamic chain identifier mapping")?
                .with_context(|| {
                    format!(
                        "missing chain identifier while setting up {} data store",
                        fork_network.display_name()
                    )
                })?;
            info!(
                "Resolved dynamic chain identifier for {}: {}",
                fork_network.display_name(),
                chain_id
            );
        }
    }

    Ok((fs_store, fs_gql_store))
}

pub(super) fn determine_startup_checkpoint(
    checkpoint: Option<u64>,
    forked_at_checkpoint: u64,
    fs_store: &FileSystemStore,
) -> Result<u64, anyhow::Error> {
    let Some(requested_checkpoint) = checkpoint else {
        return Ok(forked_at_checkpoint);
    };

    let local_latest = fs_store
        .get_latest_checkpoint()
        .context("failed to inspect local checkpoint cache")?;

    match local_latest {
        None => Ok(requested_checkpoint),
        Some(checkpoint_data) => {
            let local_latest_sequence = checkpoint_data.summary.sequence_number;
            if local_latest_sequence < requested_checkpoint {
                anyhow::bail!(
                    "local fork cache for checkpoint {} is stale: latest local checkpoint is {}",
                    requested_checkpoint,
                    local_latest_sequence
                );
            }
            Ok(local_latest_sequence)
        }
    }
}
