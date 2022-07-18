// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use std::fmt::Debug;
use store::StoreError;
use thiserror::Error;
use types::SequenceNumber;

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
    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Consensus referenced unexpected worker id {0}")]
    UnexpectedWorkerId(WorkerId),

    #[error("Unexpected consensus index number {0}")]
    UnexpectedConsensusIndex(SequenceNumber),

    #[error("The client-consensus connection dropped")]
    ConsensusConnectionDropped,

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

impl From<Box<bincode::ErrorKind>> for SubscriberError {
    fn from(e: Box<bincode::ErrorKind>) -> Self {
        Self::SerializationError(e.to_string())
    }
}

/// Trait do separate execution errors in two categories: (i) errors caused by a bad client, (ii)
/// errors caused by a fault in the authority.
pub trait ExecutionStateError: Debug {
    /// Whether the error is due to a fault in the authority (eg. internal storage error).
    fn node_error(&self) -> bool;

    /// Convert the error message in to a string.
    fn to_string(&self) -> String;
}

impl<T: ExecutionStateError> From<T> for SubscriberError {
    fn from(e: T) -> Self {
        match e.node_error() {
            true => Self::NodeExecutionError(e.to_string()),
            false => Self::ClientExecutionError(e.to_string()),
        }
    }
}
