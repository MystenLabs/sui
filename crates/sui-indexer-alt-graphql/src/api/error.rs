// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;

/// Error type for user input validation in transaction operations
#[derive(thiserror::Error, Debug)]
pub enum TransactionInputError {
    #[error("Invalid BCS encoding in transaction data: {0}")]
    InvalidTransactionBcs(bcs::Error),

    #[error("Invalid signature format in signature {index}: {err}")]
    InvalidSignatureFormat { index: usize, err: FastCryptoError },
}
