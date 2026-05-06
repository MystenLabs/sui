// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Identifies the chain of the network.
/// Mirrors `sui_protocol_config::Chain`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChainType {
    #[default]
    Unknown,
    Testnet,
    Mainnet,
}

/// Protocol configuration values that consensus reads. This is a standalone
/// struct so that `consensus-core` does not depend on `sui-protocol-config`
/// (and transitively on the Move VM).
#[derive(Clone, Debug)]
pub struct ConsensusProtocolConfig {
    protocol_version: u64,
    chain: ChainType,
    max_transaction_size_bytes: u64,
    max_transactions_in_block_bytes: u64,
    max_num_transactions_in_block: u64,
    gc_depth: u32,
    transaction_voting_enabled: bool,
    num_leaders_per_round: Option<usize>,
    bad_nodes_stake_threshold: u64,
    enable_v3: bool,
}

impl Default for ConsensusProtocolConfig {
    fn default() -> Self {
        Self {
            protocol_version: 0,
            chain: ChainType::Unknown,
            max_transaction_size_bytes: 256 * 1024,
            max_transactions_in_block_bytes: if cfg!(msim) { 256 * 1024 } else { 512 * 1024 },
            max_num_transactions_in_block: if cfg!(msim) { 8 } else { 512 },
            gc_depth: 0,
            transaction_voting_enabled: false,
            num_leaders_per_round: None,
            bad_nodes_stake_threshold: 0,
            enable_v3: false,
        }
    }
}

impl ConsensusProtocolConfig {
    pub fn new(
        protocol_version: u64,
        chain: ChainType,
        max_transaction_size_bytes: u64,
        max_transactions_in_block_bytes: u64,
        max_num_transactions_in_block: u64,
        gc_depth: u32,
        transaction_voting_enabled: bool,
        num_leaders_per_round: Option<usize>,
        bad_nodes_stake_threshold: u64,
        enable_v3: bool,
    ) -> Self {
        Self {
            protocol_version,
            chain,
            max_transaction_size_bytes,
            max_transactions_in_block_bytes,
            max_num_transactions_in_block,
            gc_depth,
            transaction_voting_enabled,
            num_leaders_per_round,
            bad_nodes_stake_threshold,
            enable_v3,
        }
    }

    /// Returns a config with all features enabled and reasonable defaults
    /// for use in tests.
    pub fn for_testing() -> Self {
        Self {
            protocol_version: u64::MAX,
            chain: ChainType::Unknown,
            max_transaction_size_bytes: 256 * 1024,
            max_transactions_in_block_bytes: if cfg!(msim) { 256 * 1024 } else { 512 * 1024 },
            max_num_transactions_in_block: if cfg!(msim) { 8 } else { 512 },
            gc_depth: if cfg!(msim) { 6 } else { 60 },
            transaction_voting_enabled: true,
            num_leaders_per_round: Some(1),
            bad_nodes_stake_threshold: 30,
            enable_v3: false,
        }
    }

    // Getter methods

    pub fn protocol_version(&self) -> u64 {
        self.protocol_version
    }

    pub fn chain(&self) -> ChainType {
        self.chain
    }

    pub fn max_transaction_size_bytes(&self) -> u64 {
        self.max_transaction_size_bytes
    }

    pub fn max_transactions_in_block_bytes(&self) -> u64 {
        self.max_transactions_in_block_bytes
    }

    pub fn max_num_transactions_in_block(&self) -> u64 {
        self.max_num_transactions_in_block
    }

    pub fn gc_depth(&self) -> u32 {
        self.gc_depth
    }

    pub fn transaction_voting_enabled(&self) -> bool {
        self.transaction_voting_enabled
    }

    pub fn num_leaders_per_round(&self) -> Option<usize> {
        self.num_leaders_per_round
    }

    pub fn bad_nodes_stake_threshold(&self) -> u64 {
        self.bad_nodes_stake_threshold
    }

    pub fn enable_v3(&self) -> bool {
        self.enable_v3
    }

    // Test setter methods

    pub fn set_gc_depth_for_testing(&mut self, val: u32) {
        self.gc_depth = val;
    }

    pub fn set_transaction_voting_enabled_for_testing(&mut self, val: bool) {
        self.transaction_voting_enabled = val;
    }

    pub fn set_max_transaction_size_bytes_for_testing(&mut self, val: u64) {
        self.max_transaction_size_bytes = val;
    }

    pub fn set_max_transactions_in_block_bytes_for_testing(&mut self, val: u64) {
        self.max_transactions_in_block_bytes = val;
    }

    pub fn set_max_num_transactions_in_block_for_testing(&mut self, val: u64) {
        self.max_num_transactions_in_block = val;
    }

    pub fn set_bad_nodes_stake_threshold_for_testing(&mut self, val: u64) {
        self.bad_nodes_stake_threshold = val;
    }

    pub fn set_num_leaders_per_round_for_testing(&mut self, val: Option<usize>) {
        self.num_leaders_per_round = val;
    }

    pub fn set_enable_v3_for_testing(&mut self, val: bool) {
        self.enable_v3 = val;
    }
}
