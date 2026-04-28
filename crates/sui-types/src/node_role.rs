// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Represents the roles a node plays in the network for a given epoch.
/// A node can have multiple active roles — for example, an observer-mode
/// full node is both a FullNode and an Observer.
///
/// Behavior should be gated through capability methods (e.g. `runs_consensus()`,
/// `can_submit_to_consensus()`) rather than checking individual role booleans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRole {
    is_validator: bool,
    is_observer: bool,
    is_fullnode: bool,
}

impl NodeRole {
    /// A validator that participates in consensus and signs checkpoints.
    pub fn validator() -> Self {
        Self {
            is_validator: true,
            is_observer: false,
            is_fullnode: false,
        }
    }

    /// An observer-mode full node: streams consensus blocks for faster ingestion
    /// while providing full-node services (JSON-RPC, indexing).
    pub fn observer() -> Self {
        Self {
            is_validator: false,
            is_observer: true,
            is_fullnode: true,
        }
    }

    /// A pure full node: syncs via state sync only.
    pub fn fullnode() -> Self {
        Self {
            is_validator: false,
            is_observer: false,
            is_fullnode: true,
        }
    }

    // --- Role checks ---

    pub fn is_validator(&self) -> bool {
        self.is_validator
    }

    pub fn is_observer(&self) -> bool {
        self.is_observer
    }

    pub fn is_fullnode(&self) -> bool {
        self.is_fullnode
    }

    // --- Capability methods ---

    /// Whether this node participates in the consensus protocol (validator or observer).
    pub fn runs_consensus(&self) -> bool {
        self.is_validator || self.is_observer
    }

    /// Whether this node can submit transactions and checkpoint signatures to consensus.
    pub fn can_submit_to_consensus(&self) -> bool {
        self.is_validator
    }

    /// Whether this node should run fork detection and recovery at startup.
    pub fn should_check_forks(&self) -> bool {
        self.is_validator || self.is_observer
    }

    /// Whether this node should create index stores for JSON-RPC and REST API.
    pub fn should_enable_index_processing(&self) -> bool {
        self.is_fullnode
    }

    /// Whether this node should expose the gRPC validator service.
    pub fn should_run_validator_service(&self) -> bool {
        self.is_validator
    }

    /// Whether this node should run the authority overload monitor.
    pub fn should_run_overload_monitor(&self) -> bool {
        self.is_validator
    }

    /// Whether this node should run the randomness manager (DKG).
    pub fn should_run_randomness(&self) -> bool {
        self.is_validator || self.is_observer
    }

    /// Whether this node should submit JWK updates to consensus.
    pub fn should_run_jwk_updater(&self) -> bool {
        self.is_validator
    }

    /// Whether this node should run the execution time observer.
    pub fn should_run_execution_time_observer(&self) -> bool {
        self.is_validator
    }

    /// Whether this node should expose HTTP/RPC servers (JSON-RPC, REST).
    pub fn should_run_rpc_servers(&self) -> bool {
        self.is_fullnode
    }
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_validator {
            write!(f, "Validator")
        } else if self.is_observer {
            write!(f, "Observer")
        } else {
            write!(f, "FullNode")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_role() {
        let role = NodeRole::validator();
        assert!(role.is_validator());
        assert!(!role.is_observer());
        assert!(!role.is_fullnode());
        assert!(role.runs_consensus());
        assert!(role.can_submit_to_consensus());
        assert!(role.should_check_forks());
        assert!(!role.should_enable_index_processing());
    }

    #[test]
    fn test_observer_role() {
        let role = NodeRole::observer();
        assert!(!role.is_validator());
        assert!(role.is_observer());
        assert!(role.is_fullnode());
        assert!(role.runs_consensus());
        assert!(!role.can_submit_to_consensus());
        assert!(role.should_check_forks());
        assert!(role.should_enable_index_processing());
        assert!(role.should_run_randomness());
    }

    #[test]
    fn test_fullnode_role() {
        let role = NodeRole::fullnode();
        assert!(!role.is_validator());
        assert!(!role.is_observer());
        assert!(role.is_fullnode());
        assert!(!role.runs_consensus());
        assert!(!role.can_submit_to_consensus());
        assert!(!role.should_check_forks());
        assert!(role.should_enable_index_processing());
    }
}
