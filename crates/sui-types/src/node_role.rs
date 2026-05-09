// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::Committee;
use serde::{Deserialize, Serialize};

/// How a full node syncs data from the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FullNodeSyncMode {
    /// Syncs exclusively via the state-sync protocol.
    StateSyncOnly = 0,
    /// Streams consensus blocks for faster ingestion, in addition to state sync.
    ConsensusObserver = 1,
}

/// Represents the role a node plays in the network for a given epoch.
/// A node is either a Validator (in the committee) or a FullNode (not in
/// the committee). FullNodes carry a sync mode that determines whether
/// they also participate in consensus as an observer.
///
/// Behavior should be gated through capability methods (e.g. `runs_consensus()`) rather than matching on variants directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRole {
    /// A validator that participates in consensus, proposes blocks and signs checkpoints.
    Validator,
    /// A full node that serves RPC/indexing and syncs via the configured mode.
    FullNode(FullNodeSyncMode),
}

impl NodeRole {
    /// Determines the node role from committee membership and the configured sync mode.
    /// Used per-epoch in AuthorityPerEpochStore to derive the authoritative role.
    pub fn from_committee(
        committee: &Committee,
        name: &AuthorityName,
        fullnode_sync_mode: Option<FullNodeSyncMode>,
    ) -> Self {
        if committee.authority_exists(name) {
            NodeRole::Validator
        } else if let Some(mode) = fullnode_sync_mode {
            NodeRole::FullNode(mode)
        } else {
            NodeRole::FullNode(FullNodeSyncMode::StateSyncOnly)
        }
    }

    pub fn is_fullnode(&self) -> bool {
        matches!(self, Self::FullNode(_))
    }

    pub fn is_validator(&self) -> bool {
        matches!(self, Self::Validator)
    }
    // --- Capability methods ---

    /// Whether this node participates in the consensus protocol.
    pub fn runs_consensus(&self) -> bool {
        matches!(
            self,
            Self::Validator | Self::FullNode(FullNodeSyncMode::ConsensusObserver)
        )
    }

    /// Whether this node should create index stores for JSON-RPC and REST API.
    pub fn should_enable_index_processing(&self) -> bool {
        matches!(self, Self::FullNode(_))
    }

    /// Whether this node should expose HTTP/RPC servers (JSON-RPC, REST).
    pub fn should_run_rpc_servers(&self) -> bool {
        matches!(self, Self::FullNode(_))
    }
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validator => write!(f, "Validator"),
            Self::FullNode(FullNodeSyncMode::StateSyncOnly) => write!(f, "FullNode"),
            Self::FullNode(FullNodeSyncMode::ConsensusObserver) => {
                write!(f, "FullNode(ConsensusObserver)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_role() {
        let role = NodeRole::Validator;
        assert!(role.runs_consensus());
        assert!(!role.should_enable_index_processing());
        assert!(!role.should_run_rpc_servers());
    }

    #[test]
    fn test_consensus_observer_role() {
        let role = NodeRole::FullNode(FullNodeSyncMode::ConsensusObserver);
        assert!(role.runs_consensus());
        assert!(role.should_enable_index_processing());
        assert!(role.should_run_rpc_servers());
    }

    #[test]
    fn test_fullnode_state_sync_role() {
        let role = NodeRole::FullNode(FullNodeSyncMode::StateSyncOnly);
        assert!(!role.runs_consensus());
        assert!(role.should_enable_index_processing());
        assert!(role.should_run_rpc_servers());
    }
}
