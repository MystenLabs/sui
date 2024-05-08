// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::{AuthorityMetrics, AuthorityState};
use sui_core::checkpoints::CheckpointServiceNoop;
use sui_core::consensus_adapter::SubmitToConsensus;
use sui_core::consensus_handler::SequencedConsensusTransaction;
use sui_types::error::SuiResult;
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use sui_types::transaction::VerifiedCertificate;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) struct MockConsensusClient {
    tx_sender: mpsc::Sender<ConsensusTransaction>,
    _consensus_handle: JoinHandle<()>,
}

pub(crate) enum ConsensusMode {
    // ConsensusClient does absolutely nothing when receiving a transaction
    Noop,
    // ConsensusClient directly sequences the transaction into the store.
    DirectSequencing,
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
        let checkpoint_service = Arc::new(CheckpointServiceNoop {});
        let epoch_store = validator.epoch_store_for_testing();
        let authority_metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        while let Some(tx) = tx_receiver.recv().await {
            match consensus_mode {
                ConsensusMode::Noop => {}
                ConsensusMode::DirectSequencing => {
                    epoch_store
                        .process_consensus_transactions_for_tests(
                            vec![SequencedConsensusTransaction::new_test(tx.clone())],
                            &checkpoint_service,
                            validator.get_cache_reader().as_ref(),
                            &authority_metrics,
                        )
                        .await
                        .unwrap();
                }
            }
            let tx = match tx.kind {
                ConsensusTransactionKind::UserTransaction(tx) => tx,
                _ => unreachable!("Only user transactions are supported in benchmark"),
            };
            if tx.contains_shared_object() {
                validator.enqueue_certificates_for_execution(
                    vec![VerifiedCertificate::new_unchecked(*tx)],
                    &epoch_store,
                );
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
