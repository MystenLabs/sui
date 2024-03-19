// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::IntCounter;
use std::mem;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::AuthorityState;
use sui_core::checkpoints::CheckpointServiceNoop;
use sui_core::consensus_adapter::SubmitToConsensus;
use sui_core::consensus_handler::SequencedConsensusTransaction;
use sui_types::error::SuiResult;
use sui_types::messages_consensus::ConsensusTransaction;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) struct MockConsensusClient {
    tx_sender: mpsc::Sender<ConsensusTransaction>,
    // TODO: May want to make sure we flush all pending transactions before benchmark ends.
    _consensus_handle: JoinHandle<()>,
}

pub(crate) enum ConsensusMode {
    // ConsensusClient does absolutely nothing when receiving a transaction
    Noop,
    // ConsensusClient directly sequences the transaction into the store.
    // u64 is the consensus commit size.
    DirectSequencing(usize),
}

impl MockConsensusClient {
    pub(crate) fn new(validator: Arc<AuthorityState>, consensus_mode: ConsensusMode) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel(1000000);
        let _consensus_handle = Self::run(validator, tx_receiver, consensus_mode);
        Self {
            tx_sender,
            _consensus_handle,
        }
    }

    pub(crate) fn run(
        validator: Arc<AuthorityState>,
        tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) -> JoinHandle<()> {
        tokio::spawn(async move { Self::run_impl(validator, tx_receiver, consensus_mode).await })
    }

    async fn run_impl(
        validator: Arc<AuthorityState>,
        mut tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) {
        let epoch_store = validator.epoch_store_for_testing().clone();
        let checkpoint_service = Arc::new(CheckpointServiceNoop {});
        let counter = IntCounter::new(
            "skipped_consensus_txns",
            "Total number of consensus transactions skipped",
        )
        .unwrap();
        let mut transactions = vec![];
        while let Some(tx) = tx_receiver.recv().await {
            match consensus_mode {
                ConsensusMode::Noop => {}
                ConsensusMode::DirectSequencing(checkpoint_size) => {
                    transactions.push(SequencedConsensusTransaction::new_test(tx));
                    if transactions.len() == checkpoint_size {
                        epoch_store
                            .process_consensus_transactions_for_tests(
                                mem::take(&mut transactions),
                                &checkpoint_service,
                                validator.get_cache_reader().as_ref(),
                                &counter,
                            )
                            .await
                            .unwrap();
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for MockConsensusClient {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.tx_sender.send(transaction.clone()).await.unwrap();
        Ok(())
    }
}
