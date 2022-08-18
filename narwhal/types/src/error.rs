// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{HeaderDigest, Round};
use config::Epoch;
use crypto::Digest;
use store::StoreError;
use thiserror::Error;

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

#[macro_export]
macro_rules! try_fut_and_permit {
    ($fut:expr, $sender:expr) => {
        futures::future::TryFutureExt::unwrap_or_else(
            futures::future::try_join(
                $fut,
                futures::TryFutureExt::map_err($sender.reserve(), |_e| {
                    DagError::ClosedChannel(stringify!(sender).to_owned())
                }),
            ),
            |e| {
                tracing::error!("{e}");
                panic!("I/O failure, killing the node.");
            },
        )
    };
}

pub type DagResult<T> = Result<T, DagError>;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("Channel {0} has closed unexpectedly")]
    ClosedChannel(String),

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

    #[error("Received unexpected vote fo header {0}")]
    UnexpectedVote(HeaderDigest),

    #[error("Received certificate without a quorum")]
    CertificateRequiresQuorum,

    #[error("Parents of header {0} are not a quorum")]
    HeaderRequiresQuorum(HeaderDigest),

    #[error("Message {0} (round {1}) too old for GC round {2}")]
    TooOld(Digest, Round, Round),

    #[error("Vote {0} (round {1}) too old for round {2}")]
    VoteTooOld(Digest, Round, Round),

    #[error("Invalid epoch (expected {expected}, received {received})")]
    InvalidEpoch { expected: Epoch, received: Epoch },

    #[error("System shutting down")]
    ShuttingDown,
}
