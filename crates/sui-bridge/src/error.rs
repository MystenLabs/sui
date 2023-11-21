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

// impl From<ProviderError> for BridgeError {
//     fn from(e: ProviderError) -> Self {
//         match e {
//             ProviderError::JsonRpcClientError(..) | ProviderError::HTTPError(..) =>
//                 BridgeError::TransientProviderError(format!(
//                     "Transient Eth Provider error: {:?}",
//                     e
//                 )),
//             _ => BridgeError::InternalError(format!(
//                     "Eth Provider error: {:?}",
//                     e
//                 )),
//         }
//     }
// }
