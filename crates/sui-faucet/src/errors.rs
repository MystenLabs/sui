// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum FaucetError {
    #[error("Faucet cannot read objects from fullnode")]
    FullnodeReadingError,

    #[error(
        "Gas coin `{0}` does not have sufficient balance and has been removed from gas coin pool"
    )]
    GasCoinWithInsufficientBalance(String),

    #[error("Faucet does not have enough balance")]
    InsuffientBalance,

    #[error("Gas coin `{0}` is not valid and has been removed from gas coin pool")]
    InvalidGasCoin(String),

    #[error("No gas coin available in the gas coin pool")]
    NoGasCoinAvailable,

    #[error("Wallet Error: `{0}`")]
    Wallet(String),

    #[error("Coin Transfer Failed `{0}`")]
    Transfer(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
