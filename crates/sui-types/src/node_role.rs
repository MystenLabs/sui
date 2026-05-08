// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::Committee;

/// How a full node syncs data from the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
/// Behavior should be gated through capability methods (e.g. `runs_consensus()`,
/// `can_propose_transactions()`) rather than matching on variants directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRole {
    /// A validator that participates in consensus and signs checkpoints.
    Validator,
    /// A full node that serves RPC/indexing and syncs via the configured mode.
    FullNode(FullNodeSyncMode),
}

impl NodeRole {
    /// Determines the node role from committee membership and observer configuration.
    /// This is the single source of truth for role derivation — used both at startup
    /// (with the latest committee from CommitteeStore) and per-epoch (in AuthorityPerEpochStore).
    pub fn from_committee(
        committee: &Committee,
        name: &AuthorityName,
        has_observer_config: bool,
    ) -> Self {
        if committee.authority_exists(name) {
            NodeRole::Validator
        } else if has_observer_config {
            NodeRole::FullNode(FullNodeSyncMode::ConsensusObserver)
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

    /// Whether this node can propose transactions and checkpoint signatures to consensus.
    pub fn can_propose_transactions(&self) -> bool {
        matches!(self, Self::Validator)
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
        assert!(role.can_propose_transactions());
        assert!(!role.should_enable_index_processing());
        assert!(!role.should_run_rpc_servers());
    }

    #[test]
    fn test_consensus_observer_role() {
        let role = NodeRole::FullNode(FullNodeSyncMode::ConsensusObserver);
        assert!(role.runs_consensus());
        assert!(!role.can_propose_transactions());
        assert!(role.should_enable_index_processing());
        assert!(role.should_run_rpc_servers());
    }

    #[test]
    fn test_fullnode_state_sync_role() {
        let role = NodeRole::FullNode(FullNodeSyncMode::StateSyncOnly);
        assert!(!role.runs_consensus());
        assert!(!role.can_propose_transactions());
        assert!(role.should_enable_index_processing());
        assert!(role.should_run_rpc_servers());
    }
}
