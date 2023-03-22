// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use std::fmt::Debug;
use store::StoreError;
use thiserror::Error;
use types::CertificateDigest;

#[macro_export]
macro_rules! bail {
    ($e:expr) => {
        return Err($e)
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

pub type SubscriberResult<T> = Result<T, SubscriberError>;

#[derive(Debug, Error, Clone)]
pub enum SubscriberError {
    #[error("channel {0} closed unexpectedly")]
    ClosedChannel(String),

    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Error occurred while retrieving certificate {0} payload: {1}")]
    PayloadRetrieveError(CertificateDigest, String),

    #[error("Consensus referenced unexpected worker id {0}")]
    UnexpectedWorkerId(WorkerId),

    #[error("Connection with the transaction executor dropped")]
    ExecutorConnectionDropped,

    #[error("Deserialization of consensus message failed: {0}")]
    SerializationError(String),

    #[error("Received unexpected protocol message from consensus")]
    UnexpectedProtocolMessage,

    #[error("There can only be a single consensus client at the time")]
    OnlyOneConsensusClientPermitted,

    #[error("Execution engine failed: {0}")]
    NodeExecutionError(String),

    #[error("Client transaction invalid: {0}")]
    ClientExecutionError(String),
}

impl From<Box<bcs::Error>> for SubscriberError {
    fn from(e: Box<bcs::Error>) -> Self {
        Self::SerializationError(e.to_string())
    }
}

impl From<Box<bincode::ErrorKind>> for SubscriberError {
    fn from(e: Box<bincode::ErrorKind>) -> Self {
        Self::SerializationError(e.to_string())
    }
}
