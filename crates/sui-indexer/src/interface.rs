// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_rest_api::{CheckpointData, Client};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[async_trait::async_trait]
pub trait Handler {
    fn name(&self) -> &str;
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()>;
}

pub trait BackfillHandler: Handler {
    fn last_processed_checkpoint(&self) -> Option<CheckpointSequenceNumber>;
    // fn has_processed_checkpoint(&self, checkpoint_sequence_number: CheckpointSequenceNumber) -> bool;
}

#[async_trait::async_trait]
pub trait OutOfOrderHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: OutOfOrderHandler> Handler for T {
    fn name(&self) -> &str {
        OutOfOrderHandler::name(self)
    }

    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        OutOfOrderHandler::process_checkpoint(self, checkpoint_data).await
    }
}

pub struct IndexerRunner {
    client: Client,
    handlers: Vec<Box<dyn Handler>>,
}

impl IndexerRunner {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            handlers: Vec::new(),
        }
    }

    pub fn add_handler<T: Handler + 'static>(&mut self, handler: T) {
        self.handlers.push(Box::new(handler));
    }

    pub async fn run(self) -> Result<()> {
        todo!()
    }
}
