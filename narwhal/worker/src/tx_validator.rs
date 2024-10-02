// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Debug, Display};

use sui_protocol_config::ProtocolConfig;
use types::Batch;

/// Defines the validation procedure for receiving either a new single transaction (from a client)
/// of a batch of transactions (from another validator). Invalid transactions will not receive
/// further processing.
pub trait TransactionValidator: Clone + Send + Sync + 'static {
    type Error: Display + Debug + Send + Sync + 'static;
    /// Determines if a transaction valid for the worker to consider putting in a batch
    fn validate(&self, t: &[u8]) -> Result<(), Self::Error>;
    /// Determines if this batch can be voted on
    fn validate_batch(
        &self,
        b: &Batch,
        protocol_config: &ProtocolConfig,
    ) -> Result<(), Self::Error>;
}

/// Simple validator that accepts all transactions and batches.
#[derive(Debug, Clone, Default)]
pub struct TrivialTransactionValidator;

impl TransactionValidator for TrivialTransactionValidator {
    type Error = eyre::Report;

    fn validate(&self, _t: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate_batch(
        &self,
        _b: &Batch,
        _protocol_config: &ProtocolConfig,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
