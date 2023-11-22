// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug)]
pub enum BridgeError {
    InvalidTxHash,
    OriginTxFailed,
    TxNotFound,
    NoBridgeEventsInTx,
    InternalError(String),
    TransientProviderError(String),
    Generic(anyhow::Error),
}

pub type BridgeResult<T> = Result<T, BridgeError>;
