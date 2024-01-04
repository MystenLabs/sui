// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::Block;
use async_trait::async_trait;

/// The interfaces to validate the legitimacy of a statement block's contents.
#[async_trait]
pub trait BlockValidator: Send + Sync + 'static {
    type Error: std::fmt::Display + std::fmt::Debug + Send + Sync + 'static;
    /// Determines if a statement block's content is valid.
    async fn validate(&self, _b: &Block) -> Result<(), Self::Error>;

    async fn validate_all(&self, _b: &[Block]) -> Result<(), Self::Error>;
}

#[derive(Clone)]
pub struct AcceptAllBlockValidator;

#[async_trait]
impl BlockValidator for AcceptAllBlockValidator {
    type Error = eyre::Report;

    async fn validate(&self, _b: &Block) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn validate_all(&self, _b: &[Block]) -> Result<(), Self::Error> {
        Ok(())
    }
}
