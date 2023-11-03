// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysticeti_core::block_handler::BlockHandler;
use mysticeti_core::minibytes;
use mysticeti_core::types::{BaseStatement, StatementBlock};

use sui_types::error::{SuiError, SuiResult};
use tap::prelude::*;
use tokio::sync::{mpsc, oneshot};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::SubmitToConsensus;
use mysticeti_core::data::Data;
use sui_types::messages_consensus::ConsensusTransaction;
use tracing::warn;

#[derive(Clone)]
pub struct SubmitToMysticeti {
    // channel to transport bcs-serialized bytes of ConsensusTransaction
    sender: mpsc::Sender<(Vec<u8>, oneshot::Sender<()>)>,
}

impl SubmitToMysticeti {
    pub fn new(sender: mpsc::Sender<(Vec<u8>, oneshot::Sender<()>)>) -> SubmitToMysticeti {
        SubmitToMysticeti { sender }
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for SubmitToMysticeti {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let (sender, receiver) = oneshot::channel();
        let tx_bytes = bcs::to_bytes(&transaction).expect("Serialization should not fail.");
        self.sender
            .send((tx_bytes, sender))
            .await
            .tap_err(|e| warn!("Submit transaction failed with {:?}", e))
            .map_err(|e| SuiError::FailedToSubmitToConsensus(format!("{:?}", e)))?;
        // Give a little bit backpressure if BlockHandler is not able to keep up.
        receiver
            .await
            .tap_err(|e| warn!("Block Handler failed to ack: {:?}", e))
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
    }
}

/// A simple BlockHandler that adds received transactions to consensus.
pub struct SimpleBlockHandler {
    receiver: mysten_metrics::metered_channel::Receiver<(Vec<u8>, oneshot::Sender<()>)>,
}

const MAX_PROPOSED_PER_BLOCK: usize = 10000;
const CHANNEL_SIZE: usize = 10240;

impl SimpleBlockHandler {
    #[allow(clippy::type_complexity)]
    pub fn new() -> (
        Self,
        mysten_metrics::metered_channel::Sender<(Vec<u8>, oneshot::Sender<()>)>,
    ) {
        let (sender, receiver) = mysten_metrics::metered_channel::channel(
            CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channels
                .with_label_values(&["simple_block_handler"]),
        );

        let this = Self { receiver };
        (this, sender)
    }
}

impl BlockHandler for SimpleBlockHandler {
    fn handle_blocks(
        &mut self,
        _blocks: &[Data<StatementBlock>],
        require_response: bool,
    ) -> Vec<BaseStatement> {
        if !require_response {
            return vec![];
        }

        // Returns transactions to be sequenced so that they will be
        // proposed to DAG shortly.
        let mut response = vec![];

        while let Ok((tx_bytes, notify_when_done)) = self.receiver.try_recv() {
            response.push(BaseStatement::Share(
                // tx_bytes is bcs-serialized bytes of ConsensusTransaction
                mysticeti_core::types::Transaction::new(tx_bytes),
            ));
            // We don't mind if the receiver is dropped.
            let _ = notify_when_done.send(());

            if response.len() >= MAX_PROPOSED_PER_BLOCK {
                break;
            }
        }
        response
    }

    fn handle_proposal(&mut self, _block: &Data<StatementBlock>) {}

    // No crash recovery at the moment.
    fn state(&self) -> minibytes::Bytes {
        minibytes::Bytes::new()
    }

    // No crash recovery at the moment.
    fn recover_state(&mut self, _state: &minibytes::Bytes) {}

    fn cleanup(&self) {}
}
