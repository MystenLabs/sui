// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use consensus_config::{AuthorityIndex, Epoch, Stake};
use fastcrypto::error::FastCryptoError;
use thiserror::Error;
use typed_store::TypedStoreError;

use crate::block::{BlockRef, BlockTimestampMs, Round};

/// Errors that can occur when processing blocks, reading from storage, or encountering shutdown.
#[derive(Clone, Debug, Error)]
pub enum ConsensusError {
    #[error("Error deserializing block: {0}")]
    MalformedBlock(bcs::Error),

    #[error("Error deserializing commit: {0}")]
    MalformedCommit(bcs::Error),

    #[error("Error serializing: {0}")]
    SerializationFailure(bcs::Error),

    #[error("Unexpected block authority {0} from peer {1}")]
    UnexpectedAuthority(AuthorityIndex, AuthorityIndex),

    #[error("Block has wrong epoch: expected {expected}, actual {actual}")]
    WrongEpoch { expected: Epoch, actual: Epoch },

    #[error("Genesis blocks should only be generated from Committee!")]
    UnexpectedGenesisBlock,

    #[error("Genesis blocks should not be queried!")]
    UnexpectedGenesisBlockRequested,

    #[error("Unexpected block returned while fetching missing blocks")]
    UnexpectedFetchedBlock {
        index: AuthorityIndex,
        block_ref: BlockRef,
    },

    #[error("Too many blocks have been returned from authority {0} when requesting to fetch missing blocks")]
    TooManyFetchedBlocksReturned(AuthorityIndex),

    #[error("Too many blocks have been requested from authority {0}")]
    TooManyFetchBlocksRequested(AuthorityIndex),

    #[error("Invalid authority index: {index} > {max}")]
    InvalidAuthorityIndex { index: AuthorityIndex, max: usize },

    #[error("Failed to deserialize signature: {0}")]
    MalformedSignature(FastCryptoError),

    #[error("Failed to verify the block's signature: {0}")]
    SignatureVerificationFailure(FastCryptoError),

    #[error("Synchronizer for fetching blocks directly from {0} is saturated")]
    SynchronizerSaturated(AuthorityIndex),

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

    #[error("Block at {block_timestamp}ms is too far in the future: {forward_time_drift:?}")]
    BlockTooFarInFuture {
        block_timestamp: BlockTimestampMs,
        forward_time_drift: Duration,
    },

    #[error("RocksDB failure: {0}")]
    RocksDBFailure(#[from] TypedStoreError),

    #[error("Unknown network peer: {0}")]
    UnknownNetworkPeer(String),

    #[error("Peer {0} is disconnected.")]
    PeerDisconnected(String),

    #[error("Network error: {0:?}")]
    NetworkError(String),

    #[error("Consensus has shut down!")]
    Shutdown,
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
