// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::{AuthorityIndex, Epoch, Stake};
use fastcrypto::error::FastCryptoError;
use strum_macros::IntoStaticStr;
use thiserror::Error;
use typed_store::TypedStoreError;

use crate::{
    block::{BlockRef, Round},
    commit::{Commit, CommitIndex},
};

/// Errors that can occur when processing blocks, reading from storage, or encountering shutdown.
#[derive(Clone, Debug, Error, IntoStaticStr)]
pub(crate) enum ConsensusError {
    #[error("Error deserializing block: {0}")]
    MalformedBlock(bcs::Error),

    #[error("Error deserializing commit: {0}")]
    MalformedCommit(bcs::Error),

    #[error("Error serializing: {0}")]
    SerializationFailure(bcs::Error),

    #[error("Block contains a transaction that is too large: {size} > {limit}")]
    TransactionTooLarge { size: usize, limit: usize },

    #[error("Block contains too many transactions: {count} > {limit}")]
    TooManyTransactions { count: usize, limit: usize },

    #[error("Block contains too many transaction bytes: {size} > {limit}")]
    TooManyTransactionBytes { size: usize, limit: usize },

    #[error("Unexpected block authority {0} from peer {1}")]
    UnexpectedAuthority(AuthorityIndex, AuthorityIndex),

    #[error("Block has wrong epoch: expected {expected}, actual {actual}")]
    WrongEpoch { expected: Epoch, actual: Epoch },

    #[error("Genesis blocks should only be generated from Committee!")]
    UnexpectedGenesisBlock,

    #[error("Genesis blocks should not be queried!")]
    UnexpectedGenesisBlockRequested,

    #[error(
        "Expected {requested} but received {received} blocks returned from authority {authority}"
    )]
    UnexpectedNumberOfBlocksFetched {
        authority: AuthorityIndex,
        requested: usize,
        received: usize,
    },

    #[error("Unexpected block returned while fetching missing blocks")]
    UnexpectedFetchedBlock {
        index: AuthorityIndex,
        block_ref: BlockRef,
    },

    #[error(
        "Unexpected block {block_ref} returned while fetching last own block from peer {index}"
    )]
    UnexpectedLastOwnBlock {
        index: AuthorityIndex,
        block_ref: BlockRef,
    },

    #[error("Too many blocks have been returned from authority {0} when requesting to fetch missing blocks")]
    TooManyFetchedBlocksReturned(AuthorityIndex),

    #[error("Too many blocks have been requested from authority {0}")]
    TooManyFetchBlocksRequested(AuthorityIndex),

    #[error("Too many authorities have been provided from authority {0}")]
    TooManyAuthoritiesProvided(AuthorityIndex),

    #[error("Provided size of highest accepted rounds parameter, {0}, is different than committee size, {1}")]
    InvalidSizeOfHighestAcceptedRounds(usize, usize),

    #[error("Invalid authority index: {index} > {max}")]
    InvalidAuthorityIndex { index: AuthorityIndex, max: usize },

    #[error("Failed to deserialize signature: {0}")]
    MalformedSignature(FastCryptoError),

    #[error("Failed to verify the block's signature: {0}")]
    SignatureVerificationFailure(FastCryptoError),

    #[error("Synchronizer for fetching blocks directly from {0} is saturated")]
    SynchronizerSaturated(AuthorityIndex),

    #[error("Block {block_ref:?} rejected: {reason}")]
    BlockRejected { block_ref: BlockRef, reason: String },

    #[error("Ancestor is in wrong position: block {block_authority}, ancestor {ancestor_authority}, position {position}")]
    InvalidAncestorPosition {
        block_authority: AuthorityIndex,
        ancestor_authority: AuthorityIndex,
        position: usize,
    },

    #[error("Ancestor's round ({ancestor}) should be lower than the block's round ({block})")]
    InvalidAncestorRound { ancestor: Round, block: Round },

    #[error("Ancestor {0} not found among genesis blocks!")]
    InvalidGenesisAncestor(BlockRef),

    #[error("Too many ancestors in the block: {0} > {1}")]
    TooManyAncestors(usize, usize),

    #[error("Ancestors from the same authority {0}")]
    DuplicatedAncestorsAuthority(AuthorityIndex),

    #[error("Insufficient stake from parents: {parent_stakes} < {quorum}")]
    InsufficientParentStakes { parent_stakes: Stake, quorum: Stake },

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Ancestors max timestamp {max_timestamp_ms} > block timestamp {block_timestamp_ms}")]
    InvalidBlockTimestamp {
        max_timestamp_ms: u64,
        block_timestamp_ms: u64,
    },

    #[error("Received no commit from peer {peer}")]
    NoCommitReceived { peer: AuthorityIndex },

    #[error(
        "Received unexpected start commit from peer {peer}: requested {start}, received {commit:?}"
    )]
    UnexpectedStartCommit {
        peer: AuthorityIndex,
        start: CommitIndex,
        commit: Box<Commit>,
    },

    #[error(
        "Received unexpected commit sequence from peer {peer}: {prev_commit:?}, {curr_commit:?}"
    )]
    UnexpectedCommitSequence {
        peer: AuthorityIndex,
        prev_commit: Box<Commit>,
        curr_commit: Box<Commit>,
    },

    #[error("Not enough votes ({stake}) on end commit from peer {peer}: {commit:?}")]
    NotEnoughCommitVotes {
        stake: Stake,
        peer: AuthorityIndex,
        commit: Box<Commit>,
    },

    #[error("Received unexpected block from peer {peer}: {requested:?} vs {received:?}")]
    UnexpectedBlockForCommit {
        peer: AuthorityIndex,
        requested: BlockRef,
        received: BlockRef,
    },

    #[error("RocksDB failure: {0}")]
    RocksDBFailure(#[from] TypedStoreError),

    #[error("Unknown network peer: {0}")]
    UnknownNetworkPeer(String),

    #[error("Peer {0} is disconnected.")]
    PeerDisconnected(String),

    #[error("Network config error: {0:?}")]
    NetworkConfig(String),

    #[error("Failed to connect as client: {0:?}")]
    NetworkClientConnection(String),

    #[error("Failed to send request: {0:?}")]
    NetworkRequest(String),

    #[error("Request timeout: {0:?}")]
    NetworkRequestTimeout(String),

    #[error("Consensus has shut down!")]
    Shutdown,
}

impl ConsensusError {
    /// Returns the error name - only the enun name without any parameters - as a static string.
    pub fn name(&self) -> &'static str {
        self.into()
    }
}

pub type ConsensusResult<T> = Result<T, ConsensusError>;

#[macro_export]
macro_rules! bail {
    ($e:expr) => {
        return Err($e);
    };
}

#[macro_export(local_inner_macros)]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            bail!($e);
        }
    };
}

#[cfg(test)]
mod test {
    use super::*;

    /// This test ensures that consensus errors when converted to a static string are the same as the enum name without
    /// any parameterers included to the result string.
    #[test]
    fn test_error_name() {
        {
            let error = ConsensusError::InvalidAncestorRound {
                ancestor: 10,
                block: 11,
            };
            let error: &'static str = error.into();

            assert_eq!(error, "InvalidAncestorRound");
        }

        {
            let error = ConsensusError::InvalidAuthorityIndex {
                index: AuthorityIndex::new_for_test(3),
                max: 10,
            };
            assert_eq!(error.name(), "InvalidAuthorityIndex");
        }

        {
            let error = ConsensusError::InsufficientParentStakes {
                parent_stakes: 5,
                quorum: 20,
            };
            assert_eq!(error.name(), "InsufficientParentStakes");
        }
    }
}
