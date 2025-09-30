// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

pub type ProofResult<T> = Result<T, ProofError>;

/// Errors that can occur during proof construction and verification.
#[derive(Debug, Error)]
pub enum ProofError {
    #[error("Given committee does not match the end of epoch committee")]
    CommitteeMismatch,

    #[error("Targets and proof type do not match")]
    MismatchedTargetAndProofType,

    #[error("No targets found")]
    NoTargetsFound,

    #[error("Transaction not found")]
    TransactionNotFound,

    #[error("All targets must refer to the same transaction")]
    MultipleTransactionsNotSupported,

    #[error("Object reference does not match the object")]
    ObjectReferenceMismatch,

    #[error("Object not found")]
    ObjectNotFound,

    #[error("Events digest does not match with effects")]
    EventsDigestMismatch,

    #[error("Event does not belong to the transaction")]
    EventTransactionMismatch,

    #[error("Event sequence number out of bounds")]
    EventSequenceOutOfBounds,

    #[error("Event contents do not match")]
    EventContentsMismatch,

    #[error("Events are missing from the transaction")]
    EventsMissing,

    #[error("Contents digest does not match the checkpoint summary")]
    ContentsDigestMismatch,

    #[error("Transaction digest does not match with execution digest")]
    TransactionDigestMismatch,

    #[error("Transaction digest not found in the checkpoint contents")]
    TransactionDigestNotFound,

    #[error("Epoch overflow when calculating next epoch")]
    EpochAddOverflow,

    #[error("Epoch mismatch between checkpoint and committee")]
    EpochMismatch,

    #[error("Expected end of epoch checkpoint")]
    ExpectedEndOfEpochCheckpoint,

    #[error("Checkpoint summary verification failed: {0}")]
    SummaryVerificationFailed(String),

    #[error("Invalid proof")]
    InvalidProof,

    #[error("Artifact digest mismatch")]
    ArtifactDigestMismatch,

    #[error("General error: {0}")]
    GeneralError(String),
}
