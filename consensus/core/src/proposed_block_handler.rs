// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio::sync::broadcast;
use tracing::{trace, warn};

use crate::{block::ExtendedBlock, transaction_certifier::TransactionCertifier};

/// Runs async processing logic for proposed blocks.
/// Currently it only call transaction certifier with proposed blocks.
/// In future, flushing dag state before proposing can be moved here.
pub(crate) struct ProposedBlockHandler {
    rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
    transaction_certifier: TransactionCertifier,
}

impl ProposedBlockHandler {
    pub(crate) fn new(
        rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
        transaction_certifier: TransactionCertifier,
    ) -> Self {
        Self {
            rx_block_broadcast,
            transaction_certifier,
        }
    }

    pub(crate) async fn run(&mut self) {
        loop {
            match self.rx_block_broadcast.recv().await {
                Ok(extended_block) => self.handle_proposed_block(extended_block),
                Err(broadcast::error::RecvError::Closed) => {
                    trace!("Handler is shutting down!");
                    return;
                }
                Err(broadcast::error::RecvError::Lagged(e)) => {
                    warn!("Handler is lagging! {e}");
                    // Re-run the loop to receive again.
                    continue;
                }
            };
        }
    }

    fn handle_proposed_block(&self, extended_block: ExtendedBlock) {
        self.transaction_certifier
            .add_proposed_block(extended_block.block.clone());
    }
}
