// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::{AuthorityMetrics, AuthorityState};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_adapter::{BlockStatusReceiver, ConsensusClient, SubmitToConsensus};
use crate::consensus_handler::SequencedConsensusTransaction;
use consensus_core::BlockRef;
use prometheus::Registry;
use std::sync::{Arc, Weak};
use sui_types::error::SuiResult;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use sui_types::transaction::{VerifiedCertificate, VerifiedTransaction};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::debug;

pub struct MockConsensusClient {
    tx_sender: mpsc::Sender<ConsensusTransaction>,
    _consensus_handle: JoinHandle<()>,
}

pub enum ConsensusMode {
    // ConsensusClient does absolutely nothing when receiving a transaction
    Noop,
    // ConsensusClient directly sequences the transaction into the store.
    DirectSequencing,
}

impl MockConsensusClient {
    pub fn new(validator: Weak<AuthorityState>, consensus_mode: ConsensusMode) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel(1000000);
        let _consensus_handle = Self::run(validator, tx_receiver, consensus_mode);
        Self {
            tx_sender,
            _consensus_handle,
        }
    }

    pub fn run(
        validator: Weak<AuthorityState>,
        tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) -> JoinHandle<()> {
        tokio::spawn(async move { Self::run_impl(validator, tx_receiver, consensus_mode).await })
    }

    async fn run_impl(
        validator: Weak<AuthorityState>,
        mut tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) {
        let checkpoint_service = Arc::new(CheckpointServiceNoop {});
        let authority_metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        while let Some(tx) = tx_receiver.recv().await {
            let Some(validator) = validator.upgrade() else {
                debug!("validator shut down; exiting MockConsensusClient");
                return;
            };
            let epoch_store = validator.epoch_store_for_testing();
            match consensus_mode {
                ConsensusMode::Noop => {}
                ConsensusMode::DirectSequencing => {
                    epoch_store
                        .process_consensus_transactions_for_tests(
                            vec![SequencedConsensusTransaction::new_test(tx.clone())],
                            &checkpoint_service,
                            validator.get_object_cache_reader().as_ref(),
                            validator.get_transaction_cache_reader().as_ref(),
                            &authority_metrics,
                            true,
                        )
                        .await
                        .unwrap();
                }
            }
            if let ConsensusTransactionKind::CertifiedTransaction(tx) = &tx.kind {
                if tx.contains_shared_object() {
                    validator.enqueue_certificates_for_execution(
                        vec![VerifiedCertificate::new_unchecked(*tx.clone())],
                        &epoch_store,
                    );
                }
            }
            if let ConsensusTransactionKind::UserTransaction(tx) = &tx.kind {
                if tx.contains_shared_object() {
                    validator.enqueue_transactions_for_execution(
                        vec![VerifiedExecutableTransaction::new_from_consensus(
                            VerifiedTransaction::new_unchecked(*tx.clone()),
                            0,
                        )],
                        &epoch_store,
                    );
                }
            }
        }
    }

    fn submit_impl(&self, transactions: &[ConsensusTransaction]) -> SuiResult<BlockStatusReceiver> {
        // TODO: maybe support multi-transactions and remove this check
        assert!(transactions.len() == 1);
        let transaction = &transactions[0];
        self.tx_sender
            .try_send(transaction.clone())
            .expect("MockConsensusClient channel should not overflow");
        Ok(with_block_status(consensus_core::BlockStatus::Sequenced(
            BlockRef::MIN,
        )))
    }
}

impl SubmitToConsensus for MockConsensusClient {
    fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.submit_impl(transactions).map(|_response| ())
    }
}

#[async_trait::async_trait]
impl ConsensusClient for MockConsensusClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<BlockStatusReceiver> {
        self.submit_impl(transactions)
    }
}

pub(crate) fn with_block_status(status: consensus_core::BlockStatus) -> BlockStatusReceiver {
    let (tx, rx) = oneshot::channel();
    tx.send(status).ok();
    rx
}
