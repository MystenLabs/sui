// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum FaucetError {
    #[error("Faucet does not have enough balance")]
    InsuffientBalance,

    #[error("Internal error: {0}")]
    Internal(String),
}
