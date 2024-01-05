// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::Block;
use async_trait::async_trait;

/// The interfaces to verify the legitimacy of a statement block's contents.
#[async_trait]
pub trait BlockVerifier: Send + Sync + 'static {
    type Error: std::fmt::Display + std::fmt::Debug + Send + Sync + 'static;
    /// Determines if a statement block's content is valid.
    async fn verify(&self, _b: &Block) -> Result<(), Self::Error>;

    async fn verify_all(&self, _b: &[Block]) -> Result<(), Self::Error>;
}

#[derive(Clone)]
pub struct TestBlockVerifier;

#[async_trait]
impl BlockVerifier for TestBlockVerifier {
    type Error = String;

    async fn verify(&self, _b: &Block) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn verify_all(&self, _b: &[Block]) -> Result<(), Self::Error> {
        Ok(())
    }
}
