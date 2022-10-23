// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{HeaderDigest, Round};
use config::Epoch;
use fastcrypto::hash::Digest;
use store::StoreError;
use thiserror::Error;

#[cfg(test)]
#[path = "./tests/error_test.rs"]
mod error_test;

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

pub type DagResult<T> = Result<T, DagError>;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("Channel {0} has closed unexpectedly")]
    ClosedChannel(String),

    #[error("Invalid Authorities Bitmap: {0}")]
    InvalidBitmap(String),

    #[error("Invalid signature")]
    InvalidSignature(#[from] signature::Error),

    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] Box<bincode::ErrorKind>),

    #[error("Invalid header id")]
    InvalidHeaderId,

    #[error("Malformed header {0}")]
    MalformedHeader(HeaderDigest),

    #[error("Received message from unknown authority {0}")]
    UnknownAuthority(String),

    #[error("Authority {0} appears in quorum more than once")]
    AuthorityReuse(String),

    #[error("Received unexpected vote for header {0}")]
    UnexpectedVote(HeaderDigest),

    #[error("Received certificate without a quorum")]
    CertificateRequiresQuorum,

    #[error("Parents of header {0} are not a quorum")]
    HeaderRequiresQuorum(HeaderDigest),

    #[error("Message {0} (round {1}) too old for GC round {2}")]
    TooOld(Digest<{ crypto::DIGEST_LENGTH }>, Round, Round),

    #[error("Message {0} (round {1}) is too new for this primary at round {2}")]
    TooNew(Digest<{ crypto::DIGEST_LENGTH }>, Round, Round),

    #[error("Vote {0} (round {1}) too old for round {2}")]
    VoteTooOld(Digest<{ crypto::DIGEST_LENGTH }>, Round, Round),

    #[error("Invalid epoch (expected {expected}, received {received})")]
    InvalidEpoch { expected: Epoch, received: Epoch },

    #[error("Too many certificates in the FetchCertificatesResponse {0} > {1}")]
    TooManyFetchedCertificatesReturned(usize, usize),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("System shutting down")]
    ShuttingDown,

    #[error("Channel Full")]
    ChannelFull,
}

impl<T> From<tokio::sync::mpsc::error::TrySendError<T>> for DagError {
    fn from(err: tokio::sync::mpsc::error::TrySendError<T>) -> Self {
        match err {
            tokio::sync::mpsc::error::TrySendError::Full(_) => DagError::ChannelFull,
            tokio::sync::mpsc::error::TrySendError::Closed(_) => DagError::ShuttingDown,
        }
    }
}
