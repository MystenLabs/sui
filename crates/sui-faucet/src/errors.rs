// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum FaucetError {
    #[error("Missing X-Turnstile-Token header. For testnet tokens, please use the Web UI: https://faucet.sui.io")]
    MissingTurnstileTokenHeader,

    #[error("Request limit exceeded. {0}")]
    TooManyRequests(String),

    #[error("Faucet cannot read objects from fullnode: {0}")]
    FullnodeReadingError(String),

    #[error("Failed to parse transaction response {0}")]
    ParseTransactionResponseError(String),

    #[error(
        "Gas coin `{0}` does not have sufficient balance and has been removed from gas coin pool"
    )]
    GasCoinWithInsufficientBalance(String),

    #[error("Faucet does not have enough balance")]
    InsuffientBalance,

    #[error("Gas coin `{0}` is not valid and has been removed from gas coin pool")]
    InvalidGasCoin(String),

    #[error("Timed out waiting for a coin from the gas coin pool")]
    NoGasCoinAvailable,

    #[error("Wallet Error: `{0}`")]
    Wallet(String),

    #[error("Coin Transfer Failed `{0}`")]
    Transfer(String),

    #[error("Too many coins in the batch queue. Please try again later.")]
    BatchSendQueueFull,

    #[error("Request consumer queue closed.")]
    ChannelClosed,

    #[error("Coin amounts sent are incorrect:`{0}`")]
    CoinAmountTransferredIncorrect(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid user agent: {0}")]
    InvalidUserAgent(String),
}

impl FaucetError {
    pub(crate) fn internal(e: impl ToString) -> Self {
        FaucetError::Internal(e.to_string())
    }
}
