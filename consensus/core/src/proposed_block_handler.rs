// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::monitored_scope;
use tokio::sync::broadcast;
use tracing::{trace, warn};

use crate::{block::ExtendedBlock, context::Context, transaction_certifier::TransactionCertifier};

/// Runs async processing logic for proposed blocks.
/// Currently it only call transaction certifier with proposed blocks.
/// In future, more logic related to proposing should be moved here, for example
/// flushing dag state.
pub(crate) struct ProposedBlockHandler {
    context: Arc<Context>,
    rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
    transaction_certifier: TransactionCertifier,
}

impl ProposedBlockHandler {
    pub(crate) fn new(
        context: Arc<Context>,
        rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
        transaction_certifier: TransactionCertifier,
    ) -> Self {
        Self {
            context,
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
        if !self.context.protocol_config.mysticeti_fastpath() {
            return;
        }
        let _scope = monitored_scope("handle_proposed_block");
        self.transaction_certifier
            .add_proposed_block(extended_block.block.clone());
    }
}
