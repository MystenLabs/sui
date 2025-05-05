// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, Error, Debug, PartialEq, Eq)]
pub enum FaucetError {
    #[error("Wallet Error: `{0}`")]
    Wallet(String),

    #[error("Coin Transfer Failed `{0}`")]
    Transfer(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl FaucetError {
    pub(crate) fn internal(e: impl ToString) -> Self {
        FaucetError::Internal(e.to_string())
    }
}
