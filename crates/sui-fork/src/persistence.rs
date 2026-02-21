// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! State dump and load support for `sui fork`.
//!
//! `dump()` serializes the current `ForkedNode` to a BCS file.
//! `load()` reconstructs a `ForkedNode` from such a file, creating a fresh
//! remote `DataStore` connection for objects not yet cached locally.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use simulacrum::epoch_state::EpochState;
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_data_store::node::Node;
use sui_data_store::stores::DataStore;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::Committee;
use sui_types::crypto::{AuthorityStrongQuorumSignInfo, EmptySignInfo};
use sui_types::digests::{CheckpointContentsDigest, CheckpointDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::message_envelope::TrustedEnvelope;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
};
use sui_types::object::Object;
use sui_types::transaction::SenderSignedData;

use crate::ForkedNode;
use crate::store::{ForkedStore, LocalState};

const STATE_DUMP_VERSION: u32 = 1;

type TrustedCheckpoint = TrustedEnvelope<CheckpointSummary, AuthorityStrongQuorumSignInfo>;
type TrustedTransaction = TrustedEnvelope<SenderSignedData, EmptySignInfo>;

#[derive(Serialize, Deserialize)]
pub(crate) struct StateDump {
    version: u32,
    fork_checkpoint: u64,
    chain_id: String,
    next_snapshot_id: u64,
    next_consensus_round: u64,
    local_state: SerializableLocalState,
}

/// Serializable form of `LocalState`. `VerifiedEnvelope` types are converted to
/// their `TrustedEnvelope` counterparts (which implement `Serialize`/`Deserialize`)
/// via `serializable_ref()` / `From` impls.
#[derive(Serialize, Deserialize)]
struct SerializableLocalState {
    checkpoints: BTreeMap<CheckpointSequenceNumber, TrustedCheckpoint>,
    checkpoint_digest_to_seq: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
    transactions: HashMap<TransactionDigest, TrustedTransaction>,
    effects: HashMap<TransactionDigest, TransactionEffects>,
    events: HashMap<TransactionDigest, TransactionEvents>,
    epoch_to_committee: Vec<Committee>,
    live_objects: HashMap<ObjectID, SequenceNumber>,
    objects: HashMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
    deleted_objects: HashSet<ObjectID>,
}

impl From<&LocalState> for SerializableLocalState {
    fn from(ls: &LocalState) -> Self {
        Self {
            checkpoints: ls
                .checkpoints
                .iter()
                .map(|(k, v)| (*k, v.serializable_ref().clone()))
                .collect(),
            checkpoint_digest_to_seq: ls.checkpoint_digest_to_seq.clone(),
            checkpoint_contents: ls.checkpoint_contents.clone(),
            transactions: ls
                .transactions
                .iter()
                .map(|(k, v)| (*k, v.serializable_ref().clone()))
                .collect(),
            effects: ls.effects.clone(),
            events: ls.events.clone(),
            epoch_to_committee: ls.epoch_to_committee.clone(),
            live_objects: ls.live_objects.clone(),
            objects: ls.objects.clone(),
            deleted_objects: ls.deleted_objects.clone(),
        }
    }
}

impl From<SerializableLocalState> for LocalState {
    fn from(s: SerializableLocalState) -> Self {
        Self {
            checkpoints: s
                .checkpoints
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            checkpoint_digest_to_seq: s.checkpoint_digest_to_seq,
            checkpoint_contents: s.checkpoint_contents,
            transactions: s
                .transactions
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            effects: s.effects,
            events: s.events,
            epoch_to_committee: s.epoch_to_committee,
            live_objects: s.live_objects,
            objects: s.objects,
            deleted_objects: s.deleted_objects,
        }
    }
}

/// Serialize the fork state to `path` (BCS format).
pub(crate) fn dump(node: &ForkedNode, path: &Path) -> Result<()> {
    let local = node.store.local.read().unwrap();
    let next_consensus_round = node.epoch_state.peek_next_consensus_round();
    let dump = StateDump {
        version: STATE_DUMP_VERSION,
        fork_checkpoint: node.fork_checkpoint,
        chain_id: node.chain_id.clone(),
        next_snapshot_id: node.next_snapshot_id,
        next_consensus_round,
        local_state: SerializableLocalState::from(&*local),
    };
    drop(local);
    let bytes = bcs::to_bytes(&dump).context("failed to BCS-encode state dump")?;
    std::fs::write(path, &bytes)
        .with_context(|| format!("failed to write state to {}", path.display()))?;
    Ok(())
}

/// Reconstruct a `ForkedNode` from a previously dumped state file.
/// A fresh remote `DataStore` is created from `node_config` to back the store.
pub(crate) fn load(path: &Path, node_config: &Node) -> Result<ForkedNode> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read state from {}", path.display()))?;
    let dump: StateDump =
        bcs::from_bytes(&bytes).context("failed to BCS-decode state dump")?;

    if dump.version != STATE_DUMP_VERSION {
        anyhow::bail!(
            "unsupported state dump version {}, expected {STATE_DUMP_VERSION}",
            dump.version
        );
    }

    let local_state: LocalState = dump.local_state.into();

    let remote = DataStore::new(node_config.clone(), env!("CARGO_PKG_VERSION"))
        .context("failed to create DataStore for state load")?;
    let store = ForkedStore::new(remote, dump.fork_checkpoint);
    *store.local.write().unwrap() = local_state;

    let system_state = simulacrum::store::SimulatorStore::get_system_state(&store);
    let mut epoch_state = EpochState::new(system_state);
    epoch_state.set_next_consensus_round(dump.next_consensus_round);

    Ok(ForkedNode {
        epoch_state,
        store,
        deny_config: TransactionDenyConfig::default(),
        verifier_signing_config: VerifierSigningConfig::default(),
        fork_checkpoint: dump.fork_checkpoint,
        chain_id: dump.chain_id,
        node_config: node_config.clone(),
        snapshots: HashMap::new(),
        next_snapshot_id: dump.next_snapshot_id,
        bridge_test_keypair: None,
    })
}
