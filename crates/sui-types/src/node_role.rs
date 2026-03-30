// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// Represents the role of a node in the Sui network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    /// A validator that participates in consensus, signs checkpoints, and executes transactions
    Validator,
    /// An observer that streams consensus blocks but doesn't participate in consensus or sign checkpoints
    Observer,
    /// A full node that syncs via state sync only, without consensus participation
    FullNode,
}

impl NodeRole {
    /// Returns true if this node is a validator
    pub fn is_validator(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node is an observer
    pub fn is_observer(&self) -> bool {
        matches!(self, NodeRole::Observer)
    }

    /// Returns true if this node is a full node
    pub fn is_fullnode(&self) -> bool {
        matches!(self, NodeRole::FullNode)
    }

    /// Returns true if this node participates in consensus (either as validator or observer)
    pub fn participates_in_consensus(&self) -> bool {
        matches!(self, NodeRole::Validator | NodeRole::Observer)
    }

    /// Returns true if this node can sign checkpoints
    pub fn can_sign_checkpoints(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node can submit transactions to consensus
    pub fn can_submit_to_consensus(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node should run the consensus handler
    pub fn runs_consensus_handler(&self) -> bool {
        matches!(self, NodeRole::Validator | NodeRole::Observer)
    }

    /// Returns true if this node should participate in DKG ceremonies
    pub fn participates_in_dkg(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node can generate randomness partial signatures
    pub fn can_generate_randomness(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node should run the authority server (gRPC)
    pub fn runs_authority_server(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }

    /// Returns true if this node should build checkpoints locally
    pub fn builds_checkpoints(&self) -> bool {
        matches!(self, NodeRole::Validator | NodeRole::Observer)
    }

    /// Returns true if this node should recover pending transactions on restart
    pub fn recovers_pending_transactions(&self) -> bool {
        matches!(self, NodeRole::Validator)
    }
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeRole::Validator => write!(f, "Validator"),
            NodeRole::Observer => write!(f, "Observer"),
            NodeRole::FullNode => write!(f, "FullNode"),
        }
    }
}
