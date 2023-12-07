// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug)]
pub enum BridgeError {
    // The input is not an invalid transaction digest/hash
    InvalidTxHash,
    // The referenced transaction failed
    OriginTxFailed,
    // The referenced transction does not exist
    TxNotFound,
    // The referenced transaction does not contain bridge events
    NoBridgeEventsInTx,
    // Internal Bridge error
    InternalError(String),
    // Transient Ethereum provider error
    TransientProviderError(String),
    // Uncategorized error
    Generic(anyhow::Error),
}

pub type BridgeResult<T> = Result<T, BridgeError>;
