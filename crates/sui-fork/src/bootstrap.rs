// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::{Context, Result};
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_data_store::stores::DataStore;
use sui_data_store::{ObjectKey, ObjectStore as RemoteObjectStore, SetupStore, VersionQuery};
use sui_types::base_types::ObjectID;

use crate::store::ForkedStore;
use crate::{ForkConfig, ForkedNode};

/// System object IDs that must be seeded locally for execution to work.
pub(crate) const SYSTEM_OBJECT_IDS: &[ObjectID] = &[
    sui_types::SUI_SYSTEM_STATE_OBJECT_ID,
    sui_types::SUI_CLOCK_OBJECT_ID,
    sui_types::SUI_DENY_LIST_OBJECT_ID,
    sui_types::SUI_AUTHENTICATOR_STATE_OBJECT_ID,
    sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID,
];

/// System package IDs (Move stdlib, Sui framework, Sui system).
pub(crate) const SYSTEM_PACKAGE_IDS: &[ObjectID] = &[
    sui_types::MOVE_STDLIB_PACKAGE_ID,
    sui_types::SUI_FRAMEWORK_PACKAGE_ID,
    sui_types::SUI_SYSTEM_PACKAGE_ID,
];

pub async fn bootstrap(config: &ForkConfig) -> Result<ForkedNode> {
    let node = config.node.clone();
    let remote =
        DataStore::new(node.clone(), env!("CARGO_PKG_VERSION")).context("Failed to create DataStore")?;

    let chain_id = remote
        .setup(None)
        .context("Failed to get chain identifier from remote")?
        .unwrap_or_default();

    let fork_checkpoint = match config.checkpoint {
        Some(cp) => cp,
        // Query GraphQL for its latest *indexed* checkpoint. The full-node's JSON-RPC tip
        // is always ahead, so using it causes "Checkpoint N in the future" errors from GraphQL.
        // We also subtract a small lag buffer: the indexer reports the latest checkpoint header
        // before all object mutations at that height are fully queryable, so going back a few
        // hundred checkpoints (≈100 seconds on mainnet at ~3 checkpoints/s) avoids edge cases.
        None => {
            const CHECKPOINT_LAG_BUFFER: u64 = 200;
            let latest = remote.latest_indexed_checkpoint().await?;
            latest.saturating_sub(CHECKPOINT_LAG_BUFFER)
        }
    };

    tracing::info!(
        fork_checkpoint,
        %chain_id,
        "Bootstrapping fork"
    );

    let store = ForkedStore::new(remote, fork_checkpoint);

    // Seed critical system objects, ignoring any that don't exist at this checkpoint
    // (e.g., randomness state was introduced after the genesis checkpoint).
    seed_objects(&store, SYSTEM_OBJECT_IDS, fork_checkpoint)?;
    seed_objects(&store, SYSTEM_PACKAGE_IDS, fork_checkpoint)?;

    let system_state = simulacrum::store::SimulatorStore::get_system_state(&store);
    let epoch_state = simulacrum::epoch_state::EpochState::new(system_state);

    Ok(ForkedNode {
        epoch_state,
        store,
        deny_config: TransactionDenyConfig::default(),
        verifier_signing_config: VerifierSigningConfig::default(),
        fork_checkpoint,
        chain_id,
        node_config: node.clone(),
        snapshots: HashMap::new(),
        next_snapshot_id: 0,
        bridge_test_keypair: None,
    })
}


pub(crate) fn seed_objects(store: &ForkedStore, ids: &[ObjectID], fork_checkpoint: u64) -> Result<()> {
    let keys: Vec<ObjectKey> = ids
        .iter()
        .map(|id| ObjectKey {
            object_id: *id,
            version_query: VersionQuery::AtCheckpoint(fork_checkpoint),
        })
        .collect();
    let results = store
        .remote
        .get_objects(&keys)
        .context("Failed to seed system objects from remote")?;
    // Silently skip objects that don't exist at this checkpoint (e.g., features introduced later).
    for result in results.into_iter().flatten() {
        let (obj, _version) = result;
        store.insert_object(obj);
    }
    Ok(())
}
